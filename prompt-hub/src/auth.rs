#![forbid(unsafe_code)]

use crate::error::{HubError, Result};
use crate::models::{AgentIdentity, Capability, Prompt};
use argon2::{
    password_hash::{rand_core::OsRng, SaltString},
    Argon2, PasswordHash, PasswordHasher, PasswordVerifier,
};
use std::collections::HashSet;
use tracing::{info, instrument, warn};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Action enum
// ---------------------------------------------------------------------------

/// Actions that require authorization.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Action {
    Read,
    Write,
    Delete,
    Admin,
    Transfer,
    Lock,
    Evolve,
}

// ---------------------------------------------------------------------------
// AuthManager trait
// ---------------------------------------------------------------------------

/// Trait for authentication and authorization backends.
/// Uses native async fn in traits (Rust 2024 Edition).
pub trait AuthManager: Send + Sync {
    /// Authenticate a bearer token and return the corresponding identity.
    async fn authenticate(&self, token: &str) -> Result<AgentIdentity>;

    /// Authorize `identity` to perform `action` on `resource`.
    async fn authorize(
        &self,
        identity: &AgentIdentity,
        action: Action,
        resource: &Prompt,
    ) -> Result<()>;
}

// ---------------------------------------------------------------------------
// RBAC Auth Manager
// ---------------------------------------------------------------------------

/// Concrete RBAC-enforcing authentication manager.
#[derive(Debug, Clone)]
pub struct RbacAuthManager;

impl RbacAuthManager {
    pub fn new() -> Self {
        Self
    }

    // ── Token hashing / verification ────────────────────────────────────────

    /// Hash a plaintext token using **argon2id** (default parameters).
    pub fn hash_token(token: &str) -> Result<String> {
        let salt = SaltString::generate(&mut OsRng);
        let argon2 = Argon2::default();
        argon2
            .hash_password(token.as_bytes(), &salt)
            .map(|h| h.to_string())
            .map_err(|e| HubError::AuthError(format!("Hash failed: {e}")))
    }

    /// Verify a plaintext token against a stored argon2id hash.
    pub fn verify_token(token: &str, hash: &str) -> Result<bool> {
        let parsed_hash =
            PasswordHash::new(hash).map_err(|e| HubError::AuthError(format!("Parse hash: {e}")))?;
        let argon2 = Argon2::default();
        Ok(argon2.verify_password(token.as_bytes(), &parsed_hash).is_ok())
    }

    // ── Capability helpers ──────────────────────────────────────────────────

    /// Returns `true` if `identity` holds `cap` **or** the blanket `Admin`
    /// capability.
    pub fn has_capability(identity: &AgentIdentity, cap: Capability) -> bool {
        identity.capabilities.contains(&cap)
            || identity.capabilities.contains(&Capability::Admin)
    }

    /// Check that `identity` is authorised to perform `action`.
    ///
    /// Returns `Ok(())` when authorised, `Err(HubError::Unauthorized)`
    /// otherwise.
    #[instrument(skip(identity))]
    pub fn authorize_action(identity: &AgentIdentity, action: Action) -> Result<()> {
        let required = match action {
            Action::Read => Capability::Read,
            Action::Write | Action::Lock | Action::Evolve => Capability::Write,
            Action::Delete | Action::Admin | Action::Transfer => Capability::Admin,
        };

        if Self::has_capability(identity, required) {
            Ok(())
        } else {
            warn!(
                "Authorization denied: {:?} lacks {:?} for {:?}",
                identity.id, required, action
            );
            Err(HubError::Unauthorized(format!(
                "agent '{}' lacks capability {:?} for {:?}",
                identity.name, required, action
            )))
        }
    }

    // ── Ownership transfer ──────────────────────────────────────────────────

