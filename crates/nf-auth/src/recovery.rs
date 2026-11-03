//! Recovery Key: 160 random bits shown once as 32 Crockford Base32 characters
//! (8 groups of 4). Never stored: only the master key wrapped by it is kept.

use zeroize::{Zeroize, ZeroizeOnDrop};

const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
pub const RECOVERY_BYTES: usize = 20;

#[derive(Clone, Zeroize, ZeroizeOnDrop, PartialEq, Eq)]
pub struct RecoveryKey([u8; RECOVERY_BYTES]);

impl std::fmt::Debug for RecoveryKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("RecoveryKey([redacted])")
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum RecoveryParseError {
    #[error("la Recovery Key doit contenir 32 caractères")]
    Length,
    #[error("caractère invalide dans la Recovery Key : {0}")]
    Char(char),
}

impl RecoveryKey {
    pub fn generate() -> Result<Self, nf_crypto::CryptoError> {
        let mut b = [0u8; RECOVERY_BYTES];
        nf_crypto::fill_random(&mut b)?;
        Ok(Self(b))
    }

    pub fn as_bytes(&self) -> &[u8; RECOVERY_BYTES] {
        &self.0
    }

    /// `XXXX-XXXX-XXXX-XXXX-XXXX-XXXX-XXXX-XXXX`
    pub fn display(&self) -> String {
        let mut chars = String::with_capacity(39);
        let mut buffer: u64 = 0;
        let mut bits = 0;
        let mut count = 0;
        for &byte in &self.0 {
            buffer = (buffer << 8) | byte as u64;
            bits += 8;
            while bits >= 5 {
                bits -= 5;
                let idx = ((buffer >> bits) & 0x1f) as usize;
                if count > 0 && count % 4 == 0 {
                    chars.push('-');
                }
                chars.push(ALPHABET[idx] as char);
                count += 1;
            }
        }
        chars
    }

    /// Parse user input. Case, spaces and dashes are ignored; I/L read as 1, O as 0.
    pub fn parse(input: &str) -> Result<Self, RecoveryParseError> {
        let mut values = Vec::with_capacity(32);
        for c in input.chars() {
            if c == '-' || c.is_whitespace() {
                continue;
            }
            let c = c.to_ascii_uppercase();
            let c = match c {
                'I' | 'L' => '1',
                'O' => '0',
                other => other,
            };
            let v = ALPHABET
                .iter()
                .position(|&a| a as char == c)
                .ok_or(RecoveryParseError::Char(c))?;
            values.push(v as u64);
        }
        if values.len() != 32 {
            return Err(RecoveryParseError::Length);
        }
        let mut out = [0u8; RECOVERY_BYTES];
        let mut buffer: u64 = 0;
        let mut bits = 0;
        let mut i = 0;
        for v in values {
            buffer = (buffer << 5) | v;
            bits += 5;
            if bits >= 8 {
                bits -= 8;
                out[i] = ((buffer >> bits) & 0xff) as u8;
                i += 1;
            }
        }
        Ok(Self(out))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_parse_roundtrip() {
        for _ in 0..50 {
            let k = RecoveryKey::generate().unwrap();
            let s = k.display();
            assert_eq!(s.len(), 39);
            assert_eq!(s.matches('-').count(), 7);
            assert_eq!(RecoveryKey::parse(&s).unwrap(), k);
            assert_eq!(RecoveryKey::parse(&s.to_lowercase().replace('-', " ")).unwrap(), k);
        }
    }

    #[test]
    fn rejects_bad_input() {
        assert_eq!(RecoveryKey::parse("ABCD"), Err(RecoveryParseError::Length));
        assert_eq!(
            RecoveryKey::parse("UUUU-UUUU-UUUU-UUUU-UUUU-UUUU-UUUU-UUUU"),
            Err(RecoveryParseError::Char('U'))
        );
    }
}
