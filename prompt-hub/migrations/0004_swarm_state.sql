-- Migration 0004_swarm_state: Track swarm state transitions
CREATE TABLE IF NOT EXISTS swarm_state (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    from_state TEXT NOT NULL,
    to_state TEXT NOT NULL,
    trigger_agent TEXT NOT NULL,
    reason TEXT NOT NULL DEFAULT '',
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_swarm_state_created ON swarm_state(created_at DESC);
CREATE INDEX IF NOT EXISTS idx_swarm_state_agent ON swarm_state(trigger_agent, created_at DESC);
