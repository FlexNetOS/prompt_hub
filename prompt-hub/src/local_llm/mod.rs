//! Local LLM inference — configuration, health checking, and model management for on-device
//! deployment scenarios.
//!
//! Provides a lightweight client that talks to local inference servers
//! (Ollama, llamafile, whisper.cpp) via their HTTP APIs. No model weights are embedded.

#![forbid(unsafe_code)]

mod engine;
mod inference;

pub use crate::models::{LocalModelConfig, LocalModelHealth, LocalProviderKind, ModelInfo};
pub use engine::LocalModelEngine;
pub use inference::{InferenceOptions, InferenceRequest};
