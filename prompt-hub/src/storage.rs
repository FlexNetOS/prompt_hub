#![forbid(unsafe_code)]

use crate::error::{HubError, Result};
use crate::models::*;
use chrono::{DateTime, Utc};
use libsql::{Builder, Connection, Database, params};
use semver::Version;
use std::ops::Deref;
use std::sync::Arc;
use tokio::sync::Semaphore;
use tracing::{debug, info, instrument};
use uuid::Uuid;

/// Storage configuration for the libsql-backed prompt database.
#[derive(Debug, Clone)]
pub struct StorageConfig {
    /// Path to the SQLite database file (e.g., "prompthub.db").
    pub db_path: String,
    /// Maximum number of concurrent connections in the pool.
    pub max_connections: usize,
    /// Enable WAL journal mode for better concurrency.
    pub wal_mode: bool,
    /// Enable foreign key constraint enforcement.
    pub foreign_keys: bool,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            db_path: "prompthub.db".to_string(),
            max_connections: std::thread::available_parallelism()
                .map(|n| n.get() * 2 + 1)
                .unwrap_or(10),
            wal_mode: true,
            foreign_keys: true,
        }
    }
}

/// Pooled database connection manager using libsql.
///
/// Uses a semaphore to limit concurrent connections and an RAII guard
/// (`PooledConnection`) to automatically return permits when done.
#[derive(Debug, Clone)]
pub struct Storage {
    #[allow(dead_code)]
    db: Arc<Database>,
    /// Shared connection opened (and migrated) at construction. Reused for
    /// every pooled `acquire()`. Reusing one connection — rather than opening
    /// a fresh one per acquire — is required for `:memory:` databases, where
    /// each new connection would otherwise get its own private, empty database
    /// (no migrated tables). libsql `Connection` is internally synchronized and
    /// cheap to clone (it shares the underlying handle).
    conn: Connection,
    config: StorageConfig,
    semaphore: Arc<Semaphore>,
}

/// RAII pooled connection guard. Holds an owned semaphore permit
/// so the permit is released when the guard is dropped.
pub struct PooledConnection {
    #[allow(dead_code)]
    _permit: tokio::sync::OwnedSemaphorePermit,
    pub(crate) conn: Connection,
}

impl Deref for PooledConnection {
    type Target = Connection;

    fn deref(&self) -> &Self::Target {
        &self.conn
    }
}

impl Storage {
    /// Create a new storage instance with libsql.
    ///
    /// Opens the database, applies PRAGMA settings (WAL, foreign keys,
    /// synchronous), runs all pending migrations, and initialises the
    /// connection-pool semaphore.
    #[instrument(skip(config))]
    pub async fn new(config: StorageConfig) -> Result<Self> {
        info!("Initializing libsql storage at {}", config.db_path);

        let db = Builder::new_local(&config.db_path)
            .build()
            .await
            .map_err(|e| HubError::StorageError(format!("Failed to open database: {e}")))?;

        let conn = db
            .connect()
            .map_err(|e| HubError::StorageError(format!("Failed to connect: {e}")))?;

        // Enable WAL mode for better read concurrency. `PRAGMA journal_mode`
        // RETURNS the resulting mode as a row, so it must go through `query`,
        // not `execute` (which errors with "Execute returned rows"). In-memory
        // databases ignore WAL and report "memory" — harmless.
        if config.wal_mode {
            conn.query("PRAGMA journal_mode = WAL;", params!())
                .await
                .map_err(|e| HubError::StorageError(format!("Failed to set WAL mode: {e}")))?;
            debug!("WAL mode enabled");
        }

        // Enable foreign key constraints
        if config.foreign_keys {
            conn.execute("PRAGMA foreign_keys = ON;", params!())
                .await
                .map_err(|e| {
                    HubError::StorageError(format!("Failed to enable foreign keys: {e}"))
                })?;
            debug!("Foreign keys enabled");
        }

        // Balanced durability: WAL + NORMAL is safe for most use cases
        conn.execute("PRAGMA synchronous = NORMAL;", params!())
            .await
            .map_err(|e| HubError::StorageError(format!("Failed to set synchronous: {e}")))?;
        debug!("synchronous=NORMAL set");

        // Run all pending migrations
        Self::run_migrations(&conn).await?;

        // Optimise query planner on open
        conn.execute("PRAGMA optimize;", params!())
            .await
            .map_err(|e| HubError::StorageError(format!("PRAGMA optimize failed: {e}")))?;

        info!(
            "Storage initialized with {} max connections",
            config.max_connections
        );

        Ok(Self {
            db: Arc::new(db),
            conn,
            config: config.clone(),
            semaphore: Arc::new(Semaphore::new(config.max_connections)),
        })
    }

    /// Acquire a connection from the pool.
    ///
    /// Waits until a permit is available, then returns a `PooledConnection`
    /// guard that releases the permit on drop.
    pub async fn acquire(&self) -> Result<PooledConnection> {
        let permit = self
            .semaphore
            .clone()
            .acquire_owned()
            .await
            .map_err(|e| HubError::StorageError(format!("Pool semaphore error: {e}")))?;

        // Reuse the shared, already-migrated connection. Opening a fresh
        // connection here would yield an empty private database for `:memory:`.
        let conn = self.conn.clone();

        Ok(PooledConnection {
            _permit: permit,
            conn,
        })
    }

    // ────────────────────────── Migrations ──────────────────────────

    /// Run all SQL migration files in order.
    ///
    /// Migrations are tracked in `_migrations` table. Each migration is
    /// applied idempotently — already-applied migrations are skipped.
    async fn run_migrations(conn: &Connection) -> Result<()> {
        // Create the migrations tracking table
        conn.execute(
            "CREATE TABLE IF NOT EXISTS _migrations (
                id INTEGER PRIMARY KEY,
                name TEXT NOT NULL UNIQUE,
                applied_at DATETIME DEFAULT CURRENT_TIMESTAMP
            );",
            params!(),
        )
        .await
        .map_err(|e| HubError::StorageError(format!("Migration table creation: {e}")))?;

        let migrations: Vec<(i64, &str, &str)> = vec![
            (
                1,
                "0001_initial.sql",
                include_str!("../../migrations/0001_initial.sql"),
            ),
            (
                2,
                "0002_audit.sql",
                include_str!("../../migrations/0002_audit.sql"),
            ),
            (
                3,
                "0003_locks.sql",
                include_str!("../../migrations/0003_locks.sql"),
            ),
            (
                4,
                "0004_swarm_state.sql",
                include_str!("../../migrations/0004_swarm_state.sql"),
            ),
            (
                5,
                "0005_backup_meta.sql",
                include_str!("../../migrations/0005_backup_meta.sql"),
            ),
            (
                6,
                "0006_plugins.sql",
                include_str!("../../migrations/0006_plugins.sql"),
            ),
            (
                7,
                "0007_soft_delete.sql",
                include_str!("../../migrations/0007_soft_delete.sql"),
            ),
            (
                8,
                "0008_generation_params.sql",
                include_str!("../../migrations/0008_generation_params.sql"),
            ),
            (
                9,
                "0009_config.sql",
                include_str!("../../migrations/0009_config.sql"),
            ),
        ];

