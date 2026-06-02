#![forbid(unsafe_code)]
use prompt_hub::plugins::{Plugin, PluginHealth};
use prompt_hub::HubError;

#[derive(Debug)]
pub struct ExampleSanitizer;

impl Plugin for ExampleSanitizer {
    fn name(&self) -> &str { "example-sanitizer" }
    fn version(&self) -> &str { "1.0.0" }
    fn initialize(&mut self) -> Result<(), HubError> {
        println!("Example sanitizer initialized");
        Ok(())
    }
    fn health(&self) -> PluginHealth { PluginHealth::Healthy }
}
