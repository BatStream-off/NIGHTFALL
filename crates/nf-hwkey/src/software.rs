//! Owner-only file secret. Used on Linux (development) and in tests.

use crate::{derive_from_seed, Backend, DeviceSecret, HwKeyError};
use nf_crypto::SecretKey;
use std::path::PathBuf;
use zeroize_free::wipe;

pub struct SoftwareSecret {
    file: PathBuf,
}

impl SoftwareSecret {
    pub fn create(file: PathBuf) -> Result<Self, HwKeyError> {
        let seed = nf_crypto::random_bytes(32)?;
        nf_core::paths::write_private(&file, &seed)?;
        Ok(Self { file })
    }

    pub fn open(file: PathBuf) -> Result<Self, HwKeyError> {
        if !file.exists() {
            return Err(HwKeyError::NotFound);
        }
        Ok(Self { file })
    }
}

impl DeviceSecret for SoftwareSecret {
    fn backend(&self) -> Backend {
        Backend::Software { file: self.file.clone() }
    }

    fn secret(&self, context: &str) -> Result<SecretKey, HwKeyError> {
        let mut seed = std::fs::read(&self.file).map_err(|_| HwKeyError::NotFound)?;
        let key = derive_from_seed(&seed, context);
        wipe(&mut seed);
        Ok(key)
    }

    fn destroy(&self) -> Result<(), HwKeyError> {
        if self.file.exists() {
            std::fs::write(&self.file, [0u8; 32])?;
            std::fs::remove_file(&self.file)?;
        }
        Ok(())
    }
}

mod zeroize_free {
    /// Overwrite a buffer before it is freed.
    pub fn wipe(buf: &mut [u8]) {
        for b in buf.iter_mut() {
            // volatile write so the compiler keeps it
            unsafe { std::ptr::write_volatile(b, 0) };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_per_context_and_destroyable() {
        let dir = tempfile::tempdir().unwrap();
        let s = SoftwareSecret::create(dir.path().join("device.secret")).unwrap();
        let a = s.secret("pin").unwrap();
        let b = SoftwareSecret::open(dir.path().join("device.secret"))
            .unwrap()
            .secret("pin")
            .unwrap();
        assert_eq!(a, b);
        assert_ne!(a, s.secret("throttle").unwrap());
        s.destroy().unwrap();
        assert!(matches!(s.secret("pin"), Err(HwKeyError::NotFound)));
    }

    #[test]
    fn two_devices_differ() {
        let dir = tempfile::tempdir().unwrap();
        let a = SoftwareSecret::create(dir.path().join("a")).unwrap();
        let b = SoftwareSecret::create(dir.path().join("b")).unwrap();
        assert_ne!(a.secret("pin").unwrap(), b.secret("pin").unwrap());
    }
}