    /// Validate that ownership may be transferred from `from` to `to` by
    /// `admin`.  Admin must hold `Capability::Admin`.
    #[instrument(skip(from, to, admin))]
    pub fn can_transfer_ownership(
        from: &AgentIdentity,
        to: &AgentIdentity,
        admin: &AgentIdentity,
    ) -> Result<()> {
        // Admin must have Admin capability
        Self::authorize_action(admin, Action::Transfer)?;

        // Cannot transfer to self
        if from.id == to.id {
            return Err(HubError::BadRequest(
                "Cannot transfer ownership to same owner".to_string(),
            ));
        }

        info!(
            "Ownership transfer authorized: {:?} -> {:?} by {:?}",
            from.id, to.id, admin.id
        );
        Ok(())
    }

    // ── Specialization scoring ──────────────────────────────────────────────

    /// Update the specialization score using an **exponential moving average**.
    ///
    /// Formula: `new_score = 0.9 * current_score + 0.1 * task_success`
    pub fn update_specialization_score(current: f64, task_success: f64) -> f64 {
        0.9_f64.mul_add(current, 0.1 * task_success.clamp(0.0, 1.0))
    }

    /// Check rate-limit bypass cooldown: admin override is allowed only once
    /// per 24-hour window.
    #[instrument]
    pub fn check_rate_limit_bypass_cooldown(
        last_bypass_timestamp: Option<chrono::DateTime<chrono::Utc>>,
    ) -> Result<()> {
        if let Some(last) = last_bypass_timestamp {
            let cooldown = chrono::Duration::hours(24);
            let now = chrono::Utc::now();
            if now - last < cooldown {
                return Err(HubError::RateLimited(
                    "Rate limit bypass cooldown is still active".to_string(),
                ));
            }
        }
        Ok(())
    }
}

impl Default for RbacAuthManager {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Bootstrap helpers
// ---------------------------------------------------------------------------

/// Create a bootstrap admin identity with a fresh random token.
///
/// Returns `(identity, plaintext_token)`.  The caller **must** persist the
/// identity and securely store or discard the plaintext token.
pub fn create_bootstrap_admin() -> (AgentIdentity, String) {
    let token = Uuid::new_v4().to_string();
    let token_hash = RbacAuthManager::hash_token(&token).unwrap_or_default();

    let mut capabilities = HashSet::new();
    capabilities.insert(Capability::Read);
    capabilities.insert(Capability::Write);
    capabilities.insert(Capability::Admin);

    let identity = AgentIdentity {
        id: Uuid::new_v4(),
        name: "admin".to_string(),
        capabilities,
        token_hash,
        specialization_score: 1.0,
    };

    (identity, token)
}

// ============================================================================
// Tests
// ============================================================================
#[cfg(test)]
mod tests {
    use super::*;

    // ── Token hashing ───────────────────────────────────────────────────────

    #[test]
    fn test_hash_and_verify() {
        let token = "test-token-123";
        let hash = RbacAuthManager::hash_token(token).unwrap();
        assert!(
            RbacAuthManager::verify_token(token, &hash).unwrap(),
            "Correct token should verify"
        );
        assert!(
            !RbacAuthManager::verify_token("wrong-token", &hash).unwrap(),
            "Wrong token should fail"
        );
    }

    #[test]
    fn test_hash_unique_salts() {
        let token = "same-token";
        let h1 = RbacAuthManager::hash_token(token).unwrap();
        let h2 = RbacAuthManager::hash_token(token).unwrap();
        // Each hash should be unique because of random salts
        assert_ne!(h1, h2, "Hashes with different salts must differ");
        // Both should still verify
        assert!(RbacAuthManager::verify_token(token, &h1).unwrap());
        assert!(RbacAuthManager::verify_token(token, &h2).unwrap());
    }

    // ── Authorization ───────────────────────────────────────────────────────

