//! Core engine that manages local model configurations, health checking, and inference
//! dispatch. Analogous to [`crate::sandbox::SandboxEngine`] / [`crate::voice::VoicePipelineEngine`].

#![forbid(unsafe_code)]

use crate::error::{HubError, Result};
use crate::local_llm::inference::{InferenceOptions, InferenceRequest, LocalInferenceClient};
use crate::models::{LocalModelConfig, LocalModelHealth, ModelInfo};
use std::sync::Arc;
use std::sync::Mutex;

impl Default for LocalModelEngine {
    fn default() -> Self {
        Self::new()
    }
}

/// Thread-safe store of local model configurations with health checking.
pub struct LocalModelEngine {
    configs: Mutex<Vec<LocalModelConfig>>,
    models: Arc<Mutex<Vec<ModelInfo>>>,
    http_client: LocalInferenceClient,
}

impl LocalModelEngine {
    /// Create a new engine with no configured endpoints.
    pub fn new() -> Self {
        Self {
            configs: Mutex::new(Vec::new()),
            models: Arc::new(Mutex::new(Vec::new())),
            http_client: LocalInferenceClient::new(),
        }
    }

    /// Add a configuration for a local model endpoint.
    pub fn add_config(&self, config: LocalModelConfig) -> Result<()> {
        let mut configs = self
            .configs
            .lock()
            .map_err(|_| HubError::Internal("local-llm engine mutex poisoned".into()))?;
        // Reject duplicates by model name.
        if configs.iter().any(|c| c.model_name == config.model_name) {
            return Err(HubError::InvalidInput(format!(
                "config already exists for model '{}'",
                config.model_name
            )));
        }
        configs.push(config);
        Ok(())
    }

    /// Remove a configuration by model name. Returns the removed config if found.
    pub fn remove_config(&self, model_name: &str) -> Option<LocalModelConfig> {
        let mut configs = self.configs.lock().ok()?;
        let idx = configs.iter().position(|c| c.model_name == model_name)?;
        Some(configs.remove(idx))
    }

    /// Return a reference to all configured local models.
    pub fn get_configs(&self) -> Vec<LocalModelConfig> {
        self.configs.lock().map(|c| c.clone()).unwrap_or_default()
    }

    /// Probe the health of each configured endpoint and return (model_name, health).
    pub async fn refresh_health(&self) -> Vec<(String, LocalModelHealth)> {
        let configs = self.configs.lock();
        let configs = match configs {
            Ok(c) => c.clone(),
            Err(_) => return Vec::new(),
        };

        let mut results = Vec::new();
        for config in configs {
            let health = self.http_client.health_check(&config.base_url).await;
            match health {
                Ok(h) => results.push((config.model_name.clone(), h)),
                Err(_) => {
                    results.push((config.model_name, LocalModelHealth::Unavailable));
                }
            }
        }
        results
    }

    /// List models available on the first configured endpoint. Returns empty vec if none.
    pub async fn list_models(&self) -> Result<Vec<ModelInfo>> {
        let first = {
            let configs = self
                .configs
                .lock()
                .map_err(|_| HubError::Internal("local-llm engine mutex poisoned".into()))?;
            if configs.is_empty() {
                return Ok(Vec::new());
            }
            configs[0].clone()
        };

        let models = self
            .http_client
            .list_models(&first.base_url, first.provider.clone())
            .await?;

        // Update the cached model registry.
        if let Ok(mut cache) = self.models.lock() {
            *cache = models.clone();
        }

        Ok(models)
    }

