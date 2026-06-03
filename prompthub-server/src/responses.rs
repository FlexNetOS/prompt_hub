#![forbid(unsafe_code)]

use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Serialize;

/// Standard API response wrapper.
///
/// All successful handler responses use this envelope so clients can rely on
/// a consistent `success` flag and a uniform shape regardless of endpoint.
#[derive(Debug, Serialize)]
pub struct ApiResponse<T> {
    pub success: bool,
    pub data: Option<T>,
    pub error: Option<String>,
}

/// Concrete JSON-value API response used for OpenAPI documentation.
///
/// utoipa cannot derive `ToSchema` for generic types, so this non-generic
/// alias is registered in the spec instead.
#[cfg(feature = "utoipa")]
#[derive(Debug, serde::Serialize, utoipa::ToSchema)]
pub struct ApiResponseDoc {
    pub success: bool,
    #[schema(value_type = Object, nullable = true)]
    pub data: Option<serde_json::Value>,
    #[schema(value_type = Option<String>)]
    pub error: Option<String>,
}

/// Standard error response returned for every failed request.
///
/// Includes the HTTP status code duplicated in the body so that log processors
/// can correlate without inspecting headers.
#[derive(Debug, Serialize)]
#[cfg_attr(feature = "utoipa", derive(utoipa::ToSchema))]
pub struct ErrorResponse {
    pub success: bool,
    pub error: String,
    pub code: u16,
}

impl ErrorResponse {
    /// Create a new error response.
    pub fn new(error: impl Into<String>, code: u16) -> Self {
        Self {
            success: false,
            error: error.into(),
            code,
        }
    }
}

impl IntoResponse for ErrorResponse {
    fn into_response(self) -> Response {
        let status = StatusCode::from_u16(self.code).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
        (status, Json(self)).into_response()
    }
}

/// Helper: wrap serialisable data in a success envelope with HTTP 200.
pub fn success<T: Serialize>(data: T) -> (StatusCode, Json<ApiResponse<T>>) {
    (
        StatusCode::OK,
        Json(ApiResponse {
            success: true,
            data: Some(data),
            error: None,
        }),
    )
}

/// Helper: build an error tuple from a status code and message.
///
/// The status code is used both as the HTTP status and embedded in the JSON
/// body so that generic log parsers can extract it without header inspection.
pub fn error(status: StatusCode, message: impl Into<String>) -> (StatusCode, Json<ErrorResponse>) {
    (status, Json(ErrorResponse::new(message, status.as_u16())))
}
