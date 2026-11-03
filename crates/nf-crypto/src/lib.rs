//! NIGHTFALL cryptography helpers.
//!
//! - XChaCha20-Poly1305 authenticated encryption (192-bit random nonces)
//! - HKDF-SHA256 key derivation
//! - HMAC-SHA256 tags
//! - 256-bit keys zeroized on drop
//!
//! No custom primitives: everything comes from the RustCrypto crates.

use chacha20poly1305::aead::{Aead, KeyInit, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use hkdf::Hkdf;
use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};
use zeroize::{Zeroize, ZeroizeOnDrop};

pub use base64::engine::general_purpose::STANDARD as B64;
pub use base64::Engine as _;

const NONCE_LEN: usize = 24;
const TAG_LEN: usize = 16;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum CryptoError {
    #[error("operating system random generator failed")]
    Random,
    #[error("ciphertext is too short")]
    Truncated,
    #[error("authentication failed: wrong key or tampered data")]
    Authentication,
    #[error("invalid key length")]
    KeyLength,
}

/// A 256-bit secret key, wiped from memory when dropped.
#[derive(Clone, Zeroize, ZeroizeOnDrop, PartialEq, Eq)]
pub struct SecretKey([u8; 32]);

impl std::fmt::Debug for SecretKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SecretKey([redacted])")
    }
}

impl SecretKey {
    /// Fresh random key from the OS CSPRNG.
    pub fn generate() -> Result<Self, CryptoError> {
        let mut k = [0u8; 32];
        fill_random(&mut k)?;
        Ok(Self(k))
    }

    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    pub fn from_slice(bytes: &[u8]) -> Result<Self, CryptoError> {
        let arr: [u8; 32] = bytes.try_into().map_err(|_| CryptoError::KeyLength)?;
        Ok(Self(arr))
    }

    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Derive an independent sub-key for `purpose` (HKDF-SHA256, no salt).
    pub fn derive(&self, purpose: &str) -> SecretKey {
        hkdf_derive(&self.0, None, purpose.as_bytes())
    }

    /// Hex of the bytes, for SQLCipher's raw key syntax. Caller must zeroize.
    pub fn to_hex(&self) -> String {
        self.0.iter().map(|b| format!("{b:02x}")).collect()
    }
}

/// Fill `buf` with cryptographically secure random bytes.
pub fn fill_random(buf: &mut [u8]) -> Result<(), CryptoError> {
    getrandom::getrandom(buf).map_err(|_| CryptoError::Random)
}

/// Random bytes as a vector.
pub fn random_bytes(n: usize) -> Result<Vec<u8>, CryptoError> {
    let mut v = vec![0u8; n];
    fill_random(&mut v)?;
    Ok(v)
}

/// HKDF-SHA256 expand to a 256-bit key.
pub fn hkdf_derive(ikm: &[u8], salt: Option<&[u8]>, info: &[u8]) -> SecretKey {
    let hk = Hkdf::<Sha256>::new(salt, ikm);
    let mut okm = [0u8; 32];
    // 32 bytes is always a valid HKDF-SHA256 output length.
    hk.expand(info, &mut okm).expect("valid HKDF length");
    SecretKey(okm)
}

/// Encrypt `plaintext`; output = nonce (24) || ciphertext || tag (16).
/// `aad` is authenticated but not encrypted (use it to bind context).
pub fn seal(key: &SecretKey, plaintext: &[u8], aad: &[u8]) -> Result<Vec<u8>, CryptoError> {
    let cipher = XChaCha20Poly1305::new(chacha20poly1305::Key::from_slice(key.as_bytes()));
    let mut nonce = [0u8; NONCE_LEN];
    fill_random(&mut nonce)?;
    let ct = cipher
        .encrypt(XNonce::from_slice(&nonce), Payload { msg: plaintext, aad })
        .map_err(|_| CryptoError::Authentication)?;
    let mut out = Vec::with_capacity(NONCE_LEN + ct.len());
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&ct);
    Ok(out)
}

/// Decrypt data produced by [`seal`] with the same key and `aad`.
pub fn open(key: &SecretKey, sealed: &[u8], aad: &[u8]) -> Result<Vec<u8>, CryptoError> {
    if sealed.len() < NONCE_LEN + TAG_LEN {
        return Err(CryptoError::Truncated);
    }
    let (nonce, ct) = sealed.split_at(NONCE_LEN);
    let cipher = XChaCha20Poly1305::new(chacha20poly1305::Key::from_slice(key.as_bytes()));
    cipher
        .decrypt(XNonce::from_slice(nonce), Payload { msg: ct, aad })
        .map_err(|_| CryptoError::Authentication)
}

/// HMAC-SHA256 tag.
pub fn hmac_tag(key: &SecretKey, data: &[u8]) -> [u8; 32] {
    let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(key.as_bytes()).expect("any key size");
    mac.update(data);
    mac.finalize().into_bytes().into()
}

/// Constant-time HMAC verification.
pub fn hmac_verify(key: &SecretKey, data: &[u8], tag: &[u8]) -> bool {
    let mut mac = <Hmac<Sha256> as Mac>::new_from_slice(key.as_bytes()).expect("any key size");
    mac.update(data);
    mac.verify_slice(tag).is_ok()
}

/// SHA-256 digest.
pub fn sha256(data: &[u8]) -> [u8; 32] {
    Sha256::digest(data).into()
}

/// Lowercase hex encoding.
pub fn hex(data: &[u8]) -> String {
    data.iter().map(|b| format!("{b:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seal_open_roundtrip() {
        let k = SecretKey::generate().unwrap();
        let sealed = seal(&k, b"top secret", b"ctx").unwrap();
        assert_eq!(sealed.len(), 24 + 10 + 16);
        assert_eq!(open(&k, &sealed, b"ctx").unwrap(), b"top secret");
    }

    #[test]
    fn wrong_key_or_aad_or_tamper_fails() {
        let k = SecretKey::generate().unwrap();
        let other = SecretKey::generate().unwrap();
        let mut sealed = seal(&k, b"data", b"ctx").unwrap();
        assert_eq!(open(&other, &sealed, b"ctx"), Err(CryptoError::Authentication));
        assert_eq!(open(&k, &sealed, b"other"), Err(CryptoError::Authentication));
        let last = sealed.len() - 1;
        sealed[last] ^= 1;
        assert_eq!(open(&k, &sealed, b"ctx"), Err(CryptoError::Authentication));
        assert_eq!(open(&k, &[0u8; 10], b"ctx"), Err(CryptoError::Truncated));
    }

    #[test]
    fn nonces_are_unique() {
        let k = SecretKey::generate().unwrap();
        let a = seal(&k, b"x", b"").unwrap();
        let b = seal(&k, b"x", b"").unwrap();
        assert_ne!(a, b);
    }

    #[test]
    fn derive_is_deterministic_and_separated() {
        let k = SecretKey::from_bytes([7u8; 32]);
        assert_eq!(k.derive("cookies"), k.derive("cookies"));
        assert_ne!(k.derive("cookies"), k.derive("history"));
    }

    #[test]
    fn hmac_roundtrip() {
        let k = SecretKey::from_bytes([1u8; 32]);
        let t = hmac_tag(&k, b"counter=3");
        assert!(hmac_verify(&k, b"counter=3", &t));
        assert!(!hmac_verify(&k, b"counter=0", &t));
    }

    #[test]
    fn debug_does_not_leak() {
        let k = SecretKey::from_bytes([9u8; 32]);
        assert_eq!(format!("{k:?}"), "SecretKey([redacted])");
    }
}