    /// Build and dispatch an inference request using the first configured model.
    ///
    /// Returns raw JSON string from the provider — caller deserializes as needed.
    pub async fn generate(
        &self,
        prompt: &str,
        options: Option<InferenceOptions>,
    ) -> Result<String> {
        let config = {
            let configs = self
                .configs
                .lock()
                .map_err(|_| HubError::Internal("local-llm engine mutex poisoned".into()))?;

            if configs.is_empty() {
                return Err(HubError::InvalidInput(
                    "no local model configured — call configure_local_model first".into(),
                ));
            }

            configs[0].clone()
        };

        let req = InferenceRequest {
            model: config.model_name.clone(),
            prompt: prompt.to_string(),
            stream: false,
            options,
        };

        self.http_client
            .generate(&config.base_url, config.provider.clone(), req)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::LocalProviderKind;

    /// Engine starts with zero configs and an initialized http client.
    #[test]
    fn test_engine_new_defaults() {
        let engine = LocalModelEngine::new();
        assert!(engine.get_configs().is_empty());
    }

    /// Add a config → get_configs returns it; remove → empty again.
    #[test]
    fn test_add_and_remove_config() {
        let engine = LocalModelEngine::new();
        let config = LocalModelConfig::new(
            LocalProviderKind::Ollama,
            "http://localhost:11434",
            "llama3.2",
        );
        assert!(engine.add_config(config.clone()).is_ok());

        let configs = engine.get_configs();
        assert_eq!(configs.len(), 1);
        assert_eq!(configs[0].model_name, "llama3.2");

        // Remove returns Some with the config.
        let removed = engine.remove_config("llama3.2");
        assert!(removed.is_some());
        assert_eq!(removed.unwrap().model_name, "llama3.2");

        // After removal, configs is empty again.
        assert!(engine.get_configs().is_empty());
    }

    /// add_config rejects duplicate model names.
    #[test]
    fn test_duplicate_rejected() {
        let engine = LocalModelEngine::new();
        let c1 = LocalModelConfig::new(
            LocalProviderKind::Ollama,
            "http://localhost:11434",
            "mistral",
        );
        assert!(engine.add_config(c1).is_ok());

        let c2 = LocalModelConfig::new(
            LocalProviderKind::Llamafile,
            "http://localhost:8080",
            "mistral",
        );
        assert!(engine.add_config(c2).is_err());
    }

    /// Health check returns empty Vec when no configs exist (no panics).
    #[tokio::test]
    async fn test_health_check_no_configs() {
        let engine = LocalModelEngine::new();
        let results = engine.refresh_health().await;
        assert!(results.is_empty());
    }

    /// list_models on no configured endpoint returns empty Vec (not error).
    #[tokio::test]
    async fn test_list_models_empty() {
        let engine = LocalModelEngine::new();
        let models = engine.list_models().await.unwrap();
        assert!(models.is_empty());
    }

    /// Config builder produces expected defaults.
    #[test]
    fn test_default_config_values() {
        let config = LocalModelConfig::new(
            LocalProviderKind::Ollama,
            "http://localhost:11434",
            "llama3.2",
        );
        assert_eq!(config.temperature, 0.7);
        assert_eq!(config.top_p, 0.9);
        assert_eq!(config.max_tokens, 2048);
    }

    /// Config builder methods override defaults correctly.
    #[test]
    fn test_config_builders() {
        let config = LocalModelConfig::new(
            LocalProviderKind::Ollama,
            "http://localhost:11434",
            "mistral",
        )
        .with_temperature(1.0)
        .with_top_p(0.5)
        .with_max_tokens(1024);

        assert_eq!(config.temperature, 1.0);
        assert_eq!(config.top_p, 0.5);
        assert_eq!(config.max_tokens, 1024);
    }

    /// generate returns error when no config is registered.
    #[tokio::test]
    async fn test_generate_no_config_error() {
        let engine = LocalModelEngine::new();
        let result = engine.generate("hello", None).await;
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("no local model configured")
        );
    }

    /// Multiple configs can be added and get_configs returns them all.
    #[test]
    fn test_multiple_configs() {
        let engine = LocalModelEngine::new();
        let c1 = LocalModelConfig::new(
            LocalProviderKind::Ollama,
            "http://localhost:11434",
            "llama3.2",
        );
        let c2 = LocalModelConfig::new(
            LocalProviderKind::Llamafile,
            "http://localhost:8080",
            "mistral",
        );
        assert!(engine.add_config(c1).is_ok());
        assert!(engine.add_config(c2).is_ok());

        let configs = engine.get_configs();
        assert_eq!(configs.len(), 2);
    }
}
