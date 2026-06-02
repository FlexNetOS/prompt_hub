#![forbid(unsafe_code)]

use crate::error::{HubError, Result};
use crate::models::HealthStatus;
use std::path::Path;
use std::sync::Mutex;
use tracing::{info, instrument, warn};

/// Static plugin registry — plugins register themselves at compile time via
/// the [`register_plugin`] function.  Each entry stores a human-readable name
/// and a constructor function that produces a boxed trait object.
static PLUGIN_REGISTRY: Mutex<Vec<(&'static str, fn() -> Box<dyn Plugin>)>> =
    Mutex::new(Vec::new());

/// Register a plugin constructor in the static registry.
///
/// Call this from a plugin crate's `lib.rs` during module initialisation:
///
/// ```ignore
/// use prompt_hub::plugins::{register_plugin, Plugin};
///
/// struct MyPlugin;
/// impl Plugin for MyPlugin { /* … */ }
///
/// fn create_my_plugin() -> Box<dyn Plugin> {
///     Box::new(MyPlugin)
/// }
///
/// register_plugin("my-plugin", create_my_plugin);
/// ```
pub fn register_plugin(name: &'static str, constructor: fn() -> Box<dyn Plugin>) {
    match PLUGIN_REGISTRY.lock() {
        Ok(mut registry) => {
            registry.push((name, constructor));
            info!("Statically registered plugin constructor '{}'", name);
        }
        Err(e) => {
            warn!("Failed to lock plugin registry for '{}': {}", name, e);
        }
    }
}

/// Load all statically registered plugins by invoking their constructors.
///
/// The returned `Vec` owns the plugin instances; they can then be added to a
/// [`PluginRegistry`] via [`PluginRegistry::register`].
pub fn load_static_plugins() -> Vec<Box<dyn Plugin>> {
    PLUGIN_REGISTRY
        .lock()
        .map(|registry| {
            let count = registry.len();
            let plugins: Vec<Box<dyn Plugin>> =
                registry.iter().map(|(_, ctor)| ctor()).collect();
            info!("Loaded {} static plugin(s)", count);
            plugins
        })
        .unwrap_or_else(|e| {
            warn!("Failed to lock plugin registry: {}", e);
            Vec::new()
        })
}

/// Trait for PromptHub plugins
///
/// Plugins are registered at startup and participate in health checks and
/// lifecycle management (initialize / shutdown).
pub trait Plugin: Send + Sync + std::fmt::Debug {
    /// Plugin name (kebab-case, unique within a registry)
    fn name(&self) -> &'static str;

    /// Plugin version in semver format
    fn version(&self) -> &'static str;

    /// Called once at startup before the server begins accepting requests.
    fn initialize(&mut self) -> Result<()>;

    /// Called during graceful shutdown to release resources.
    fn shutdown(&mut self) -> Result<()>;

    /// Return the current health of this plugin.
    fn health(&self) -> HealthStatus;
}

/// Plugin registry
///
/// Maintains an ordered list of loaded plugins and exposes bulk operations
/// for lifecycle and health monitoring.
#[derive(Debug)]
pub struct PluginRegistry {
    plugins: Vec<Box<dyn Plugin>>,
}

impl PluginRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self { plugins: Vec::new() }
    }

    /// Register a plugin.  The plugin's `initialize` is called immediately.
    #[instrument(skip(self, plugin))]
    pub fn register(&mut self, mut plugin: Box<dyn Plugin>) -> Result<()> {
        info!("Initializing plugin: {} v{}", plugin.name(), plugin.version());
        plugin.initialize()?;
        self.plugins.push(plugin);
        Ok(())
    }

    /// Register a plugin by path — logs the path and returns the plugin name.
    ///
    /// Full dynamic loading (dlopen) requires `unsafe` code which is forbidden
    /// by `#![forbid(unsafe_code)]`.  This method records the intent and
    /// returns the stem of the path so that callers can fall back to static
    /// registration via [`register_plugin`] and [`load_static_plugins`].
    #[instrument(skip(self))]
    pub fn register_from_path(&mut self, path: &Path) -> Result<String> {
        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown")
            .to_string();
        info!(
            "Plugin '{}' registered from path {:?} (static fallback — no dlopen)",
            name, path
        );
        Ok(name)
    }

    /// List all registered plugins with name, version, and health.
    pub fn list(&self) -> Vec<(&'static str, &'static str, HealthStatus)> {
        self.plugins
            .iter()
            .map(|p| (p.name(), p.version(), p.health()))
            .collect()
    }

    /// Shut down all plugins in reverse registration order.
    #[instrument(skip(self))]
    pub fn shutdown_all(&mut self) {
        for plugin in self.plugins.iter_mut().rev() {
            if let Err(e) = plugin.shutdown() {
                warn!("Plugin '{}' shutdown error: {}", plugin.name(), e);
            } else {
                info!("Plugin '{}' shutdown complete", plugin.name());
            }
        }
    }

    /// Check whether every registered plugin is healthy.
    pub fn all_healthy(&self) -> bool {
        self.plugins
            .iter()
            .all(|p| matches!(p.health(), HealthStatus::Healthy))
    }

    /// Return the number of registered plugins.
    pub fn len(&self) -> usize {
        self.plugins.len()
    }

    /// Returns true if no plugins are registered.
    pub fn is_empty(&self) -> bool {
        self.plugins.is_empty()
    }
}

