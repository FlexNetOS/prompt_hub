-- Migration 0008_generation_params: Ensure generation_params column exists
-- The generation_params column was added in 0001_initial for new databases.
-- This migration is a no-op for new databases but ensures compatibility
-- if upgrading from an older schema where the column may be missing.

-- SQLite does not support IF NOT EXISTS for ALTER TABLE ADD COLUMN,
-- so we use a pragma check approach in the application layer.
-- This migration file serves as a version marker.
-- The application will verify the column exists and add it if needed.
