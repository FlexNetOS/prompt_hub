#![forbid(unsafe_code)]
use anyhow::Result;
use prompt_hub::plugins::{PluginRegistry, load_static_plugins};
use tracing::{info, warn};

#[derive(Debug, Clone)]
pub enum PluginCommand {
    List,
    Install { path: std::path::PathBuf },
    Uninstall { name: String },
    Enable { name: String },
    Disable { name: String },
}

pub async fn run(cmd: PluginCommand) -> Result<()> {
    match cmd {
        PluginCommand::List => {
            info!("Listing plugins");
            let mut registry = PluginRegistry::new();
            let static_plugins = load_static_plugins();
            for plugin in static_plugins {
                let name = plugin.name().to_string();
                if let Err(e) = registry.register(plugin) {
                    warn!("Failed to register plugin '{}': {}", name, e);
                }
            }
            let plugins = registry.list();
            if plugins.is_empty() {
                println!("No plugins registered");
            } else {
                println!("Registered plugins ({}):", plugins.len());
                for (name, version, health) in plugins {
                    println!("  - {} (v{}) [{:?}]", name, version, health);
                }
            }
        }
        PluginCommand::Install { path } => {
            info!("Installing plugin from {:?}", path);
            let mut registry = PluginRegistry::new();
            let name = registry.register_from_path(&path)?;
            println!("Plugin '{}' registered from path {:?}", name, path);
            println!("  Note: Full dynamic loading requires unsafe code (forbidden).");
            println!("  Use static registration with register_plugin!() macro.");
        }
        PluginCommand::Uninstall { name } => {
            info!("Uninstalling plugin: {}", name);
            println!("Uninstalling '{}'...", name);
            println!("  Note: Uninstall requires rebuilding without the plugin.");
            println!("  Remove the plugin dependency and rebuild.");
        }
        PluginCommand::Enable { name } => {
            info!("Enabling plugin: {}", name);
            let mut registry = PluginRegistry::new();
            let static_plugins = load_static_plugins();
            for plugin in static_plugins {
                let _ = registry.register(plugin);
            }
            if registry.list().iter().any(|(n, _, _)| *n == name.as_str()) {
                println!("Plugin '{}' is enabled and ready", name);
            } else {
                println!("Plugin '{}' not found in registry. Install it first.", name);
            }
        }
        PluginCommand::Disable { name } => {
            info!("Disabling plugin: {}", name);
            println!("Disabling '{}'...", name);
            println!("  Note: Disable requires rebuilding without the plugin.");
            println!("  Remove the plugin dependency and rebuild.");
        }
    }

    Ok(())
}
