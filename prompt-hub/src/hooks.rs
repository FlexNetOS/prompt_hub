#![forbid(unsafe_code)]

use crate::error::Result;
use crate::models::{AgentIdentity, ExecutionPlan, ExecutionResult};
use std::fmt::Debug;
use std::future::Future;
use std::pin::Pin;

/// Hooks allow intercepting core Hub operations.
pub trait Hook: Send + Sync + Debug {
    /// Name of the hook
    fn name(&self) -> &'static str;

    /// Called before an operation is executed.
    fn pre_execute<'a>(
        &'a self,
        plan: &'a ExecutionPlan,
        identity: &'a AgentIdentity,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>>;

    /// Called after an operation is executed.
    fn post_execute<'a>(
        &'a self,
        plan: &'a ExecutionPlan,
        result: &'a ExecutionResult,
        identity: &'a AgentIdentity,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>>;
}

/// A specialized hook for Junie's orchestration.
#[derive(Debug, Default)]
pub struct JunieHook;

impl Hook for JunieHook {
    fn name(&self) -> &'static str {
        "junie-orchestrator"
    }

    fn pre_execute<'a>(
        &'a self,
        plan: &'a ExecutionPlan,
        identity: &'a AgentIdentity,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(async move {
            tracing::info!(
                agent = %identity.name,
                steps = plan.steps.len(),
                "Junie pre-execution hook triggered"
            );
            Ok(())
        })
    }

    fn post_execute<'a>(
        &'a self,
        _plan: &'a ExecutionPlan,
        result: &'a ExecutionResult,
        identity: &'a AgentIdentity,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(async move {
            tracing::info!(
                agent = %identity.name,
                success = result.success,
                "Junie post-execution hook triggered"
            );
            Ok(())
        })
    }
}

/// Manages a collection of hooks.
#[derive(Default)]
pub struct HookRegistry {
    hooks: Vec<Box<dyn Hook>>,
}

impl HookRegistry {
    pub fn new() -> Self {
        Self { hooks: Vec::new() }
    }

    pub fn register(&mut self, hook: Box<dyn Hook>) {
        self.hooks.push(hook);
    }

    async fn trigger_pre_execute(
        &self,
        plan: &ExecutionPlan,
        identity: &AgentIdentity,
    ) -> Result<()> {
        for hook in &self.hooks {
            hook.pre_execute(plan, identity).await?;
        }
        Ok(())
    }

    async fn trigger_post_execute(
        &self,
        plan: &ExecutionPlan,
        result: &ExecutionResult,
        identity: &AgentIdentity,
    ) -> Result<()> {
        for hook in &self.hooks {
            hook.post_execute(plan, result, identity).await?;
        }
        Ok(())
    }
}

