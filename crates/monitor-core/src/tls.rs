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

/// Every local, non-loopback IPv4/IPv6 address on this machine, so a
/// dashboard reached by IP (`https://192.168.1.20:8443`, Grafana-style)
/// gets a certificate that actually matches — not just the hostname.
fn local_ip_sans() -> Vec<String> {
    let mut ips = vec!["127.0.0.1".to_string(), "::1".to_string()];
    if let Ok(interfaces) = if_addrs::get_if_addrs() {
        for iface in interfaces {
            if iface.is_loopback() {
                continue;
            }
            ips.push(iface.ip().to_string());
        }
    }
    ips
}

/// Best-guess LAN-reachable IPv4 address for this machine, used to print a
/// copy-pasteable `monitor install --monitor-url https://<ip>:<port> ...`
/// hint from `monitor server add` — Grafana-style "reach it by IP" access.
pub fn primary_local_ip() -> Option<String> {
    if_addrs::get_if_addrs()
        .ok()?
        .into_iter()
        .find_map(|iface| {
            if iface.is_loopback() || !iface.ip().is_ipv4() {
                return None;
            }
            Some(iface.ip().to_string())
        })
}

pub fn generate_self_signed(hostname: &str) -> anyhow::Result<TlsMaterial> {
    let mut subject_alt_names = vec![hostname.to_string(), "localhost".to_string()];
    subject_alt_names.extend(local_ip_sans());

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
