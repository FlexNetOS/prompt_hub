-- Migration 0005_backup_meta: Track database backup metadata
CREATE TABLE IF NOT EXISTS backup_meta (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    path TEXT NOT NULL,
    sha256 TEXT NOT NULL,
    size_bytes INTEGER NOT NULL,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_backup_created ON backup_meta(created_at DESC);
