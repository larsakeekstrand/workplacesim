//! Thin error type for handlers. Axum's `Json` extractor already rejects
//! malformed bodies with 400, so most handlers don't need to produce errors
//! themselves; this exists as a place to hang future fallible paths.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

#[derive(Debug)]
#[allow(dead_code)]
pub enum AppError {
    BadRequest(String),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        match self {
            AppError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg).into_response(),
        }
    }
}
