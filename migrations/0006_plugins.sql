-- Migration 0006_plugins: Plugin registry for extensibility
CREATE TABLE IF NOT EXISTS plugin_registry (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    version TEXT NOT NULL,
    path TEXT NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 1,
    health_status TEXT NOT NULL DEFAULT 'Healthy',
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_plugins_name ON plugin_registry(name);
CREATE INDEX IF NOT EXISTS idx_plugins_enabled ON plugin_registry(enabled);
