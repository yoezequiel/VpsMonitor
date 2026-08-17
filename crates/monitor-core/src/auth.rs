//! Tokens, sessions and password hashing. Deliberately no RBAC: any
//! authenticated user has full access — a flat permission model by
//! product decision, not an MVP shortcut.

use argon2::password_hash::rand_core::OsRng;
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use rand::RngCore;
use sha2::{Digest, Sha256};
use shared::{Server, User};

use crate::error::ApiError;
use crate::state::AppState;

pub fn hash_password(password: &str) -> anyhow::Result<String> {
    let salt = SaltString::generate(&mut OsRng);
    let hash = Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map_err(|e| anyhow::anyhow!("failed to hash password: {e}"))?;
    Ok(hash.to_string())
}

pub fn verify_password(password: &str, hash: &str) -> bool {
    let Ok(parsed) = PasswordHash::new(hash) else {
        return false;
    };
    Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .is_ok()
}

/// Generates a random, URL-safe opaque token (agent token, pairing token,
/// session token...). Only the SHA-256 hash of this value is ever
/// persisted — see [`hash_token`].
pub fn generate_token() -> String {
    let mut bytes = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut bytes);
    base64::Engine::encode(&base64::engine::general_purpose::URL_SAFE_NO_PAD, bytes)
}

/// Deterministic hash used to look tokens up in storage without ever
/// storing the token itself. SHA-256 (not Argon2) is intentional here:
/// these tokens are already high-entropy random values, not
/// user-chosen passwords, so a fast, indexable hash is the right trade-off.
pub fn hash_token(token: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    format!("{:x}", hasher.finalize())
}

pub fn extract_bearer(headers: &axum::http::HeaderMap) -> Result<&str, ApiError> {
    headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .ok_or(ApiError::Unauthorized)
}

/// Extracts the logged-in dashboard/CLI user from a session bearer token.
pub struct AuthUser(pub User);

impl FromRequestParts<AppState> for AuthUser {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let token = extract_bearer(&parts.headers)?;
        let user = state
            .store
            .get_user_by_session_token_hash(&hash_token(token))
            .await?
            .ok_or(ApiError::Unauthorized)?;
        Ok(AuthUser(user))
    }
}

/// Extracts the calling Agent from its permanent bearer token, used on
/// `/api/agent/heartbeat` and `/api/agent/metrics`.
pub struct AuthAgent(pub Server);

impl FromRequestParts<AppState> for AuthAgent {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let token = extract_bearer(&parts.headers)?;
        let server = state
            .store
            .get_server_by_token_hash(&hash_token(token))
            .await?
            .ok_or(ApiError::Unauthorized)?;
        Ok(AuthAgent(server))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn password_hash_roundtrips() {
        let hash = hash_password("correct horse battery staple").unwrap();
        assert!(verify_password("correct horse battery staple", &hash));
        assert!(!verify_password("wrong password", &hash));
    }

    #[test]
    fn tokens_are_unique_and_hash_deterministically() {
        let a = generate_token();
        let b = generate_token();
        assert_ne!(a, b);
        assert_eq!(hash_token(&a), hash_token(&a));
        assert_ne!(hash_token(&a), hash_token(&b));
    }
}
