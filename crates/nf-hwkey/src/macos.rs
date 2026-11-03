//! macOS Keychain backend.
//!
//! A random 256-bit seed is stored as a generic password in the login
//! Keychain. The Keychain item is bound to this application's code signature:
//! another program reading it triggers a system consent prompt.

use crate::{derive_from_seed, Backend, DeviceSecret, HwKeyError};
use nf_crypto::SecretKey;
use security_framework::passwords::{
    delete_generic_password, get_generic_password, set_generic_password,
};

const SERVICE: &str = "NIGHTFALL Device Secret";

pub struct Keychain {
    service: String,
    account: String,
}

impl Keychain {
    pub fn create(install_id: &str) -> Result<Self, HwKeyError> {
        let seed = nf_crypto::random_bytes(32)?;
        set_generic_password(SERVICE, install_id, &seed)
            .map_err(|e| HwKeyError::Unavailable(format!("Keychain: {e}")))?;
        Ok(Self { service: SERVICE.to_string(), account: install_id.to_string() })
    }

    pub fn open(service: &str, account: &str) -> Result<Self, HwKeyError> {
        let k = Self { service: service.to_string(), account: account.to_string() };
        // Probe so a missing item is reported now rather than at unlock.
        get_generic_password(&k.service, &k.account).map_err(|_| HwKeyError::NotFound)?;
        Ok(k)
    }
}

impl DeviceSecret for Keychain {
    fn backend(&self) -> Backend {
        Backend::Keychain { service: self.service.clone(), account: self.account.clone() }
    }

    fn secret(&self, context: &str) -> Result<SecretKey, HwKeyError> {
        let mut seed =
            get_generic_password(&self.service, &self.account).map_err(|_| HwKeyError::NotFound)?;
        let key = derive_from_seed(&seed, context);
        for b in seed.iter_mut() {
            unsafe { std::ptr::write_volatile(b, 0) };
        }
        Ok(key)
    }

    fn destroy(&self) -> Result<(), HwKeyError> {
        let _ = delete_generic_password(&self.service, &self.account);
        Ok(())
    }
}