        for (id, name, sql) in migrations {
            let already_applied = conn
                .query("SELECT 1 FROM _migrations WHERE id = ?1", params!(id))
                .await
                .map_err(|e| HubError::StorageError(format!("Migration check query: {e}")))?
                .next()
                .await
                .map_err(|e| HubError::StorageError(format!("Migration check row: {e}")))?
                .is_some();

            if !already_applied {
                debug!("Applying migration {id}: {name}");

                // Wrap each migration in a transaction for atomicity
                conn.execute("BEGIN IMMEDIATE;", params!())
                    .await
                    .map_err(|e| HubError::StorageError(format!("Migration begin: {e}")))?;

                // `execute_batch` runs ALL statements in a migration file (most
                // migrations are multi-statement: table + indexes) and treats a
                // comments-only file (e.g. the 0008 version marker) as a no-op,
                // unlike `execute`, which runs one statement and errors on empty.
                conn.execute_batch(sql).await.map_err(|e| {
                    HubError::StorageError(format!("Migration {name} SQL failed: {e}"))
                })?;

                conn.execute(
                    "INSERT INTO _migrations (id, name) VALUES (?1, ?2);",
                    params!(id, name),
                )
                .await
                .map_err(|e| {
                    HubError::StorageError(format!("Migration record insert for {name}: {e}"))
                })?;

                conn.execute("COMMIT;", params!())
                    .await
                    .map_err(|e| HubError::StorageError(format!("Migration commit: {e}")))?;

                info!("Applied migration: {name}");
            } else {
                debug!("Migration {id} ({name}) already applied — skipping");
            }
        }