    #[test]
    fn test_authorize_read() {
        let mut caps = HashSet::new();
        caps.insert(Capability::Read);
        let identity = AgentIdentity {
            id: Uuid::new_v4(),
            name: "reader".to_string(),
            capabilities: caps,
            token_hash: "hash".to_string(),
            specialization_score: 0.5,
        };
        assert!(RbacAuthManager::authorize_action(&identity, Action::Read).is_ok());
        assert!(RbacAuthManager::authorize_action(&identity, Action::Write).is_err());
        assert!(RbacAuthManager::authorize_action(&identity, Action::Delete).is_err());
    }

    #[test]
    fn test_admin_has_all_capabilities() {
        let mut caps = HashSet::new();
        caps.insert(Capability::Admin);
        let admin = AgentIdentity {
            id: Uuid::new_v4(),
            name: "admin".to_string(),
            capabilities: caps,
            token_hash: "hash".to_string(),
            specialization_score: 1.0,
        };
        assert!(RbacAuthManager::authorize_action(&admin, Action::Read).is_ok());
        assert!(RbacAuthManager::authorize_action(&admin, Action::Write).is_ok());
        assert!(RbacAuthManager::authorize_action(&admin, Action::Delete).is_ok());
        assert!(RbacAuthManager::authorize_action(&admin, Action::Transfer).is_ok());
        assert!(RbacAuthManager::authorize_action(&admin, Action::Lock).is_ok());
        assert!(RbacAuthManager::authorize_action(&admin, Action::Evolve).is_ok());
    }

    #[test]
    fn test_writer_can_lock_and_evolve() {
        let mut caps = HashSet::new();
        caps.insert(Capability::Write);
        let identity = AgentIdentity {
            id: Uuid::new_v4(),
            name: "writer".to_string(),
            capabilities: caps,
            token_hash: "hash".to_string(),
            specialization_score: 0.5,
        };
        assert!(RbacAuthManager::authorize_action(&identity, Action::Lock).is_ok());
        assert!(RbacAuthManager::authorize_action(&identity, Action::Evolve).is_ok());
        assert!(RbacAuthManager::authorize_action(&identity, Action::Delete).is_err());
    }

    // ── Ownership transfer ──────────────────────────────────────────────────

    #[test]
    fn test_transfer_ownership_ok() {
        let mut admin_caps = HashSet::new();
        admin_caps.insert(Capability::Admin);
        let admin = AgentIdentity {
            id: Uuid::new_v4(),
            name: "admin".to_string(),
            capabilities: admin_caps,
            token_hash: "hash".to_string(),
            specialization_score: 1.0,
        };
        let from = AgentIdentity {
            id: Uuid::new_v4(),
            name: "from".to_string(),
            capabilities: HashSet::new(),
            token_hash: "hash".to_string(),
            specialization_score: 0.5,
        };
        let to = AgentIdentity {
            id: Uuid::new_v4(),
            name: "to".to_string(),
            capabilities: HashSet::new(),
            token_hash: "hash".to_string(),
            specialization_score: 0.5,
        };
        assert!(RbacAuthManager::can_transfer_ownership(&from, &to, &admin).is_ok());
    }

