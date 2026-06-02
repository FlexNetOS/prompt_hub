-- Migration 0002_audit: Audit log for tracking all changes
CREATE TABLE IF NOT EXISTS audit_log (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    timestamp DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    agent_id TEXT NOT NULL,
    action TEXT NOT NULL,
    prompt_id TEXT,
    diff_hash TEXT NOT NULL DEFAULT '',
    before_json TEXT,
    after_json TEXT,
    ip_address TEXT
);

-- Audit trail lookups by prompt
CREATE INDEX IF NOT EXISTS idx_audit_prompt_id ON audit_log(prompt_id, timestamp DESC);

-- Audit trail lookups by agent
CREATE INDEX IF NOT EXISTS idx_audit_agent_id ON audit_log(agent_id, timestamp DESC);
