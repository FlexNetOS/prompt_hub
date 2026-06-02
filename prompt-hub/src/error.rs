#![forbid(unsafe_code)]

use thiserror::Error;

/// Central error type for the prompt-hub library
#[derive(Error, Debug, Clone)]
pub enum HubError {
    #[error("Internal error: {0}")]
    Internal(String),

    #[error("Invalid input: {0}")]
    InvalidInput(String),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Unauthorized: {0}")]
    Unauthorized(String),

    #[error("Conflict: {0}")]
    Conflict(String),

    #[error("Rate limited: {0}")]
    RateLimited(String),

    #[error("Timeout: {0}")]
    Timeout(String),

    #[error("Validation failed: {0}")]
    Validation(String),

    #[error("Validation error: {0}")]
    ValidationError(String),

    #[error("Bad request: {0}")]
    BadRequest(String),

    #[error("Auth error: {0}")]
    AuthError(String),

    #[error("Audit error: {0}")]
    AuditError(String),

    #[error("Lock error: {0}")]
    LockError(String),

    #[error("Storage error: {0}")]
    StorageError(String),

    #[error("Search error: {0}")]
    SearchError(String),

    #[error("Serialization error: {0}")]
    SerdeError(String),

    #[error("Sync error: {0}")]
    SyncError(String),

    #[error("Sanitization error: {0}")]
    SanitizationError(String),

    #[error("IO error: {0}")]
    Io(String),

    #[error("Serialization error: {0}")]
    Serialization(String),

    #[error("Network error: {0}")]
    Network(String),

    #[error("Database error: {0}")]
    Database(String),

    #[error("Plugin error: {0}")]
    Plugin(String),

    #[error("Security violation: {0}")]
    Security(String),

    #[error("Fallback exhausted: {0}")]
    FallbackExhausted(String),

    #[error("Cost exceeded: {0}")]
    CostExceeded(String),

    #[error("Aborted: {0}")]
    Aborted(String),
}

/// Convenience result type
pub type Result<T> = std::result::Result<T, HubError>;

impl From<std::io::Error> for HubError {
    fn from(e: std::io::Error) -> Self {
        HubError::Io(e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hub_error_display() {
        let e = HubError::NotFound("test-id".to_string());
        assert!(format!("{}", e).contains("test-id"));
    }

    #[test]
    fn test_hub_error_from_io() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file missing");
        let hub_err: HubError = io_err.into();
        assert!(matches!(hub_err, HubError::Io(_)));
    }

    #[test]
    fn test_result_type_alias() {
        let r: Result<i32> = Ok(42);
        assert!(r.is_ok());
    }

    #[test]
    fn test_all_error_variants() {
        let variants = vec![
            HubError::Internal("test".to_string()),
            HubError::InvalidInput("test".to_string()),
            HubError::NotFound("test".to_string()),
            HubError::Unauthorized("test".to_string()),
            HubError::Conflict("test".to_string()),
            HubError::RateLimited("test".to_string()),
            HubError::Timeout("test".to_string()),
            HubError::Validation("test".to_string()),
            HubError::ValidationError("test".to_string()),
            HubError::BadRequest("test".to_string()),
            HubError::AuthError("test".to_string()),
            HubError::AuditError("test".to_string()),
            HubError::LockError("test".to_string()),
            HubError::StorageError("test".to_string()),
            HubError::SearchError("test".to_string()),
            HubError::SerdeError("test".to_string()),
            HubError::SyncError("test".to_string()),
            HubError::SanitizationError("test".to_string()),
            HubError::Io("test".to_string()),
            HubError::Serialization("test".to_string()),
            HubError::Network("test".to_string()),
            HubError::Database("test".to_string()),
            HubError::Plugin("test".to_string()),
            HubError::Security("test".to_string()),
            HubError::FallbackExhausted("test".to_string()),
            HubError::CostExceeded("test".to_string()),
            HubError::Aborted("test".to_string()),
        ];
        for e in variants {
            let s = format!("{}", e);
            assert!(!s.is_empty(), "Error variant should have display text");
        }
    }
}