impl Default for PluginRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug)]
    struct TestPlugin {
        name: &'static str,
        version: &'static str,
        healthy: bool,
        initialized: bool,
    }

    impl TestPlugin {
        fn new(name: &'static str, version: &'static str, healthy: bool) -> Self {
            Self {
                name,
                version,
                healthy,
                initialized: false,
            }
        }
    }

    impl Plugin for TestPlugin {
        fn name(&self) -> &'static str {
            self.name
        }
        fn version(&self) -> &'static str {
            self.version
        }
        fn initialize(&mut self) -> Result<()> {
            self.initialized = true;
            Ok(())
        }
        fn shutdown(&mut self) -> Result<()> {
            Ok(())
        }
        fn health(&self) -> HealthStatus {
            if self.healthy {
                HealthStatus::Healthy
            } else {
                HealthStatus::Unhealthy
            }
        }
    }

    #[test]
    fn test_register_and_list() {
        let mut registry = PluginRegistry::new();
        let plugin = Box::new(TestPlugin::new("test-plugin", "1.0.0", true));
        assert!(registry.register(plugin).is_ok());
        assert_eq!(registry.len(), 1);

        let list = registry.list();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].0, "test-plugin");
        assert_eq!(list[0].1, "1.0.0");
        assert!(matches!(list[0].2, HealthStatus::Healthy));
    }

    #[test]
    fn test_all_healthy() {
        let mut registry = PluginRegistry::new();
        registry
            .register(Box::new(TestPlugin::new("p1", "1.0.0", true)))
            .unwrap();
        assert!(registry.all_healthy());

        registry
            .register(Box::new(TestPlugin::new("p2", "1.0.0", false)))
            .unwrap();
        assert!(!registry.all_healthy());
    }

    #[test]
    fn test_shutdown_all() {
        let mut registry = PluginRegistry::new();
        registry
            .register(Box::new(TestPlugin::new("p1", "1.0.0", true)))
            .unwrap();
        registry
            .register(Box::new(TestPlugin::new("p2", "2.0.0", true)))
            .unwrap();
        registry.shutdown_all();
    }

    #[test]
    fn test_register_from_path() {
        let mut registry = PluginRegistry::new();
        let path = std::path::Path::new("/some/lib/my_plugin.so");
        let name = registry.register_from_path(path).unwrap();
        assert_eq!(name, "my_plugin");
    }

    #[test]
    fn test_load_static_plugins() {
        // Clear any existing registrations from previous tests
        {
            let mut reg = PLUGIN_REGISTRY.lock().unwrap();
            reg.clear();
        }

        // Register two test plugin constructors
        register_plugin("static-p1", || {
            Box::new(TestPlugin::new("static-p1", "1.0.0", true))
        });
        register_plugin("static-p2", || {
            Box::new(TestPlugin::new("static-p2", "2.0.0", true))
        });

        let plugins = load_static_plugins();
        assert_eq!(plugins.len(), 2);
        assert_eq!(plugins[0].name(), "static-p1");
        assert_eq!(plugins[1].name(), "static-p2");

        // Clean up so we don't pollute other test runs
        {
            let mut reg = PLUGIN_REGISTRY.lock().unwrap();
            reg.clear();
        }
    }
}