impl Debug for HookRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HookRegistry")
            .field("hooks_count", &self.hooks.len())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::HubError;
    use crate::models::{AgentIdentity, ExecutionPlan, ExecutionResult};
    use std::sync::{Arc, Mutex};

    /// A test hook that records the order in which its phases fire into a shared
    /// log, and can be configured to fail in either phase. Lets us assert
    /// ordering and error short-circuit behaviour of [`HookRegistry`].
    #[derive(Debug)]
    struct RecordingHook {
        name: &'static str,
        log: Arc<Mutex<Vec<String>>>,
        fail_pre: bool,
        fail_post: bool,
    }

    impl RecordingHook {
        fn new(name: &'static str, log: Arc<Mutex<Vec<String>>>) -> Self {
            Self {
                name,
                log,
                fail_pre: false,
                fail_post: false,
            }
        }

        fn failing_pre(name: &'static str, log: Arc<Mutex<Vec<String>>>) -> Self {
            Self {
                name,
                log,
                fail_pre: true,
                fail_post: false,
            }
        }

        fn failing_post(name: &'static str, log: Arc<Mutex<Vec<String>>>) -> Self {
            Self {
                name,
                log,
                fail_pre: false,
                fail_post: true,
            }
        }
    }

    impl Hook for RecordingHook {
        fn name(&self) -> &'static str {
            self.name
        }

        fn pre_execute<'a>(
            &'a self,
            _plan: &'a ExecutionPlan,
            _identity: &'a AgentIdentity,
        ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>> {
            Box::pin(async move {
                self.log.lock().unwrap_or_else(std::sync::PoisonError::into_inner).push(format!("{}:pre", self.name));
                if self.fail_pre {
                    return Err(HubError::Internal(format!("{} pre failed", self.name)));
                }
                Ok(())
            })
        }

        fn post_execute<'a>(
            &'a self,
            _plan: &'a ExecutionPlan,
            _result: &'a ExecutionResult,
            _identity: &'a AgentIdentity,
        ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>> {
            Box::pin(async move {
                self.log.lock().unwrap_or_else(std::sync::PoisonError::into_inner).push(format!("{}:post", self.name));
                if self.fail_post {
                    return Err(HubError::Internal(format!("{} post failed", self.name)));
                }
                Ok(())
            })
        }
    }

    fn log() -> Arc<Mutex<Vec<String>>> {
        Arc::new(Mutex::new(Vec::new()))
    }

    fn drain(log: &Arc<Mutex<Vec<String>>>) -> Vec<String> {
        log.lock().unwrap_or_else(std::sync::PoisonError::into_inner).clone()
    }

    #[test]
    fn junie_hook_has_stable_name() {
        assert_eq!(JunieHook.name(), "junie-orchestrator");
    }

    #[tokio::test]
    async fn junie_hook_pre_and_post_succeed() {
        let hook = JunieHook;
        let plan = ExecutionPlan::default();
        let result = ExecutionResult::default();
        let identity = AgentIdentity::default();

        assert!(hook.pre_execute(&plan, &identity).await.is_ok());
        assert!(hook.post_execute(&plan, &result, &identity).await.is_ok());
    }

    #[tokio::test]
    async fn empty_registry_is_a_no_op() {
        let registry = HookRegistry::new();
        let plan = ExecutionPlan::default();
        let result = ExecutionResult::default();
        let identity = AgentIdentity::default();

        assert!(registry.trigger_pre_execute(&plan, &identity).await.is_ok());
        assert!(
            registry
                .trigger_post_execute(&plan, &result, &identity)
                .await
                .is_ok()
        );
    }

    #[tokio::test]
    async fn pre_execute_runs_all_hooks_in_registration_order() {
        let log = log();
        let mut registry = HookRegistry::new();
        registry.register(Box::new(RecordingHook::new("first", log.clone())));
        registry.register(Box::new(RecordingHook::new("second", log.clone())));
        registry.register(Box::new(RecordingHook::new("third", log.clone())));

        let plan = ExecutionPlan::default();
        let identity = AgentIdentity::default();
        registry
            .trigger_pre_execute(&plan, &identity)
            .await
            .expect("all pre hooks succeed");

        assert_eq!(drain(&log), vec!["first:pre", "second:pre", "third:pre"]);
    }

    #[tokio::test]
    async fn post_execute_runs_all_hooks_in_registration_order() {
        let log = log();
        let mut registry = HookRegistry::new();
        registry.register(Box::new(RecordingHook::new("first", log.clone())));
        registry.register(Box::new(RecordingHook::new("second", log.clone())));

        let plan = ExecutionPlan::default();
        let result = ExecutionResult::default();
        let identity = AgentIdentity::default();
        registry
            .trigger_post_execute(&plan, &result, &identity)
            .await
            .expect("all post hooks succeed");

        assert_eq!(drain(&log), vec!["first:post", "second:post"]);
    }

    #[tokio::test]
    async fn pre_execute_short_circuits_on_first_error() {
        let log = log();
        let mut registry = HookRegistry::new();
        registry.register(Box::new(RecordingHook::new("ok", log.clone())));
        registry.register(Box::new(RecordingHook::failing_pre("boom", log.clone())));
        // This hook must NOT run because the previous one errored.
        registry.register(Box::new(RecordingHook::new("never", log.clone())));

        let plan = ExecutionPlan::default();
        let identity = AgentIdentity::default();
        let err = registry
            .trigger_pre_execute(&plan, &identity)
            .await
            .expect_err("the failing hook propagates");

        assert!(matches!(err, HubError::Internal(msg) if msg.contains("boom")));
        // "ok" and "boom" ran; "never" was short-circuited.
        assert_eq!(drain(&log), vec!["ok:pre", "boom:pre"]);
    }

    #[tokio::test]
    async fn post_execute_short_circuits_on_first_error() {
        let log = log();
        let mut registry = HookRegistry::new();
        registry.register(Box::new(RecordingHook::failing_post("boom", log.clone())));
        registry.register(Box::new(RecordingHook::new("never", log.clone())));

        let plan = ExecutionPlan::default();
        let result = ExecutionResult::default();
        let identity = AgentIdentity::default();
        let err = registry
            .trigger_post_execute(&plan, &result, &identity)
            .await
            .expect_err("the failing post hook propagates");

        assert!(matches!(err, HubError::Internal(msg) if msg.contains("boom")));
        assert_eq!(drain(&log), vec!["boom:post"]);
    }

    #[test]
    fn registry_debug_reports_hook_count() {
        let log = log();
        let mut registry = HookRegistry::new();
        registry.register(Box::new(RecordingHook::new("a", log.clone())));
        registry.register(Box::new(RecordingHook::new("b", log)));

        assert_eq!(format!("{registry:?}"), "HookRegistry { hooks_count: 2 }");
    }
}
