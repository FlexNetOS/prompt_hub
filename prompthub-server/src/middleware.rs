#![forbid(unsafe_code)]

use axum::{body::Body, extract::Request, http::StatusCode, middleware::Next, response::Response};
use std::time::Instant;
use tower_http::{
    cors::{Any, CorsLayer},
    request_id::{MakeRequestUuid, SetRequestIdLayer},
    trace::TraceLayer,
};
use tracing::{error, info, warn};

// ---------------------------------------------------------------------------
// Layer constructors
// ---------------------------------------------------------------------------

/// Create a CORS middleware layer allowing any origin, method, and header.
///
/// In production this should be restricted to known origins.
pub fn create_cors_layer() -> CorsLayer {
    CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any)
        .max_age(std::time::Duration::from_secs(3600))
}

/// Create a Tower HTTP trace layer for request/response logging.
///
/// Uses `tracing` to emit spans for every request with method, URI, status
/// code, and latency.
pub fn create_trace_layer()
-> TraceLayer<tower_http::classify::SharedClassifier<tower_http::classify::ServerErrorsAsFailures>>
{
    TraceLayer::new_for_http()
}

/// Create a request-ID layer that generates a UUID for every incoming request.
///
/// The ID is attached as the `x-request-id` header and is propagated through
/// the trace layer so that every log line for a request can be correlated.
pub fn create_request_id_layer() -> SetRequestIdLayer<MakeRequestUuid> {
    SetRequestIdLayer::x_request_id(MakeRequestUuid)
}

// ---------------------------------------------------------------------------
// Handler-style middleware functions
// ---------------------------------------------------------------------------

/// Request timing middleware.
///
/// Records the HTTP method, path, status code, and duration (in milliseconds)
/// for every request.  Uses structured `tracing` logs at the appropriate level:
///
/// * `ERROR` — 5xx server errors  
/// * `WARN`  — 4xx client errors  
/// * `INFO`  — everything else (2xx, 3xx)
pub async fn request_timing(req: Request<Body>, next: Next) -> Response {
    let start = Instant::now();
    let method = req.method().clone();
    let path = req.uri().path().to_string();

    let response = next.run(req).await;

    let duration = start.elapsed();
    let status = response.status();

    if status.is_server_error() {
        error!(
            method = %method,
            path = %path,
            status = %status.as_u16(),
            duration_ms = %duration.as_millis(),
            "Request failed"
        );
    } else if status.is_client_error() {
        warn!(
            method = %method,
            path = %path,
            status = %status.as_u16(),
            duration_ms = %duration.as_millis(),
            "Client error"
        );
    } else {
        info!(
            method = %method,
            path = %path,
            status = %status.as_u16(),
            duration_ms = %duration.as_millis(),
            "Request complete"
        );
    }

    response
}

/// Error response normalisation middleware.
///
/// Converts bare `500 Internal Server Error` responses into JSON problem
/// responses with a stable body shape so clients can rely on the format.
pub async fn error_handler(req: Request<Body>, next: Next) -> Response {
    let response = next.run(req).await;

    if response.status() == StatusCode::INTERNAL_SERVER_ERROR {
        warn!("Normalising 500 response to JSON error body");
        let body = Body::from(r#"{"error":"internal server error"}"#);
        return Response::builder()
            .status(StatusCode::INTERNAL_SERVER_ERROR)
            .header("content-type", "application/json")
            .body(body)
            .unwrap();
    }

    response
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt;

    #[test]
    fn test_cors_layer_creation() {
        let _layer = create_cors_layer();
    }

    #[test]
    fn test_trace_layer_creation() {
        let _layer = create_trace_layer();
    }

    #[test]
    fn test_request_id_layer_creation() {
        let _layer = create_request_id_layer();
    }

    // ——— Handler middleware smoke tests via a mini-router ———

    async fn ok_handler(_req: Request<Body>) -> Response {
        Response::builder()
            .status(StatusCode::OK)
            .body(Body::empty())
            .unwrap()
    }

    async fn err_handler(_req: Request<Body>) -> Response {
        Response::builder()
            .status(StatusCode::INTERNAL_SERVER_ERROR)
            .body(Body::empty())
            .unwrap()
    }

    #[tokio::test]
    async fn test_request_timing_ok() {
        use axum::{Router, middleware::from_fn};

        let app = Router::new()
            .route("/test", axum::routing::get(ok_handler))
            .layer(from_fn(request_timing));

        let response = app
            .oneshot(Request::builder().uri("/test").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_error_handler_converts_500() {
        use axum::{Router, middleware::from_fn};

        let app = Router::new()
            .route("/error", axum::routing::get(err_handler))
            .layer(from_fn(error_handler));

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/error")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);

        // Verify the body is JSON
        let headers = response.headers();
        assert_eq!(headers.get("content-type").unwrap(), "application/json");
    }
}
