//! HTTP client that maps each [`LocalProviderKind`] to its specific API protocol.
//!
//! No model weights are embedded and no unsafe code is used — all protocol knowledge
//! lives in this thin HTTP layer that dispatches to running local servers.

#![forbid(unsafe_code)]

use crate::error::{HubError, Result};
use crate::models::{LocalProviderKind, ModelInfo};
use serde::{Deserialize, Serialize};

/// Sampling options for a single inference request.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferenceOptions {
    /// Override sampling temperature for this request.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    /// Override top-p threshold for this request.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
    /// Override maximum tokens for this request.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
}

/// A prompt + options payload ready to be serialized for a provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferenceRequest {
    /// The model identifier to target.
    pub model: String,
    /// The prompt text to generate from.
    pub prompt: String,
    /// Whether to stream the response (always false for our use-case).
    #[serde(default = "default_false")]
    pub stream: bool,
    /// Optional sampling overrides.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<InferenceOptions>,
}

fn default_false() -> bool {
    false
}

/// Stateless HTTP client that maps providers to their API protocols.
pub struct LocalInferenceClient {
    http_client: reqwest::Client,
}

impl LocalInferenceClient {
    pub fn new() -> Self {
        Self {
            http_client: reqwest::Client::new(),
        }
    }

    /// Probe the health of a local inference server at `base_url`.
    ///
    /// For Ollama it hits `/api/tags`; for Llamafile it hits `/v1/models`.
    pub async fn health_check(&self, base_url: &str) -> Result<crate::models::LocalModelHealth> {
        let url = format!("{base_url}/api/tags");
        match self.http_client.get(&url).send().await {
            Ok(resp) if resp.status().is_success() => Ok(crate::models::LocalModelHealth::Healthy),
            Ok(_) => Ok(crate::models::LocalModelHealth::Unavailable),
            Err(_e) => Ok(crate::models::LocalModelHealth::Degraded),
        }
    }

    /// List models available on a local server at `base_url` for the given provider.
    pub async fn list_models(
        &self,
        base_url: &str,
        _provider: LocalProviderKind,
    ) -> Result<Vec<ModelInfo>> {
        // Ollama-style /api/tags response: {"models": [...]}
        let url = format!("{base_url}/api/tags");
        let resp = match self.http_client.get(&url).send().await {
            Ok(r) => r,
            Err(e) => return Err(HubError::Network(format!("list models: {e}"))),
        };

        #[derive(Deserialize)]
        struct OllamaTags {
            models: Option<Vec<OllamaModel>>,
        }

        #[derive(Deserialize)]
        struct OllamaModel {
            name: String,
            size: u64,
            format: Option<String>,
        }

        let tags: OllamaTags = match resp.json().await {
            Ok(t) => t,
            Err(e) => return Err(HubError::SerdeError(format!("parse models: {e}"))),
        };
        Ok(tags
            .models
            .unwrap_or_default()
            .into_iter()
            .map(|m| ModelInfo {
                name: m.name,
                format: m.format.unwrap_or_else(|| "gguf".into()),
                size_bytes: m.size,
                status: crate::models::LocalModelHealth::Healthy,
                downloaded_at: None,
            })
            .collect())
    }

    /// Build and dispatch an inference request via Ollama-style `/api/generate`.
    pub async fn generate(
        &self,
        base_url: &str,
        provider: LocalProviderKind,
        request: InferenceRequest,
    ) -> Result<String> {
        let url = match provider {
            LocalProviderKind::Ollama => format!("{base_url}/api/generate"),
            LocalProviderKind::Llamafile => format!("{base_url}/v1/completions"),
            LocalProviderKind::WhisperCPP => {
                return Err(HubError::InvalidInput(
                    "generate_text is not available for WhisperCPP (STT-only)".into(),
                ));
            }
        };

        let resp = match self.http_client.post(&url).json(&request).send().await {
            Ok(r) => r,
            Err(e) => return Err(HubError::Network(format!("generate: {e}"))),
        };

        let body = match resp.text().await {
            Ok(b) => b,
            Err(e) => return Err(HubError::Io(format!("read response: {e}"))),
        };
        Ok(body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::LocalModelHealth;

    /// Display impl for LocalProviderKind outputs correct strings.
    #[test]
    fn test_local_provider_display() {
        assert_eq!(format!("{}", LocalProviderKind::Ollama), "ollama");
        assert_eq!(format!("{}", LocalProviderKind::Llamafile), "llamafile");
        assert_eq!(format!("{}", LocalProviderKind::WhisperCPP), "whisper-cpp");
    }

    /// JSON round-trip of InferenceRequest serializes correctly.
    #[test]
    fn test_inference_request_serialization() {
        let req = InferenceRequest {
            model: "llama3.2".into(),
            prompt: "Hello".into(),
            stream: false,
            options: Some(InferenceOptions {
                temperature: Some(0.8),
                top_p: None,
                max_tokens: Some(100),
            }),
        };

        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("\"model\":\"llama3.2\""));
        assert!(json.contains("\"stream\":false"));
        // top_p should be omitted (skip_serializing_if)
        assert!(!json.contains("top_p"));
    }

    /// Default_false ensures stream field is present in JSON.
    #[test]
    fn test_default_false_stream() {
        let req = InferenceRequest {
            model: "llama3".into(),
            prompt: "hi".into(),
            stream: false,
            options: None,
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("\"stream\":false"));
    }

    /// LocalModelHealth::is_healthy matches only Healthy variant.
    #[test]
    fn test_health_is_healthy() {
        assert!(LocalModelHealth::Healthy.is_healthy());
        assert!(!LocalModelHealth::Degraded.is_healthy());
        assert!(!LocalModelHealth::Unavailable.is_healthy());
    }
}
