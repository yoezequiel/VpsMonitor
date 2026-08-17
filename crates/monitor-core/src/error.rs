use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

use crate::store::StoreError;

#[derive(Debug)]
pub enum ApiError {
    Unauthorized,
    NotFound,
    BadRequest(String),
    Internal(anyhow::Error),
}

impl From<StoreError> for ApiError {
    fn from(err: StoreError) -> Self {
        match err {
            StoreError::NotFound => ApiError::NotFound,
            StoreError::Conflict(msg) => ApiError::BadRequest(msg),
            StoreError::Backend(e) => ApiError::Internal(e),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            ApiError::Unauthorized => (StatusCode::UNAUTHORIZED, "unauthorized".to_string()),
            ApiError::NotFound => (StatusCode::NOT_FOUND, "not found".to_string()),
            ApiError::BadRequest(msg) => (StatusCode::BAD_REQUEST, msg),
            ApiError::Internal(err) => {
                tracing::error!(%err, "internal error");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal error".to_string(),
                )
            }
        };
        (status, Json(json!({ "error": message }))).into_response()
    }
}
