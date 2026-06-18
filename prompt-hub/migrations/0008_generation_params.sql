-- Migration 0008_generation_params: index the per-prompt generation parameters.
--
-- Background: the `generation_params` column lives on the `prompts` table
-- (added in 0001_initial) and stores a serialized JSON object matching the
-- `GenerationParams` model — temperature, top_p, max_tokens, stop_sequences,
-- frequency_penalty, presence_penalty. It is written/read as a blob in
-- storage.rs (insert_prompt / update_prompt / row hydration).
--
-- Until now this migration was a comments-only "version marker": the column
-- was already present for fresh databases, so the file did nothing and the
-- stored JSON was opaque to the query planner. This migration turns that blob
-- into queryable, indexed schema — real, idempotent DDL consistent with the
-- index-driven style of 0001_initial and 0007_soft_delete.
--
-- All statements use IF NOT EXISTS so the migration is safe to re-run and is a
-- no-op on databases that already carry these indexes. libsql ships SQLite's
-- JSON1 functions (json_valid / json_extract), so the expression indexes below
-- are evaluated only for rows whose generation_params holds valid JSON.

-- Partial index: fast lookup / counting of prompts that carry custom
-- generation parameters (excludes the common NULL-params rows to stay small).
CREATE INDEX IF NOT EXISTS idx_prompts_generation_params
    ON prompts(id)
    WHERE generation_params IS NOT NULL;

-- Expression index on the temperature field so range/equality filters over a
-- prompt's sampling temperature can use an index instead of a full scan.
CREATE INDEX IF NOT EXISTS idx_prompts_gen_temperature
    ON prompts(json_extract(generation_params, '$.temperature'))
    WHERE generation_params IS NOT NULL
      AND json_valid(generation_params);

-- Expression index on max_tokens for budget-oriented queries
-- (e.g. "prompts that cap generation at <= N tokens").
CREATE INDEX IF NOT EXISTS idx_prompts_gen_max_tokens
    ON prompts(json_extract(generation_params, '$.max_tokens'))
    WHERE generation_params IS NOT NULL
      AND json_valid(generation_params);
