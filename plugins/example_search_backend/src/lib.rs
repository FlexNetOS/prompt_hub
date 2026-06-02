#![forbid(unsafe_code)]
use prompt_hub::plugins::{Plugin, PluginHealth};
use prompt_hub::HubError;

#[derive(Debug)]
pub struct ExampleSearchBackend;

impl Plugin for ExampleSearchBackend {
    fn name(&self) -> &str { "example-search-backend" }
    fn version(&self) -> &str { "1.0.0" }
    fn initialize(&mut self) -> Result<(), HubError> {
        println!("Example search backend initialized");
        Ok(())
    }
    fn health(&self) -> PluginHealth { PluginHealth::Healthy }
}
