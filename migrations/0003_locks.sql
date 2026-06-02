-- Migration 0003_locks: Optimistic locking for concurrent editing
CREATE TABLE IF NOT EXISTS locks (
    id TEXT PRIMARY KEY,
    prompt_id TEXT NOT NULL UNIQUE REFERENCES prompts(id) ON DELETE CASCADE,
    agent_id TEXT NOT NULL,
    token_hash TEXT NOT NULL,
    expires_at DATETIME NOT NULL,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- Lock expiration cleanup index
CREATE INDEX IF NOT EXISTS idx_locks_expires ON locks(expires_at);

-- Agent-held locks lookup
CREATE INDEX IF NOT EXISTS idx_locks_agent ON locks(agent_id);
