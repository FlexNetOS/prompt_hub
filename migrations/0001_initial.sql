-- Migration 0001_initial: Core schema for prompt management
-- Tables: prompts, versions, metrics, embeddings, prompts_fts

-- Main prompts table with soft-delete support
CREATE TABLE IF NOT EXISTS prompts (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    version TEXT NOT NULL DEFAULT '0.1.0',
    status TEXT NOT NULL DEFAULT 'Draft',
    system_prompt TEXT NOT NULL DEFAULT '',
    user_template TEXT NOT NULL DEFAULT '',
    required_vars TEXT NOT NULL DEFAULT '[]',
    domain TEXT NOT NULL DEFAULT 'General',
    tags TEXT NOT NULL DEFAULT '[]',
    target_roles TEXT NOT NULL DEFAULT '[]',
    metadata TEXT NOT NULL DEFAULT '{}',
    metrics TEXT NOT NULL DEFAULT '{}',
    author_id TEXT NOT NULL DEFAULT '00000000-0000-0000-0000-000000000000',
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    deleted_at DATETIME,
    generation_params TEXT,
    locale TEXT,
    multimodal_config TEXT
);

-- Covering index for filtered list queries
CREATE INDEX IF NOT EXISTS idx_prompts_status_domain ON prompts(status, domain);

-- Ownership lookups
CREATE INDEX IF NOT EXISTS idx_prompts_author ON prompts(author_id);

-- Partial index: soft-delete filtering (excludes non-deleted rows)
CREATE INDEX IF NOT EXISTS idx_prompts_deleted_at ON prompts(deleted_at) WHERE deleted_at IS NOT NULL;

-- Name lookups for search
CREATE INDEX IF NOT EXISTS idx_prompts_name ON prompts(name);

-- Version history tracking
CREATE TABLE IF NOT EXISTS versions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    prompt_id TEXT NOT NULL REFERENCES prompts(id) ON DELETE CASCADE,
    parent_id TEXT REFERENCES prompts(id) ON DELETE SET NULL,
    version TEXT NOT NULL,
    changelog TEXT NOT NULL DEFAULT '',
    diff TEXT NOT NULL DEFAULT '',
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_versions_prompt_id ON versions(prompt_id, version DESC);

-- Usage metrics per prompt
CREATE TABLE IF NOT EXISTS metrics (
    prompt_id TEXT PRIMARY KEY REFERENCES prompts(id) ON DELETE CASCADE,
    usage_count INTEGER NOT NULL DEFAULT 0,
    success_rate REAL NOT NULL DEFAULT 0.0,
    avg_tokens INTEGER NOT NULL DEFAULT 0,
    avg_latency_ms INTEGER NOT NULL DEFAULT 0,
    last_used DATETIME,
    cost_estimate_usd REAL NOT NULL DEFAULT 0.0
);

-- Vector embeddings for semantic search (libsql native F32_BLOB)
CREATE TABLE IF NOT EXISTS embeddings (
    prompt_id TEXT PRIMARY KEY REFERENCES prompts(id) ON DELETE CASCADE,
    embedding F32_BLOB(384),
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- FTS5 virtual table for full-text search
CREATE VIRTUAL TABLE IF NOT EXISTS prompts_fts USING fts5(
    name,
    system_prompt,
    tags,
    content='prompts',
    content_rowid='rowid'
);

-- Triggers to keep FTS index in sync with prompts table
CREATE TRIGGER IF NOT EXISTS prompts_fts_insert
AFTER INSERT ON prompts
BEGIN
    INSERT INTO prompts_fts(rowid, name, system_prompt, tags)
    VALUES (new.rowid, new.name, new.system_prompt, new.tags);
END;

CREATE TRIGGER IF NOT EXISTS prompts_fts_update
AFTER UPDATE ON prompts
BEGIN
    INSERT INTO prompts_fts(prompts_fts, rowid, name, system_prompt, tags)
    VALUES ('delete', old.rowid, old.name, old.system_prompt, old.tags);
    INSERT INTO prompts_fts(rowid, name, system_prompt, tags)
    VALUES (new.rowid, new.name, new.system_prompt, new.tags);
END;

CREATE TRIGGER IF NOT EXISTS prompts_fts_delete
AFTER DELETE ON prompts
BEGIN
    INSERT INTO prompts_fts(prompts_fts, rowid, name, system_prompt, tags)
    VALUES ('delete', old.rowid, old.name, old.system_prompt, old.tags);
END;

CREATE TABLE IF NOT EXISTS agents (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    capabilities TEXT NOT NULL DEFAULT '[]',
    token_hash TEXT NOT NULL,
    specialization_score REAL NOT NULL DEFAULT 0.0,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_agents_name ON agents(name);
