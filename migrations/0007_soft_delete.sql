-- Migration 0007_soft_delete: Additional soft-delete support
-- Soft delete is already supported via the deleted_at column in prompts (0001_initial).
-- This migration adds any additional indexes or cleanup triggers.

-- Index for listing deleted prompts (admin use)
CREATE INDEX IF NOT EXISTS idx_prompts_deleted_name ON prompts(deleted_at, name) WHERE deleted_at IS NOT NULL;