    #[test]
    fn test_transfer_ownership_same_owner() {
        let mut admin_caps = HashSet::new();
        admin_caps.insert(Capability::Admin);
        let admin = AgentIdentity {
            id: Uuid::new_v4(),
            name: "admin".to_string(),
            capabilities: admin_caps,
            token_hash: "hash".to_string(),
            specialization_score: 1.0,
        };
        let owner = AgentIdentity {
            id: Uuid::new_v4(),
            name: "owner".to_string(),
            capabilities: HashSet::new(),
            token_hash: "hash".to_string(),
            specialization_score: 0.5,
        };
        let result = RbacAuthManager::can_transfer_ownership(&owner, &owner, &admin);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), HubError::BadRequest(_)));
    }

    #[test]
    fn test_transfer_ownership_non_admin_fails() {
        let mut read_caps = HashSet::new();
        read_caps.insert(Capability::Read);
        let non_admin = AgentIdentity {
            id: Uuid::new_v4(),
            name: "reader".to_string(),
            capabilities: read_caps,
            token_hash: "hash".to_string(),
            specialization_score: 0.5,
        };
        let from = AgentIdentity {
            id: Uuid::new_v4(),
            name: "from".to_string(),
            capabilities: HashSet::new(),
            token_hash: "hash".to_string(),
            specialization_score: 0.5,
        };
        let to = AgentIdentity {
            id: Uuid::new_v4(),
            name: "to".to_string(),
            capabilities: HashSet::new(),
            token_hash: "hash".to_string(),
            specialization_score: 0.5,
        };
        let result = RbacAuthManager::can_transfer_ownership(&from, &to, &non_admin);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), HubError::Unauthorized(_)));
    }

    // ── Specialization scoring ──────────────────────────────────────────────

    #[test]
    fn test_specialization_score_ema() {
        let current = 0.5;
        let task_success = 1.0;
        let new_score = RbacAuthManager::update_specialization_score(current, task_success);
        // 0.9 * 0.5 + 0.1 * 1.0 = 0.45 + 0.1 = 0.55
        assert!((new_score - 0.55).abs() < 0.001, "EMA update incorrect: {new_score}");
    }

    #[test]
    fn test_specialization_score_clamped() {
        let result = RbacAuthManager::update_specialization_score(0.5, 5.0);
        // task_success should be clamped to 1.0
        let expected = 0.9 * 0.5 + 0.1 * 1.0;
        assert!((result - expected).abs() < 0.001);
    }

    #[test]
    fn test_specialization_score_converges() {
        let mut score = 0.0;
        for _ in 0..100 {
            score = RbacAuthManager::update_specialization_score(score, 1.0);
        }
        // Should converge close to 1.0
        assert!(score > 0.99, "Score should converge to ~1.0: {score}");
    }

    // ── Rate limit bypass cooldown ──────────────────────────────────────────

    #[test]
    fn test_rate_limit_bypass_no_previous() {
        assert!(RbacAuthManager::check_rate_limit_bypass_cooldown(None).is_ok());
    }

    #[test]
    fn test_rate_limit_bypass_cooldown_active() {
        let recent = Some(chrono::Utc::now());
        assert!(RbacAuthManager::check_rate_limit_bypass_cooldown(recent).is_err());
    }

    #[test]
    fn test_rate_limit_bypass_cooldown_expired() {
        let old = Some(chrono::Utc::now() - chrono::Duration::hours(25));
        assert!(RbacAuthManager::check_rate_limit_bypass_cooldown(old).is_ok());
    }

    // ── Bootstrap ───────────────────────────────────────────────────────────

    #[test]
    fn test_bootstrap_admin() {
        let (admin, token) = create_bootstrap_admin();
        assert_eq!(admin.name, "admin");
        assert!(admin.capabilities.contains(&Capability::Read));
        assert!(admin.capabilities.contains(&Capability::Write));
        assert!(admin.capabilities.contains(&Capability::Admin));
        assert!(RbacAuthManager::verify_token(&token, &admin.token_hash).unwrap());
        assert_eq!(admin.specialization_score, 1.0);
    }

    // ── SwarmOnly capability ────────────────────────────────────────────────

    #[test]
    fn test_swarm_only_capability() {
        let mut caps = HashSet::new();
        caps.insert(Capability::SwarmOnly);
        let identity = AgentIdentity {
            id: Uuid::new_v4(),
            name: "swarm".to_string(),
            capabilities: caps.clone(),
            token_hash: "hash".to_string(),
            specialization_score: 0.5,
        };
        // SwarmOnly does not grant Read/Write/Admin
        assert!(RbacAuthManager::authorize_action(&identity, Action::Read).is_err());
        // But has_capability should work for SwarmOnly specifically
        assert!(RbacAuthManager::has_capability(&identity, Capability::SwarmOnly));
    }

    // ── Send / Sync ─────────────────────────────────────────────────────────

    #[test]
    fn test_rbac_auth_manager_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<RbacAuthManager>();
    }
}
