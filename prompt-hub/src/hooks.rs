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
