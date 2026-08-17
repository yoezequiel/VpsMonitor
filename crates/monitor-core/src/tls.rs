//! Self-signed TLS certificate generated on first run when the user hasn't
//! brought their own. Not meant to be trusted by public CAs — Agents pin
//! the Monitor's certificate at pairing time instead of relying on a chain
//! of trust, which is the right model for a self-hosted, LAN-facing tool.

use std::path::Path;

use rcgen::{generate_simple_self_signed, CertifiedKey};

pub struct TlsMaterial {
    pub cert_pem: String,
    pub key_pem: String,
}

pub fn generate_self_signed(hostname: &str) -> anyhow::Result<TlsMaterial> {
    let subject_alt_names = vec![hostname.to_string(), "localhost".to_string()];
    let CertifiedKey { cert, key_pair } = generate_simple_self_signed(subject_alt_names)
        .map_err(|e| anyhow::anyhow!("failed to generate self-signed certificate: {e}"))?;
    Ok(TlsMaterial {
        cert_pem: cert.pem(),
        key_pem: key_pair.serialize_pem(),
    })
}

/// Loads an existing cert/key pair from disk, generating and persisting a
/// new self-signed one on first run.
pub async fn load_or_generate(
    cert_path: &Path,
    key_path: &Path,
    hostname: &str,
) -> anyhow::Result<TlsMaterial> {
    if cert_path.exists() && key_path.exists() {
        return Ok(TlsMaterial {
            cert_pem: tokio::fs::read_to_string(cert_path).await?,
            key_pem: tokio::fs::read_to_string(key_path).await?,
        });
    }

    let material = generate_self_signed(hostname)?;
    if let Some(parent) = cert_path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    tokio::fs::write(cert_path, &material.cert_pem).await?;
    tokio::fs::write(key_path, &material.key_pem).await?;
    Ok(material)
}
