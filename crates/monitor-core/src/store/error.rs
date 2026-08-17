use thiserror::Error;

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("not found")]
    NotFound,
    #[error("conflict: {0}")]
    Conflict(String),
    #[error("backend error: {0}")]
    Backend(#[from] anyhow::Error),
}
