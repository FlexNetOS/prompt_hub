use prompt_hub::plugins::{Plugin, PluginRegistry, PluginHealth};

#[derive(Debug)]
struct ExamplePlugin;

impl Plugin for ExamplePlugin {
    fn name(&self) -> &str { "example-plugin" }
    fn version(&self) -> &str { "1.0.0" }
    fn initialize(&mut self) -> Result<(), prompt_hub::HubError> {
        println!("Example plugin initialized");
        Ok(())
    }
    fn health(&self) -> PluginHealth { PluginHealth::Healthy }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut registry = PluginRegistry::new();
    registry.register(Box::new(ExamplePlugin))?;
    println!("Registered plugins: {:?}", registry.list_names());
    Ok(())
}