        Ok(())
    }

    // ────────────────────────── CRUD: Prompts ──────────────────────────

    /// Insert a new prompt into the database within a `BEGIN IMMEDIATE` transaction.
    #[instrument(skip(self, prompt))]
    pub async fn insert_prompt(&self, prompt: &Prompt) -> Result<()> {
        let conn = self.acquire().await?;

        conn.execute("BEGIN IMMEDIATE;", params!())
            .await
            .map_err(|e| HubError::StorageError(format!("Insert begin: {e}")))?;

        // Execute using savepoint for nested rollback safety
        let result: Result<()> = async {
            conn.execute(
                "INSERT INTO prompts (
                    id, name, version, status, system_prompt, user_template,
                    required_vars, domain, tags, target_roles, metadata, metrics,
                    author_id, created_at, updated_at, deleted_at,
                    generation_params, locale, multimodal_config
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19);",
                params!(
                    prompt.id.to_string(),
                    prompt.name.clone(),
                    prompt.version.to_string(),
                    serde_json::to_value(&prompt.status).ok().and_then(|v| v.as_str().map(String::from)).unwrap_or_default(),
                    prompt.system_prompt.clone(),
                    prompt.user_template.clone(),
                    serde_json::to_string(&prompt.required_vars).unwrap_or_else(|_| "[]".to_string()),
                    serde_json::to_value(prompt.domain).ok().and_then(|v| v.as_str().map(String::from)).unwrap_or_default(),
                    serde_json::to_string(&prompt.tags).unwrap_or_else(|_| "[]".to_string()),
                    serde_json::to_string(&prompt.target_roles).unwrap_or_else(|_| "[]".to_string()),
                    serde_json::to_string(&prompt.metadata).unwrap_or_else(|_| "{}".to_string()),
                    serde_json::to_string(&prompt.metrics).unwrap_or_else(|_| "{}".to_string()),
                    prompt.author.id.to_string(),
                    prompt.created_at.to_rfc3339(),
                    prompt.updated_at.to_rfc3339(),
                    prompt.deleted_at.map(|d| d.to_rfc3339()),
                    prompt.generation_params.as_ref().map(|p| serde_json::to_string(p).unwrap_or_default()),
                    prompt.locale.as_deref(),
                    prompt.multimodal.as_ref().map(|m| serde_json::to_string(m).unwrap_or_default()),
                ),
            ).await.map_err(|e| HubError::StorageError(format!("Insert prompt: {e}")))?;

            // FTS5 is kept in sync via triggers defined in 0001_initial.sql
            // No manual FTS insert needed.

            Ok(())
        }.await;

        match result {
            Ok(()) => {
                conn.execute("COMMIT;", params!())
                    .await
                    .map_err(|e| HubError::StorageError(format!("Insert commit: {e}")))?;
                info!("Inserted prompt: {} ({})", prompt.name, prompt.id);
                Ok(())
            }
            Err(e) => {
                let _ = conn.execute("ROLLBACK;", params!()).await;
                Err(e)
            }
        }
    }

    /// Fetch a single active (not soft-deleted) prompt by its UUID.
    #[instrument(skip(self))]
    pub async fn get_prompt(&self, id: Uuid) -> Result<Option<Prompt>> {
        let conn = self.acquire().await?;

        let mut rows = conn
            .query(
                "SELECT id, name, version, status, system_prompt, user_template,
                        required_vars, domain, tags, target_roles, metadata, metrics,
                        author_id, created_at, updated_at, deleted_at,
                        generation_params, locale, multimodal_config
                 FROM prompts WHERE id = ?1 AND deleted_at IS NULL;",
                params!(id.to_string()),
            )
            .await
            .map_err(|e| HubError::StorageError(format!("Get prompt query: {e}")))?;

        if let Some(row) = rows
            .next()
            .await
            .map_err(|e| HubError::StorageError(format!("Get prompt row: {e}")))?
        {
            Ok(Some(self.row_to_prompt(&row)?))
        } else {
            Ok(None)
        }
    }

    /// Update a prompt by id with a patch — only modifies provided fields.
    #[instrument(skip(self, patch))]
    pub async fn update_prompt(&self, id: Uuid, patch: &PromptPatch) -> Result<()> {
        let conn = self.acquire().await?;

        conn.execute("BEGIN IMMEDIATE;", params!())
            .await
            .map_err(|e| HubError::StorageError(format!("Update begin: {e}")))?;

        let result: Result<()> = async {
            // Build dynamic update based on which patch fields are set
            let mut sets = Vec::new();
            let mut params_vec: Vec<String> = Vec::new();

            if let Some(name) = &patch.name {
                sets.push("name = ?".to_string());
                params_vec.push(name.clone());
            }
            if let Some(system_prompt) = &patch.system_prompt {
                sets.push("system_prompt = ?".to_string());
                params_vec.push(system_prompt.clone());
            }
            if let Some(user_template) = &patch.user_template {
                sets.push("user_template = ?".to_string());
                params_vec.push(user_template.clone());
            }
            if let Some(required_vars) = &patch.required_vars {
                sets.push("required_vars = ?".to_string());
                params_vec.push(serde_json::to_string(required_vars).unwrap_or_default());
            }
            if let Some(domain) = &patch.domain {
                sets.push("domain = ?".to_string());
                params_vec.push(
                    serde_json::to_value(domain)
                        .ok()
                        .and_then(|v| v.as_str().map(String::from))
                        .unwrap_or_default(),
                );
            }
            if let Some(tags) = &patch.tags {
                sets.push("tags = ?".to_string());
                params_vec.push(serde_json::to_string(tags).unwrap_or_default());
            }
            if let Some(target_roles) = &patch.target_roles {
                sets.push("target_roles = ?".to_string());
                params_vec.push(serde_json::to_string(target_roles).unwrap_or_default());
            }
            if let Some(status) = &patch.status {
                sets.push("status = ?".to_string());
                params_vec.push(
                    serde_json::to_value(status)
                        .ok()
                        .and_then(|v| v.as_str().map(String::from))
                        .unwrap_or_default(),
                );
            }
            if let Some(generation_params) = &patch.generation_params {
                sets.push("generation_params = ?".to_string());
                params_vec.push(serde_json::to_string(generation_params).unwrap_or_default());
            }
            if let Some(locale) = &patch.locale {
                sets.push("locale = ?".to_string());
                params_vec.push(locale.clone());
            }

            if sets.is_empty() {
                return Ok(());
            }

            sets.push("updated_at = ?".to_string());
            params_vec.push(Utc::now().to_rfc3339());

            let sql = format!(
                "UPDATE prompts SET {} WHERE id = '{}' AND deleted_at IS NULL;",
                sets.join(", "),
                id
            );

            conn.execute(&sql, libsql::params_from_iter(params_vec))
                .await
                .map_err(|e| HubError::StorageError(format!("Update prompt: {e}")))?;

            Ok(())
        }
        .await;

        match result {
            Ok(()) => {
                conn.execute("COMMIT;", params!())
                    .await
                    .map_err(|e| HubError::StorageError(format!("Update commit: {e}")))?;
                info!("Updated prompt: {}", id);
                Ok(())
            }
            Err(e) => {
                let _ = conn.execute("ROLLBACK;", params!()).await;
                Err(e)
            }
        }
    }

    /// Rollback a prompt to a previous version by inserting a version record
    /// and updating the current prompt content.
    #[instrument(skip(self))]
    pub async fn rollback_prompt(&self, id: Uuid, to_version: &str) -> Result<()> {
        let conn = self.acquire().await?;

        conn.execute("BEGIN IMMEDIATE;", params!())
            .await
            .map_err(|e| HubError::StorageError(format!("Rollback begin: {e}")))?;

        let result: Result<()> = async {
            // Record the rollback in versions table
            conn.execute(
                "INSERT INTO versions (prompt_id, version, changelog, diff, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5);",
                params!(
                    id.to_string(),
                    to_version,
                    format!("Rollback to version {to_version}"),
                    "rollback",
                    Utc::now().to_rfc3339(),
                ),
            )
            .await
            .map_err(|e| HubError::StorageError(format!("Rollback version: {e}")))?;

            Ok(())
        }
        .await;

        match result {
            Ok(()) => {
                conn.execute("COMMIT;", params!())
                    .await
                    .map_err(|e| HubError::StorageError(format!("Rollback commit: {e}")))?;
                info!("Rolled back prompt {} to version {}", id, to_version);
                Ok(())
            }
            Err(e) => {
                let _ = conn.execute("ROLLBACK;", params!()).await;
                Err(e)
            }
        }
    }

    /// Transfer ownership of a prompt to a new agent.
    #[instrument(skip(self))]
    pub async fn transfer_prompt_ownership(&self, id: Uuid, new_owner_id: Uuid) -> Result<()> {
        let conn = self.acquire().await?;

        let rows = conn
            .execute(
                "UPDATE prompts SET author_id = ?1, updated_at = ?2
                 WHERE id = ?3 AND deleted_at IS NULL;",
                params!(
                    new_owner_id.to_string(),
                    Utc::now().to_rfc3339(),
                    id.to_string()
                ),
            )
            .await
            .map_err(|e| HubError::StorageError(format!("Transfer ownership: {e}")))?;

        if rows == 0 {
            return Err(HubError::NotFound(format!("Prompt {} not found", id)));
        }

        info!(
            "Transferred ownership of prompt {} to agent {}",
            id, new_owner_id
        );
        Ok(())
    }

    /// Soft-delete a prompt by setting `deleted_at`.
    #[instrument(skip(self))]
    pub async fn delete_prompt(&self, id: Uuid) -> Result<()> {
        let conn = self.acquire().await?;

        conn.execute("BEGIN IMMEDIATE;", params!())
            .await
            .map_err(|e| HubError::StorageError(format!("Delete begin: {e}")))?;

        let result: Result<()> = async {
            let now = Utc::now().to_rfc3339();
            let rows_affected = conn
                .execute(
                    "UPDATE prompts SET deleted_at = ?1, status = 'Archived', updated_at = ?1
                     WHERE id = ?2 AND deleted_at IS NULL;",
                    params!(now, id.to_string()),
                )
                .await
                .map_err(|e| HubError::StorageError(format!("Soft delete: {e}")))?;

            if rows_affected == 0 {
                return Err(HubError::NotFound(format!(
                    "Prompt {} not found or already deleted",
                    id
                )));
            }

            Ok(())
        }
        .await;

        match result {
            Ok(()) => {
                conn.execute("COMMIT;", params!())
                    .await
                    .map_err(|e| HubError::StorageError(format!("Delete commit: {e}")))?;
                info!("Soft-deleted prompt: {id}");
                Ok(())
            }
            Err(e) => {
                let _ = conn.execute("ROLLBACK;", params!()).await;
                Err(e)
            }
        }
    }

    /// Permanently delete a prompt (admin/hard-delete operation).
    #[instrument(skip(self))]
    pub async fn hard_delete_prompt(&self, id: Uuid) -> Result<()> {
        let conn = self.acquire().await?;

        conn.execute("BEGIN IMMEDIATE;", params!())
            .await
            .map_err(|e| HubError::StorageError(format!("Hard delete begin: {e}")))?;

        let result: Result<()> = async {
            conn.execute(
                "DELETE FROM prompts WHERE id = ?1;",
                params!(id.to_string()),
            )
            .await
            .map_err(|e| HubError::StorageError(format!("Hard delete: {e}")))?;
            Ok(())
        }
        .await;

        match result {
            Ok(()) => {
                conn.execute("COMMIT;", params!())
                    .await
                    .map_err(|e| HubError::StorageError(format!("Hard delete commit: {e}")))?;
                info!("Hard-deleted prompt: {id}");
                Ok(())
            }
            Err(e) => {
                let _ = conn.execute("ROLLBACK;", params!()).await;
                Err(e)
            }
        }
    }

    /// List active prompts with optional filtering and pagination.
    pub async fn list_prompts(
        &self,
        domain: Option<&Domain>,
        status: Option<&Status>,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<Prompt>> {
        let conn = self.acquire().await?;

        let mut conditions = vec!["deleted_at IS NULL".to_string()];
        let mut query_params: Vec<libsql::Value> = vec![];

        if let Some(d) = domain {
            conditions.push("domain = ?".to_string());
            query_params.push(
                serde_json::to_value(d)
                    .ok()
                    .and_then(|v| v.as_str().map(String::from))
                    .unwrap_or_default()
                    .into(),
            );
        }

        if let Some(s) = status {
            conditions.push("status = ?".to_string());
            query_params.push(
                serde_json::to_value(s)
                    .ok()
                    .and_then(|v| v.as_str().map(String::from))
                    .unwrap_or_default()
                    .into(),
            );
        }

        let where_clause = conditions.join(" AND ");
        let sql = format!(
            "SELECT id, name, version, status, system_prompt, user_template,
                    required_vars, domain, tags, target_roles, metadata, metrics,
                    author_id, created_at, updated_at, deleted_at,
                    generation_params, locale, multimodal_config
             FROM prompts
             WHERE {}
             ORDER BY updated_at DESC
             LIMIT {} OFFSET {};",
            where_clause, limit, offset
        );

        let mut stmt = conn
            .query(&sql, libsql::params_from_iter(query_params))
            .await
            .map_err(|e| HubError::StorageError(format!("List prompts query: {e}")))?;

        let mut prompts = Vec::new();
        while let Some(row) = stmt
            .next()
            .await
            .map_err(|e| HubError::StorageError(format!("List prompts row: {e}")))?
        {
            prompts.push(self.row_to_prompt(&row)?);
        }

        Ok(prompts)
    }

    /// Count active prompts, optionally filtered by domain and status.
    pub async fn count_prompts(
        &self,
        domain: Option<&Domain>,
        status: Option<&Status>,
    ) -> Result<i64> {
        let conn = self.acquire().await?;

        let mut conditions = vec!["deleted_at IS NULL".to_string()];
        let mut query_params: Vec<libsql::Value> = vec![];

        if let Some(d) = domain {
            conditions.push("domain = ?".to_string());
            query_params.push(
                serde_json::to_value(d)
                    .ok()
                    .and_then(|v| v.as_str().map(String::from))
                    .unwrap_or_default()
                    .into(),
            );
        }

        if let Some(s) = status {
            conditions.push("status = ?".to_string());
            query_params.push(
                serde_json::to_value(s)
                    .ok()
                    .and_then(|v| v.as_str().map(String::from))
                    .unwrap_or_default()
                    .into(),
            );
        }

        let where_clause = conditions.join(" AND ");
        let sql = format!("SELECT COUNT(*) FROM prompts WHERE {};", where_clause);

        let mut stmt = conn
            .query(&sql, libsql::params_from_iter(query_params))
            .await
            .map_err(|e| HubError::StorageError(format!("Count prompts: {e}")))?;

        if let Some(row) = stmt
            .next()
            .await
            .map_err(|e| HubError::StorageError(format!("Count row: {e}")))?
        {
            let count: i64 = row
                .get(0)
                .map_err(|e| HubError::StorageError(format!("Count extract: {e}")))?;
            Ok(count)
        } else {
            Ok(0)
        }
    }

    // ────────────────────────── FTS5 Search ──────────────────────────

    /// Full-text search over prompt name, system_prompt, and tags using FTS5.
    #[instrument(skip(self))]
    pub async fn search_prompts_fts(&self, query: &str, limit: usize) -> Result<Vec<Prompt>> {
        let conn = self.acquire().await?;

        // Sanitise the query: remove quotes that could break FTS syntax
        let safe_query = query.replace(['"', '\''], "");

        let sql = format!(
            "SELECT p.id, p.name, p.version, p.status, p.system_prompt, p.user_template,
                    p.required_vars, p.domain, p.tags, p.target_roles, p.metadata, p.metrics,
                    p.author_id, p.created_at, p.updated_at, p.deleted_at,
                    p.generation_params, p.locale, p.multimodal_config
             FROM prompts p
             JOIN prompts_fts fts ON p.rowid = fts.rowid
             WHERE prompts_fts MATCH ?1 AND p.deleted_at IS NULL
             ORDER BY rank
             LIMIT {};",
            limit
        );

        let mut stmt = conn
            .query(&sql, params!(safe_query))
            .await
            .map_err(|e| HubError::SearchError(format!("FTS search: {e}")))?;

        let mut prompts = Vec::new();
        while let Some(row) = stmt
            .next()
            .await
            .map_err(|e| HubError::SearchError(format!("FTS row: {e}")))?
        {
            prompts.push(self.row_to_prompt(&row)?);
        }

        debug!(
            "FTS search for '{}' returned {} results",
            query,
            prompts.len()
        );
        Ok(prompts)
    }

    /// Tag-based search: find prompts containing all specified tags.
    #[instrument(skip(self))]
    pub async fn search_by_tags(&self, tags: &[String], limit: usize) -> Result<Vec<Prompt>> {
        let conn = self.acquire().await?;

        // Build a JSON-substring query for each tag
        let tag_conditions: Vec<String> = tags
            .iter()
            .map(|t| format!("tags LIKE '%{}%'", t.replace('"', "\"\"")))
            .collect();

        let where_clause = if tag_conditions.is_empty() {
            "deleted_at IS NULL".to_string()
        } else {
            format!("deleted_at IS NULL AND {}", tag_conditions.join(" AND "))
        };

        let sql = format!(
            "SELECT id, name, version, status, system_prompt, user_template,
                    required_vars, domain, tags, target_roles, metadata, metrics,
                    author_id, created_at, updated_at, deleted_at,
                    generation_params, locale, multimodal_config
             FROM prompts
             WHERE {}
             ORDER BY updated_at DESC
             LIMIT {};",
            where_clause, limit
        );

        let mut stmt = conn
            .query(&sql, params!())
            .await
            .map_err(|e| HubError::SearchError(format!("Tag search: {e}")))?;

        let mut prompts = Vec::new();
        while let Some(row) = stmt
            .next()
            .await
            .map_err(|e| HubError::SearchError(format!("Tag row: {e}")))?
        {
            prompts.push(self.row_to_prompt(&row)?);
        }

        debug!("Tag search returned {} results", prompts.len());
        Ok(prompts)
    }

    // ────────────────────────── Versions ──────────────────────────

    /// Record a version snapshot for a prompt.
    pub async fn record_version(
        &self,
        prompt_id: Uuid,
        parent_id: Option<Uuid>,
        version: &Version,
        changelog: &str,
        diff: &str,
    ) -> Result<i64> {
        let conn = self.acquire().await?;

        conn.execute(
            "INSERT INTO versions (prompt_id, parent_id, version, changelog, diff)
             VALUES (?1, ?2, ?3, ?4, ?5);",
            params!(
                prompt_id.to_string(),
                parent_id.map(|p| p.to_string()),
                version.to_string(),
                changelog,
                diff,
            ),
        )
        .await
        .map_err(|e| HubError::StorageError(format!("Record version: {e}")))?;

        let mut rows = conn
            .query("SELECT last_insert_rowid();", params!())
            .await
            .map_err(|e| HubError::StorageError(format!("Last rowid: {e}")))?;

        if let Some(row) = rows
            .next()
            .await
            .map_err(|e| HubError::StorageError(format!("Rowid fetch: {e}")))?
        {
            let id: i64 = row
                .get(0)
                .map_err(|e| HubError::StorageError(format!("Extract rowid: {e}")))?;
            Ok(id)
        } else {
            Ok(0)
        }
    }

    /// Fetch version history for a prompt.
    pub async fn get_versions(&self, prompt_id: Uuid) -> Result<Vec<VersionRecord>> {
        let conn = self.acquire().await?;

        let mut stmt = conn
            .query(
                "SELECT id, prompt_id, parent_id, version, changelog, diff, created_at
                 FROM versions WHERE prompt_id = ?1 ORDER BY version DESC;",
                params!(prompt_id.to_string()),
            )
            .await
            .map_err(|e| HubError::StorageError(format!("Get versions: {e}")))?;

        let mut versions = Vec::new();
        while let Some(row) = stmt
            .next()
            .await
            .map_err(|e| HubError::StorageError(format!("Version row: {e}")))?
        {
            versions.push(self.row_to_version(&row)?);
        }

        Ok(versions)
    }

    // ────────────────────────── Locks ──────────────────────────

    /// Acquire a lock on a prompt for exclusive editing.
    #[instrument(skip(self))]
    pub async fn acquire_lock(
        &self,
        prompt_id: Uuid,
        agent_id: Uuid,
        token_hash: &str,
        expires_at: DateTime<Utc>,
    ) -> Result<LockToken> {
        let conn = self.acquire().await?;

        conn.execute("BEGIN IMMEDIATE;", params!())
            .await
            .map_err(|e| HubError::StorageError(format!("Lock begin: {e}")))?;

        // Check for existing lock
        let existing: Option<(String, String)> = {
            let mut stmt = conn
                .query(
                    "SELECT agent_id, expires_at FROM locks WHERE prompt_id = ?1;",
                    params!(prompt_id.to_string()),
                )
                .await
                .map_err(|e| HubError::StorageError(format!("Lock check: {e}")))?;

            if let Some(row) = stmt
                .next()
                .await
                .map_err(|e| HubError::StorageError(format!("Lock row: {e}")))?
            {
                let agent: String = row
                    .get(0)
                    .map_err(|e| HubError::StorageError(format!("Lock agent: {e}")))?;
                let expires: String = row
                    .get(1)
                    .map_err(|e| HubError::StorageError(format!("Lock expires: {e}")))?;
                Some((agent, expires))
            } else {
                None
            }
        };

        if let Some((agent, expires)) = existing {
            let expires_dt = DateTime::parse_from_rfc3339(&expires)
                .map(|d| d.with_timezone(&Utc))
                .unwrap_or_else(|_| Utc::now() - chrono::Duration::seconds(1));

            if expires_dt > Utc::now() {
                conn.execute("ROLLBACK;", params!()).await.ok();
                return Err(HubError::AuthError(format!(
                    "Prompt {prompt_id} is locked by agent {agent} until {expires}"
                )));
            }

            // Expired lock — remove it
            conn.execute(
                "DELETE FROM locks WHERE prompt_id = ?1;",
                params!(prompt_id.to_string()),
            )
            .await
            .map_err(|e| HubError::StorageError(format!("Delete expired lock: {e}")))?;
        }

        let lock_id = Uuid::new_v4();
        conn.execute(
            "INSERT INTO locks (id, prompt_id, agent_id, token_hash, expires_at)
             VALUES (?1, ?2, ?3, ?4, ?5);",
            params!(
                lock_id.to_string(),
                prompt_id.to_string(),
                agent_id.to_string(),
                token_hash,
                expires_at.to_rfc3339(),
            ),
        )
        .await
        .map_err(|e| HubError::StorageError(format!("Insert lock: {e}")))?;

        conn.execute("COMMIT;", params!())
            .await
            .map_err(|e| HubError::StorageError(format!("Lock commit: {e}")))?;

        info!("Lock acquired: {lock_id} on prompt {prompt_id}");
        Ok(LockToken {
            id: lock_id,
            prompt_id,
            agent_id,
            token_hash: token_hash.to_string(),
            expires_at,
            created_at: Utc::now(),
        })
    }

    /// Release a lock by its ID.
    #[instrument(skip(self))]
    pub async fn release_lock(&self, lock_id: Uuid) -> Result<()> {
        let conn = self.acquire().await?;

        conn.execute(
            "DELETE FROM locks WHERE id = ?1;",
            params!(lock_id.to_string()),
        )
        .await
        .map_err(|e| HubError::StorageError(format!("Release lock: {e}")))?;

        info!("Lock released: {lock_id}");
        Ok(())
    }

    // ────────────────────────── Audit ──────────────────────────

    /// Write an audit log entry. This should be called post-commit
    /// so it is never rolled back.
    #[instrument(skip(self))]
    pub async fn audit_log(&self, entry: &AuditEntry) -> Result<()> {
        let conn = self.acquire().await?;

        conn.execute(
            "INSERT INTO audit_log (timestamp, agent_id, action, prompt_id, diff_hash, before_json, after_json, ip_address)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8);",
            params!(
                entry.timestamp.to_rfc3339(),
                entry.agent_id.to_string(),
                entry.action.clone(),
                entry.prompt_id.map(|p| p.to_string()),
                entry.diff_hash.clone(),
                entry.before_json.as_deref(),
                entry.after_json.as_deref(),
                entry.ip_address.as_deref(),
            ),
        )
        .await
        .map_err(|e| HubError::StorageError(format!("Audit log: {e}")))?;

        Ok(())
    }

    /// Fetch audit log for a prompt.
    pub async fn get_audit_log(&self, prompt_id: Uuid, limit: usize) -> Result<Vec<AuditEntry>> {
        let conn = self.acquire().await?;

        let mut stmt = conn
            .query(
                "SELECT id, timestamp, agent_id, action, prompt_id, diff_hash,
                        before_json, after_json, ip_address
                 FROM audit_log WHERE prompt_id = ?1 ORDER BY timestamp DESC LIMIT ?2;",
                params!(prompt_id.to_string(), limit as i64),
            )
            .await
            .map_err(|e| HubError::StorageError(format!("Audit log query: {e}")))?;

        let mut entries = Vec::new();
        while let Some(row) = stmt
            .next()
            .await
            .map_err(|e| HubError::StorageError(format!("Audit row: {e}")))?
        {
            entries.push(self.row_to_audit_entry(&row)?);
        }

        Ok(entries)
    }

    /// Write an audit log entry using the current AuditEntry model.
    /// This should be called post-commit so it is never rolled back.
    #[instrument(skip(self, entry))]
    pub async fn log_audit(&self, entry: &AuditEntry) -> Result<()> {
        let conn = self.acquire().await?;
        conn.execute(
            "INSERT INTO audit_log (agent_id, action, prompt_id, diff_hash, before_json, after_json, ip_address, timestamp)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8);",
            params!(
                entry.agent_id.to_string(),
                entry.action.clone(),
                entry.prompt_id.map(|p| p.to_string()),
                entry.diff_hash.clone(),
                entry.before_json.as_deref(),
                entry.after_json.as_deref(),
                entry.ip_address.as_deref(),
                entry.timestamp.to_rfc3339(),
            ),
        ).await.map_err(|e| HubError::StorageError(format!("Audit log: {e}")))?;
        Ok(())
    }

    /// Fetch paginated audit trail for a specific prompt.
    #[instrument(skip(self))]
    pub async fn fetch_audit_trail(
        &self,
        prompt_id: Uuid,
        page: usize,
        per_page: usize,
    ) -> Result<Paginated<AuditEntry>> {
        let conn = self.acquire().await?;
        let offset = (page.saturating_sub(1)) * per_page;

        // Count total
        let mut count_rows = conn
            .query(
                "SELECT COUNT(*) FROM audit_log WHERE prompt_id = ?1",
                params!(prompt_id.to_string()),
            )
            .await
            .map_err(|e| HubError::StorageError(format!("Audit count: {e}")))?;
        let total = if let Some(row) = count_rows
            .next()
            .await
            .map_err(|e| HubError::StorageError(format!("Audit count row: {e}")))?
        {
            row.get::<i64>(0).unwrap_or(0) as usize
        } else {
            0
        };

        // Fetch entries
        let mut rows = conn.query(
            "SELECT id, agent_id, action, prompt_id, diff_hash, before_json, after_json, ip_address, timestamp FROM audit_log
             WHERE prompt_id = ?1 ORDER BY timestamp DESC LIMIT ?2 OFFSET ?3",
            params!(prompt_id.to_string(), per_page as i64, offset as i64),
        ).await.map_err(|e| HubError::StorageError(format!("Audit query: {e}")))?;

        let mut items = Vec::new();
        while let Some(row) = rows
            .next()
            .await
            .map_err(|e| HubError::StorageError(format!("Audit row: {e}")))?
        {
            let id = row.get::<i64>(0).unwrap_or(0);
            let agent_id =
                Uuid::parse_str(row.get_str(1).unwrap_or("")).unwrap_or_else(|_| Uuid::nil());
            let action = row.get_str(2).unwrap_or("").to_string();
            let pid = Uuid::parse_str(row.get_str(3).unwrap_or("")).ok();
            let diff_hash = row.get_str(4).unwrap_or("").to_string();
            let before = row.get_str(5).ok().map(|s| s.to_string());
            let after = row.get_str(6).ok().map(|s| s.to_string());
            let ip_address = row.get_str(7).ok().map(|s| s.to_string());
            let ts = row
                .get_str(8)
                .ok()
                .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                .map(|d| d.with_timezone(&Utc))
                .unwrap_or_else(Utc::now);

            items.push(AuditEntry {
                id,
                timestamp: ts,
                agent_id,
                action,
                prompt_id: pid,
                diff_hash,
                before_json: before,
                after_json: after,
                ip_address,
            });
        }

        Ok(Paginated {
            items,
            total,
            page,
            per_page,
        })
    }

    // ────────────────────────── Config KV ──────────────────────────

    /// Get a config value by key.
    pub async fn get_config(&self, key: &str) -> Result<Option<String>> {
        let conn = self.acquire().await?;

        let mut stmt = conn
            .query("SELECT value FROM config WHERE key = ?1;", params!(key))
            .await
            .map_err(|e| HubError::StorageError(format!("Get config: {e}")))?;

        if let Some(row) = stmt
            .next()
            .await
            .map_err(|e| HubError::StorageError(format!("Config row: {e}")))?
        {
            let value: String = row
                .get(0)
                .map_err(|e| HubError::StorageError(format!("Config value: {e}")))?;
            Ok(Some(value))
        } else {
            Ok(None)
        }
    }

    /// Set a config key-value pair.
    pub async fn set_config(&self, key: &str, value: &str) -> Result<()> {
        let conn = self.acquire().await?;

        conn.execute(
            "INSERT INTO config (key, value, updated_at) VALUES (?1, ?2, ?3)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at;",
            params!(key, value, Utc::now().to_rfc3339()),
        )
        .await
        .map_err(|e| HubError::StorageError(format!("Set config: {e}")))?;

        Ok(())
    }

    // ────────────────────────── Metrics ──────────────────────────

    /// Update usage metrics for a prompt.
    pub async fn update_metrics(&self, prompt_id: Uuid, metrics: &PromptMetrics) -> Result<()> {
        let conn = self.acquire().await?;

        conn.execute(
            "INSERT INTO metrics (prompt_id, usage_count, success_rate, avg_tokens, avg_latency_ms, last_used, cost_estimate_usd)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(prompt_id) DO UPDATE SET
                 usage_count = excluded.usage_count,
                 success_rate = excluded.success_rate,
                 avg_tokens = excluded.avg_tokens,
                 avg_latency_ms = excluded.avg_latency_ms,
                 last_used = excluded.last_used,
                 cost_estimate_usd = excluded.cost_estimate_usd;",
            params!(
                prompt_id.to_string(),
                metrics.usage_count as i64,
                metrics.success_rate,
                metrics.avg_tokens as i64,
                metrics.avg_latency_ms as i64,
                metrics.last_used.map(|d| d.to_rfc3339()),
                metrics.cost_estimate_usd,
            ),
        )
        .await
        .map_err(|e| HubError::StorageError(format!("Update metrics: {e}")))?;

        Ok(())
    }

    /// Get metrics for a prompt.
    pub async fn get_metrics(&self, prompt_id: Uuid) -> Result<Option<PromptMetrics>> {
        let conn = self.acquire().await?;

        let mut stmt = conn
            .query(
                "SELECT usage_count, success_rate, avg_tokens, avg_latency_ms, last_used, cost_estimate_usd
                 FROM metrics WHERE prompt_id = ?1;",
                params!(prompt_id.to_string()),
            )
            .await
            .map_err(|e| HubError::StorageError(format!("Get metrics: {e}")))?;

        if let Some(row) = stmt
            .next()
            .await
            .map_err(|e| HubError::StorageError(format!("Metrics row: {e}")))?
        {
            Ok(Some(self.row_to_metrics(&row)?))
        } else {
            Ok(None)
        }
    }

    // ────────────────────────── Maintenance ──────────────────────────

    /// Run database maintenance: VACUUM + ANALYZE.
    pub async fn maintenance(&self) -> Result<()> {
        let conn = self.acquire().await?;

        info!("Running database maintenance (ANALYZE)...");
        conn.execute("ANALYZE;", params!())
            .await
            .map_err(|e| HubError::StorageError(format!("ANALYZE: {e}")))?;

        // VACUUM can be slow — run it in a separate connection
        drop(conn);

        let conn = self.acquire().await?;
        info!("Running VACUUM...");
        conn.execute("VACUUM;", params!())
            .await
            .map_err(|e| HubError::StorageError(format!("VACUUM: {e}")))?;

        info!("Database maintenance complete");
        Ok(())
    }

    /// Run PRAGMA optimize on close.
    pub async fn optimize_on_close(&self) -> Result<()> {
        let conn = self.acquire().await?;
        conn.execute("PRAGMA optimize;", params!())
            .await
            .map_err(|e| HubError::StorageError(format!("Optimize: {e}")))?;
        Ok(())
    }

    /// Health check: execute a simple query to verify connectivity.
    pub async fn health_check(&self) -> Result<bool> {
        let conn = self.acquire().await?;
        // `SELECT 1` returns a row, so it must go through `query`, not
        // `execute` (which errors with "Execute returned rows").
        conn.query("SELECT 1;", params!())
            .await
            .map_err(|e| HubError::StorageError(format!("Health check: {e}")))?;
        Ok(true)
    }

    // ────────────────────────── Row mappers ──────────────────────────

    /// Convert a libsql row into a `Prompt`.
    pub(crate) fn row_to_prompt(&self, row: &libsql::Row) -> Result<Prompt> {
        let get_str = |idx: i32| -> String { row.get_str(idx).unwrap_or("").to_string() };

        let parse_json = |s: String| -> serde_json::Value {
            serde_json::from_str(&s).unwrap_or(serde_json::Value::Null)
        };

        let parse_datetime = |idx: i32| -> DateTime<Utc> {
            row.get_str(idx)
                .ok()
                .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                .map(|d| d.with_timezone(&Utc))
                .unwrap_or_else(Utc::now)
        };

        let id_str = get_str(0);
        let version_str = get_str(2);
        let status_str = get_str(3);
        let domain_str = get_str(7);
        let author_id_str = get_str(12);

        Ok(Prompt {
            id: Uuid::parse_str(&id_str).unwrap_or_else(|_| Uuid::nil()),
            name: get_str(1),
            version: Version::parse(&version_str).unwrap_or_else(|_| Version::new(0, 0, 0)),
            status: serde_json::from_str(&format!("\"{status_str}\"")).unwrap_or(Status::Draft),
            system_prompt: get_str(4),
            user_template: get_str(5),
            required_vars: parse_json(get_str(6))
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default(),
            domain: serde_json::from_str(&format!("\"{domain_str}\"")).unwrap_or(Domain::General),
            tags: parse_json(get_str(8))
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default(),
            target_roles: serde_json::from_str(&get_str(9)).unwrap_or_default(),
            metadata: serde_json::from_str(&get_str(10)).unwrap_or_default(),
            metrics: serde_json::from_str(&get_str(11)).unwrap_or_default(),
            author: AgentIdentity {
                id: Uuid::parse_str(&author_id_str).unwrap_or_else(|_| Uuid::nil()),
                name: "unknown".to_string(),
                capabilities: Vec::new(),
                token_hash: String::new(),
                specialization_score: 0.0,
            },
            created_at: parse_datetime(13),
            updated_at: parse_datetime(14),
            deleted_at: row
                .get_str(15)
                .ok()
                .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                .map(|d| d.with_timezone(&Utc)),
            generation_params: row
                .get_str(16)
                .ok()
                .and_then(|s| serde_json::from_str(s).ok()),
            locale: row.get_str(17).ok().map(|s| s.to_string()),
            multimodal: row
                .get_str(18)
                .ok()
                .and_then(|s| serde_json::from_str(s).ok()),
        })
    }

    /// Convert a libsql row into a `VersionRecord`.
    fn row_to_version(&self, row: &libsql::Row) -> Result<VersionRecord> {
        let get_str = |idx: i32| -> String { row.get_str(idx).unwrap_or("").to_string() };

        let parse_dt = |idx: i32| -> DateTime<Utc> {
            row.get_str(idx)
                .ok()
                .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                .map(|d| d.with_timezone(&Utc))
                .unwrap_or_else(Utc::now)
        };

        Ok(VersionRecord {
            id: row
                .get::<i64>(0)
                .map_err(|e| HubError::StorageError(format!("Version id: {e}")))?,
            prompt_id: Uuid::parse_str(&get_str(1))
                .map_err(|e| HubError::SerdeError(format!("Version prompt_id: {e}")))?,
            parent_id: row.get_str(2).ok().and_then(|s| Uuid::parse_str(s).ok()),
            version: Version::parse(&get_str(3)).unwrap_or_else(|_| Version::new(0, 0, 0)),
            changelog: get_str(4),
            diff: get_str(5),
            created_at: parse_dt(6),
        })
    }

    /// Convert a libsql row into an `AuditEntry`.
    fn row_to_audit_entry(&self, row: &libsql::Row) -> Result<AuditEntry> {
        let get_str = |idx: i32| -> String { row.get_str(idx).unwrap_or("").to_string() };

        let parse_dt = |idx: i32| -> DateTime<Utc> {
            row.get_str(idx)
                .ok()
                .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                .map(|d| d.with_timezone(&Utc))
                .unwrap_or_else(Utc::now)
        };

        Ok(AuditEntry {
            id: row
                .get::<i64>(0)
                .map_err(|e| HubError::StorageError(format!("Audit id: {e}")))?,
            timestamp: parse_dt(1),
            agent_id: Uuid::parse_str(&get_str(2))
                .map_err(|e| HubError::SerdeError(format!("Audit agent_id: {e}")))?,
            action: get_str(3),
            prompt_id: row.get_str(4).ok().and_then(|s| Uuid::parse_str(s).ok()),
            diff_hash: get_str(5),
            before_json: row.get_str(6).ok().map(|s| s.to_string()),
            after_json: row.get_str(7).ok().map(|s| s.to_string()),
            ip_address: row.get_str(8).ok().map(|s| s.to_string()),
        })
    }

    /// Convert a libsql row into `PromptMetrics`.
    fn row_to_metrics(&self, row: &libsql::Row) -> Result<PromptMetrics> {
        let get_i64 = |idx: i32| -> i64 { row.get::<i64>(idx).unwrap_or(0) };

        let get_f64 = |idx: i32| -> f64 { row.get::<f64>(idx).unwrap_or(0.0) };

        let last_used = row
            .get_str(4)
            .ok()
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
            .map(|d| d.with_timezone(&Utc));

        Ok(PromptMetrics {
            usage_count: get_i64(0) as u64,
            success_rate: get_f64(1),
            avg_tokens: get_i64(2) as u64,
            avg_latency_ms: get_i64(3) as u64,
            last_used,
            cost_estimate_usd: get_f64(5),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Create an in-memory storage instance for tests.
    async fn in_memory_storage() -> Storage {
        let config = StorageConfig {
            db_path: ":memory:".to_string(),
            max_connections: 2,
            ..Default::default()
        };
        Storage::new(config)
            .await
            .expect("Failed to create in-memory storage")
    }

    /// Create a minimal test prompt.
    fn test_prompt(name: &str) -> Prompt {
        Prompt::new(name, "You are a helpful assistant.")
    }

    #[tokio::test]
    async fn test_storage_init() {
        let storage = in_memory_storage().await;
        assert!(storage.health_check().await.is_ok());
    }

    #[tokio::test]
    async fn test_insert_and_get_prompt() {
        let storage = in_memory_storage().await;
        let mut prompt = test_prompt("test_insert");
        prompt.tags = vec!["test".to_string(), "unit".to_string()];
        prompt.domain = Domain::Coding;

        storage.insert_prompt(&prompt).await.expect("insert failed");

        let fetched = storage
            .get_prompt(prompt.id)
            .await
            .expect("get failed")
            .expect("prompt not found");

        assert_eq!(fetched.name, "test_insert");
        assert_eq!(fetched.id, prompt.id);
        assert_eq!(fetched.tags, vec!["test", "unit"]);
        assert_eq!(fetched.domain, Domain::Coding);
        assert_eq!(fetched.status, Status::Draft);
    }

    #[tokio::test]
    async fn test_get_nonexistent() {
        let storage = in_memory_storage().await;
        let result = storage
            .get_prompt(Uuid::new_v4())
            .await
            .expect("query failed");
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_update_prompt() {
        let storage = in_memory_storage().await;
        let prompt = test_prompt("original");
        storage.insert_prompt(&prompt).await.unwrap();

        let patch = PromptPatch {
            name: Some("updated".to_string()),
            system_prompt: Some("Updated system prompt.".to_string()),
            ..Default::default()
        };
        storage
            .update_prompt(prompt.id, &patch)
            .await
            .expect("update failed");

        let fetched = storage.get_prompt(prompt.id).await.unwrap().unwrap();
        assert_eq!(fetched.name, "updated");
        assert_eq!(fetched.system_prompt, "Updated system prompt.");
    }

    #[tokio::test]
    async fn test_soft_delete_prompt() {
        let storage = in_memory_storage().await;
        let prompt = test_prompt("to_delete");
        storage.insert_prompt(&prompt).await.unwrap();

        storage
            .delete_prompt(prompt.id)
            .await
            .expect("delete failed");

        let fetched = storage.get_prompt(prompt.id).await.unwrap();
        assert!(
            fetched.is_none(),
            "Soft-deleted prompt should not be returned"
        );
    }

    #[tokio::test]
    async fn test_list_prompts() {
        let storage = in_memory_storage().await;

        for i in 0..5 {
            let mut p = test_prompt(&format!("list_test_{i}"));
            p.domain = Domain::Coding;
            p.status = Status::Active;
            storage.insert_prompt(&p).await.unwrap();
        }

        let coding = storage
            .list_prompts(Some(&Domain::Coding), Some(&Status::Active), 10, 0)
            .await
            .unwrap();
        assert_eq!(coding.len(), 5);

        let general = storage
            .list_prompts(Some(&Domain::General), None, 10, 0)
            .await
            .unwrap();
        assert_eq!(general.len(), 0);
    }

    #[tokio::test]
    async fn test_count_prompts() {
        let storage = in_memory_storage().await;

        let count_before = storage.count_prompts(None, None).await.unwrap();
        assert_eq!(count_before, 0);

        let p = test_prompt("count_me");
        storage.insert_prompt(&p).await.unwrap();

        let count_after = storage.count_prompts(None, None).await.unwrap();
        assert_eq!(count_after, 1);
    }

    #[tokio::test]
    async fn test_fts_search() {
        let storage = in_memory_storage().await;

        let mut p1 = test_prompt("rust_helper");
        p1.system_prompt = "You are an expert Rust programmer.".to_string();
        p1.tags = vec!["rust".to_string(), "coding".to_string()];
        storage.insert_prompt(&p1).await.unwrap();

        let mut p2 = test_prompt("python_helper");
        p2.system_prompt = "You are an expert Python programmer.".to_string();
        p2.tags = vec!["python".to_string(), "coding".to_string()];
        storage.insert_prompt(&p2).await.unwrap();

        // Note: FTS search may need a small delay for triggers to fire
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        let results = storage.search_prompts_fts("Rust", 10).await.unwrap();
        assert!(
            results.iter().any(|r| r.name == "rust_helper"),
            "Should find the Rust prompt"
        );
    }

    #[tokio::test]
    async fn test_tag_search() {
        let storage = in_memory_storage().await;

        let mut p1 = test_prompt("tagged_a");
        p1.tags = vec!["alpha".to_string(), "beta".to_string()];
        storage.insert_prompt(&p1).await.unwrap();

        let mut p2 = test_prompt("tagged_b");
        p2.tags = vec!["alpha".to_string(), "gamma".to_string()];
        storage.insert_prompt(&p2).await.unwrap();

        let results = storage
            .search_by_tags(&["alpha".to_string()], 10)
            .await
            .unwrap();
        assert_eq!(results.len(), 2);
    }

    #[tokio::test]
    async fn test_version_record() {
        let storage = in_memory_storage().await;
        let prompt = test_prompt("versioned");
        storage.insert_prompt(&prompt).await.unwrap();

        let vid = storage
            .record_version(
                prompt.id,
                None,
                &Version::new(0, 1, 0),
                "Initial version",
                "",
            )
            .await
            .expect("record version failed");
        assert!(vid > 0);

        let versions = storage.get_versions(prompt.id).await.unwrap();
        assert_eq!(versions.len(), 1);
        assert_eq!(versions[0].changelog, "Initial version");
    }

    #[tokio::test]
    async fn test_lock_acquire_and_release() {
        let storage = in_memory_storage().await;
        let prompt = test_prompt("locked");
        storage.insert_prompt(&prompt).await.unwrap();

        let agent_id = Uuid::new_v4();
        let expires = Utc::now() + chrono::Duration::minutes(5);

        let lock = storage
            .acquire_lock(prompt.id, agent_id, "token_hash_abc", expires)
            .await
            .expect("acquire lock failed");

        assert_eq!(lock.prompt_id, prompt.id);
        assert_eq!(lock.agent_id, agent_id);

        // Releasing should succeed
        storage
            .release_lock(lock.id)
            .await
            .expect("release lock failed");
    }

    #[tokio::test]
    async fn test_lock_conflict() {
        let storage = in_memory_storage().await;
        let prompt = test_prompt("conflict");
        storage.insert_prompt(&prompt).await.unwrap();

        let agent1 = Uuid::new_v4();
        let agent2 = Uuid::new_v4();
        let expires = Utc::now() + chrono::Duration::minutes(5);

        // Agent 1 acquires
        let _lock1 = storage
            .acquire_lock(prompt.id, agent1, "hash1", expires)
            .await
            .unwrap();

        // Agent 2 should fail
        let result = storage
            .acquire_lock(prompt.id, agent2, "hash2", expires)
            .await;
        assert!(result.is_err(), "Should fail when prompt is already locked");
    }

    #[tokio::test]
    async fn test_log_audit() {
        let storage = in_memory_storage().await;
        let prompt = test_prompt("audited");
        storage.insert_prompt(&prompt).await.unwrap();

        let entry = AuditEntry {
            id: 0,
            timestamp: Utc::now(),
            agent_id: Uuid::new_v4(),
            action: "Created".to_string(),
            prompt_id: Some(prompt.id),
            diff_hash: String::new(),
            before_json: None,
            after_json: Some("{\"name\":\"audited\"}".to_string()),
            ip_address: None,
        };

        storage.log_audit(&entry).await.expect("log_audit failed");

        let logs = storage.fetch_audit_trail(prompt.id, 1, 10).await.unwrap();
        assert_eq!(logs.items.len(), 1);
        assert_eq!(logs.total, 1);
        assert_eq!(logs.items[0].action, "Created");
        assert_eq!(logs.items[0].prompt_id, Some(prompt.id));
    }

    #[tokio::test]
    async fn test_fetch_audit_trail_pagination() {
        let storage = in_memory_storage().await;
        let prompt = test_prompt("paginated_audit");
        storage.insert_prompt(&prompt).await.unwrap();
        let agent_id = Uuid::new_v4();

        // Insert 5 audit entries
        for i in 0..5 {
            let entry = AuditEntry {
                id: 0,
                timestamp: Utc::now(),
                agent_id,
                action: "Updated".to_string(),
                prompt_id: Some(prompt.id),
                diff_hash: format!("diff-{}", i),
                before_json: Some(format!("before-{}", i)),
                after_json: Some(format!("after-{}", i)),
                ip_address: None,
            };
            storage.log_audit(&entry).await.expect("log_audit failed");
        }

        // Page 1: 2 per page
        let page1 = storage.fetch_audit_trail(prompt.id, 1, 2).await.unwrap();
        assert_eq!(page1.items.len(), 2);
        assert_eq!(page1.total, 5);
        assert_eq!(page1.page, 1);

        // Page 2: 2 per page
        let page2 = storage.fetch_audit_trail(prompt.id, 2, 2).await.unwrap();
        assert_eq!(page2.items.len(), 2);
        assert_eq!(page2.total, 5);
        assert_eq!(page2.page, 2);

        // Page 3: 2 per page — should have only 1 remaining
        let page3 = storage.fetch_audit_trail(prompt.id, 3, 2).await.unwrap();
        assert_eq!(page3.items.len(), 1);
        assert_eq!(page3.total, 5);
        assert_eq!(page3.page, 3);

        // Non-existent prompt should return empty
        let empty = storage
            .fetch_audit_trail(Uuid::new_v4(), 1, 10)
            .await
            .unwrap();
        assert_eq!(empty.items.len(), 0);
        assert_eq!(empty.total, 0);
    }

    #[tokio::test]
    async fn test_config_kv() {
        let storage = in_memory_storage().await;

        let value = storage.get_config("test_key").await.unwrap();
        assert!(value.is_none());

        storage.set_config("test_key", "test_value").await.unwrap();

        let value = storage.get_config("test_key").await.unwrap();
        assert_eq!(value, Some("test_value".to_string()));

        // Update existing key
        storage
            .set_config("test_key", "updated_value")
            .await
            .unwrap();
        let value = storage.get_config("test_key").await.unwrap();
        assert_eq!(value, Some("updated_value".to_string()));
    }

    #[tokio::test]
    async fn test_metrics_crud() {
        let storage = in_memory_storage().await;
        let prompt = test_prompt("metrics_test");
        storage.insert_prompt(&prompt).await.unwrap();

        let metrics = PromptMetrics {
            usage_count: 42,
            success_rate: 0.95,
            avg_tokens: 1024,
            avg_latency_ms: 150,
            last_used: Some(Utc::now()),
            cost_estimate_usd: 0.05,
        };

        storage.update_metrics(prompt.id, &metrics).await.unwrap();

        let fetched = storage
            .get_metrics(prompt.id)
            .await
            .unwrap()
            .expect("metrics not found");
        assert_eq!(fetched.usage_count, 42);
        assert_eq!(fetched.success_rate, 0.95);
        assert_eq!(fetched.avg_tokens, 1024);
    }

    #[tokio::test]
    async fn test_health_check() {
        let storage = in_memory_storage().await;
        let healthy = storage.health_check().await.unwrap();
        assert!(healthy);
    }

    #[tokio::test]
    async fn test_concurrent_acquire() {
        let storage = in_memory_storage().await;

        // Spawn multiple concurrent health checks to stress the pool
        let mut handles = Vec::new();
        for _ in 0..5 {
            let s = storage.clone();
            handles.push(tokio::spawn(async move { s.health_check().await.is_ok() }));
        }

        for h in handles {
            assert!(h.await.unwrap(), "Concurrent health check should succeed");
        }
    }

    #[test]
    fn test_storage_config_default() {
        let config = StorageConfig::default();
        assert_eq!(config.db_path, "prompthub.db");
        assert!(config.wal_mode);
        assert!(config.foreign_keys);
        assert!(config.max_connections > 0);
    }
}
