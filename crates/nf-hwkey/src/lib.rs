//! Device-bound secret used by NIGHTFALL LOCK.
//!
//! The PIN alone (6 to 12 digits) has too little entropy to resist an offline
//! brute-force attack. The key that unwraps the master key therefore mixes the
//! Argon2id output of the PIN with a secret that cannot leave this device:
//!
//! | Platform | Backend | Exportable? |
//! |---|---|---|
//! | Windows with TPM 2.0 | RSA key in the TPM (Microsoft Platform Crypto Provider), deterministic PKCS#1 signature of a context | No |
//! | Windows without TPM | Random secret protected by DPAPI (bound to the Windows account, not to the hardware) | Weaker |
//! | macOS | Random secret stored in the login Keychain, access bound to the signed application | No (without user consent) |
//! | Linux / tests | Random secret in an owner-only file | Yes: development only |
//!
//! No biometric API is used anywhere.

use nf_crypto::SecretKey;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[cfg(target_os = "macos")]
mod macos;
mod software;
#[cfg(target_os = "windows")]
mod windows_impl;

pub use software::SoftwareSecret;

#[derive(Debug, thiserror::Error)]
pub enum HwKeyError {
    #[error("hardware key store unavailable: {0}")]
    Unavailable(String),
    #[error("device secret not found (profile copied from another device?)")]
    NotFound,
    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),
    #[error("crypto error: {0}")]
    Crypto(#[from] nf_crypto::CryptoError),
}

/// Which backend protects the device secret. Stored in the key store so the
/// same backend is reopened at the next start.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Backend {
    /// Windows TPM 2.0 key, identified by its persisted key name.
    Tpm { key_name: String },
    /// Windows DPAPI-protected secret file.
    Dpapi { file: PathBuf },
    /// macOS Keychain generic password.
    Keychain { service: String, account: String },
    /// Plain owner-only file (Linux development, CI).
    Software { file: PathBuf },
}

impl Backend {
    /// True when the secret cannot be copied off the device by reading files.
    pub fn is_hardware_backed(&self) -> bool {
        matches!(self, Backend::Tpm { .. } | Backend::Keychain { .. })
    }

    /// Human readable description for the Security Center.
    pub fn describe(&self) -> &'static str {
        match self {
            Backend::Tpm { .. } => "Puce TPM 2.0 (clé non exportable)",
            Backend::Dpapi { .. } => "DPAPI Windows (liée à la session, sans TPM)",
            Backend::Keychain { .. } => "Trousseau macOS (limité à cet appareil)",
            Backend::Software { .. } => "Fichier local (développement uniquement)",
        }
    }
}

/// A device-bound secret provider.
pub trait DeviceSecret: Send + Sync {
    fn backend(&self) -> Backend;
    /// Deterministic 256-bit secret for `context`. The same device and
    /// context always give the same value; another device cannot compute it.
    fn secret(&self, context: &str) -> Result<SecretKey, HwKeyError>;
    /// Permanently destroy the secret (used by the emergency purge).
    fn destroy(&self) -> Result<(), HwKeyError>;
}

/// Create a brand new device secret on the best backend available.
/// `install_id` makes key names unique per installation.
pub fn create(profile: &nf_core::ProfilePaths, install_id: &str) -> Result<Box<dyn DeviceSecret>, HwKeyError> {
    #[cfg(target_os = "windows")]
    {
        match windows_impl::Tpm::create(install_id) {
            Ok(tpm) => return Ok(Box::new(tpm)),
            Err(_) => {
                let d = windows_impl::Dpapi::create(profile.root.join("device.secret.dpapi"))?;
                return Ok(Box::new(d));
            }
        }
    }
    #[cfg(target_os = "macos")]
    {
        let _ = profile;
        return Ok(Box::new(macos::Keychain::create(install_id)?));
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        let _ = install_id;
        Ok(Box::new(SoftwareSecret::create(profile.software_secret())?))
    }
}

/// Reopen the device secret described by `backend`.
pub fn open(backend: &Backend) -> Result<Box<dyn DeviceSecret>, HwKeyError> {
    match backend {
        Backend::Software { file } => Ok(Box::new(SoftwareSecret::open(file.clone())?)),
        #[cfg(target_os = "windows")]
        Backend::Tpm { key_name } => Ok(Box::new(windows_impl::Tpm::open(key_name)?)),
        #[cfg(target_os = "windows")]
        Backend::Dpapi { file } => Ok(Box::new(windows_impl::Dpapi::open(file.clone())?)),
        #[cfg(target_os = "macos")]
        Backend::Keychain { service, account } => {
            Ok(Box::new(macos::Keychain::open(service, account)?))
        }
        #[allow(unreachable_patterns)]
        other => Err(HwKeyError::Unavailable(format!("{other:?} not supported on this OS"))),
    }
}

/// Derive the context secret from a stored random seed (non-TPM backends).
pub(crate) fn derive_from_seed(seed: &[u8], context: &str) -> SecretKey {
    nf_crypto::hkdf_derive(seed, Some(b"nightfall/device-secret/v1"), context.as_bytes())
}
