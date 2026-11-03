//! NIGHTFALL LOCK: PIN-only authentication (no biometrics).
//!
//! Key hierarchy
//! ```text
//! master key (random 256 bits)
//!  ├─ wrapped by KEK_pin = HKDF( Argon2id(PIN, salt) || device_secret("pin") )
//!  ├─ wrapped by KEK_recovery = HKDF( recovery key (160 bits), salt )
//!  └─ wraps one random key per data category (database, vault, cookies, sync)
//! ```
//! Destroying a category key makes that category unreadable (cryptographic
//! purge). Destroying the device secret and the key store makes everything
//! unreadable.

pub mod pin;
pub mod recovery;
pub mod throttle;

use base64::Engine as _;
use nf_core::ProfilePaths;
use nf_crypto::{SecretKey, B64};
use nf_hwkey::{Backend, DeviceSecret};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub use recovery::RecoveryKey;
pub use throttle::AttemptState;

const KEYSTORE_VERSION: u32 = 1;
const AAD_PIN: &[u8] = b"nightfall/master-key/pin/v1";
const AAD_RECOVERY: &[u8] = b"nightfall/master-key/recovery/v1";

/// Data categories that get their own key (for selective cryptographic purge).
pub const CATEGORIES: &[&str] = &["database", "vault", "cookies", "sync"];

/// Argon2id cost parameters.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArgonParams {
    pub memory_kib: u32,
    pub iterations: u32,
    pub parallelism: u32,
}

impl ArgonParams {
    /// 64 MiB, 3 passes: roughly 0.3 to 0.6 s on a recent laptop.
    pub const PRODUCTION: Self = Self { memory_kib: 64 * 1024, iterations: 3, parallelism: 1 };
    /// Cheap parameters for unit tests only.
    pub const TESTING: Self = Self { memory_kib: 64, iterations: 1, parallelism: 1 };
}

#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error(transparent)]
    Pin(#[from] pin::PinError),
    #[error("NIGHTFALL est déjà configuré")]
    AlreadyConfigured,
    #[error("NIGHTFALL n'est pas encore configuré")]
    NotConfigured,
    #[error("magasin de clés illisible : {0}")]
    Corrupt(String),
    #[error("secret matériel indisponible : {0}")]
    Device(#[from] nf_hwkey::HwKeyError),
    #[error("erreur cryptographique : {0}")]
    Crypto(#[from] nf_crypto::CryptoError),
    #[error("erreur disque : {0}")]
    Io(#[from] std::io::Error),
    #[error("Argon2id : {0}")]
    Argon(String),
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum UnlockError {
    #[error("code incorrect")]
    WrongPin { failures: u32, next_wait_seconds: i64 },
    #[error("patientez {0} s avant un nouvel essai")]
    Throttled(i64),
    #[error("trop d'échecs : la Recovery Key est nécessaire")]
    RecoveryRequired,
    #[error("Recovery Key incorrecte")]
    WrongRecoveryKey,
    #[error("erreur interne : {0}")]
    Internal(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct KeystoreFile {
    version: u32,
    install_id: String,
    backend: Backend,
    argon: ArgonParams,
    pin_salt: String,
    pin_wrapped: String,
    recovery_salt: String,
    recovery_wrapped: String,
    category_keys: BTreeMap<String, String>,
    created: i64,
}

/// Status shown on the lock screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct LockStatus {
    pub failures: u32,
    pub wait_seconds: i64,
    pub recovery_required: bool,
    pub hardware_backed: bool,
}

pub struct NightfallLock {
    paths: ProfilePaths,
    file: KeystoreFile,
    device: Box<dyn DeviceSecret>,
}

fn argon2id(pin: &str, salt: &[u8], p: ArgonParams) -> Result<[u8; 32], AuthError> {
    use argon2::{Algorithm, Argon2, Params, Version};
    let params = Params::new(p.memory_kib, p.iterations, p.parallelism, Some(32))
        .map_err(|e| AuthError::Argon(e.to_string()))?;
    let mut out = [0u8; 32];
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
        .hash_password_into(pin.as_bytes(), salt, &mut out)
        .map_err(|e| AuthError::Argon(e.to_string()))?;
    Ok(out)
}

fn b64d(s: &str) -> Result<Vec<u8>, AuthError> {
    B64.decode(s).map_err(|e| AuthError::Corrupt(e.to_string()))
}

impl NightfallLock {
    /// True when a key store exists (NIGHTFALL already configured).
    pub fn is_configured(paths: &ProfilePaths) -> bool {
        paths.keystore().exists()
    }

    /// First launch: create the device secret, the master key, the recovery
    /// key and all category keys. Returns the master key (unlocked session)
    /// and the recovery key to show once.
    pub fn setup(
        paths: &ProfilePaths,
        pin: &str,
        argon: ArgonParams,
    ) -> Result<(Self, SecretKey, RecoveryKey), AuthError> {
        pin::validate(pin)?;
        if Self::is_configured(paths) {
            return Err(AuthError::AlreadyConfigured);
        }
        paths.ensure()?;
        let install_id = nf_crypto::hex(&nf_crypto::random_bytes(8)?);
        let device = nf_hwkey::create(paths, &install_id)?;
        let master = SecretKey::generate()?;
        let recovery = RecoveryKey::generate()?;

        let file = KeystoreFile {
            version: KEYSTORE_VERSION,
            install_id,
            backend: device.backend(),
            argon,
            pin_salt: String::new(),
            pin_wrapped: String::new(),
            recovery_salt: String::new(),
            recovery_wrapped: String::new(),
            category_keys: BTreeMap::new(),
            created: nf_core::unix_now(),
        };
        let mut lock = Self { paths: paths.clone(), file: file.clone(), device };
        lock.wrap_with_pin(&master, pin)?;
        lock.wrap_with_recovery(&master, &recovery)?;
        for cat in CATEGORIES {
            let k = SecretKey::generate()?;
            let sealed = nf_crypto::seal(&master, k.as_bytes(), cat.as_bytes())?;
            lock.file.category_keys.insert((*cat).to_string(), B64.encode(sealed));
        }
        lock.save()?;
        throttle::save(&paths.lock_state(), &lock.throttle_key()?, &AttemptState::default())?;
        Ok((lock, master, recovery))
    }

    /// Load an existing key store and reopen its device secret.
    pub fn load(paths: &ProfilePaths) -> Result<Self, AuthError> {
        let bytes = std::fs::read(paths.keystore()).map_err(|_| AuthError::NotConfigured)?;
        let file: KeystoreFile =
            serde_json::from_slice(&bytes).map_err(|e| AuthError::Corrupt(e.to_string()))?;
        if file.version != KEYSTORE_VERSION {
            return Err(AuthError::Corrupt(format!("version {}", file.version)));
        }
        let device = nf_hwkey::open(&file.backend)?;
        Ok(Self { paths: paths.clone(), file, device })
    }

    pub fn backend(&self) -> &Backend {
        &self.file.backend
    }

    fn save(&self) -> Result<(), AuthError> {
        let json = serde_json::to_vec_pretty(&self.file).expect("serializable");
        nf_core::paths::write_private(&self.paths.keystore(), &json)?;
        Ok(())
    }

    fn throttle_key(&self) -> Result<SecretKey, AuthError> {
        Ok(self.device.secret("lock-state-mac")?)
    }

    fn pin_kek(&self, pin: &str, salt: &[u8]) -> Result<SecretKey, AuthError> {
        let mut stretched = argon2id(pin, salt, self.file.argon)?;
        let device = self.device.secret("pin")?;
        let mut ikm = Vec::with_capacity(64);
        ikm.extend_from_slice(&stretched);
        ikm.extend_from_slice(device.as_bytes());
        let kek = nf_crypto::hkdf_derive(&ikm, Some(salt), b"nightfall/pin-kek/v1");
        zeroize::Zeroize::zeroize(&mut stretched);
        zeroize::Zeroize::zeroize(&mut ikm);
        Ok(kek)
    }

    fn wrap_with_pin(&mut self, master: &SecretKey, pin: &str) -> Result<(), AuthError> {
        let salt = nf_crypto::random_bytes(16)?;
        let kek = self.pin_kek(pin, &salt)?;
        let wrapped = nf_crypto::seal(&kek, master.as_bytes(), AAD_PIN)?;
        self.file.pin_salt = B64.encode(&salt);
        self.file.pin_wrapped = B64.encode(wrapped);
        Ok(())
    }

    fn wrap_with_recovery(&mut self, master: &SecretKey, rk: &RecoveryKey) -> Result<(), AuthError> {
        let salt = nf_crypto::random_bytes(16)?;
        let kek = nf_crypto::hkdf_derive(rk.as_bytes(), Some(&salt), b"nightfall/recovery-kek/v1");
        let wrapped = nf_crypto::seal(&kek, master.as_bytes(), AAD_RECOVERY)?;
        self.file.recovery_salt = B64.encode(&salt);
        self.file.recovery_wrapped = B64.encode(wrapped);
        Ok(())
    }

    fn attempts(&self, now: i64) -> AttemptState {
        match self.throttle_key() {
            Ok(k) => throttle::load(&self.paths.lock_state(), &k, now),
            Err(_) => AttemptState { failures: throttle::TAMPERED_FAILURES, last_failure: now },
        }
    }

    fn record(&self, state: &AttemptState) {
        if let Ok(k) = self.throttle_key() {
            let _ = throttle::save(&self.paths.lock_state(), &k, state);
        }
    }

    pub fn status(&self, now: i64) -> LockStatus {
        let s = self.attempts(now);
        LockStatus {
            failures: s.failures,
            wait_seconds: s.wait_seconds(now),
            recovery_required: s.recovery_required(),
            hardware_backed: self.file.backend.is_hardware_backed(),
        }
    }

    /// Try to unlock with the PIN, enforcing the progressive delay.
    pub fn unlock_with_pin(&self, pin: &str, now: i64) -> Result<SecretKey, UnlockError> {
        let mut state = self.attempts(now);
        if state.recovery_required() {
            return Err(UnlockError::RecoveryRequired);
        }
        let wait = state.wait_seconds(now);
        if wait > 0 {
            return Err(UnlockError::Throttled(wait));
        }
        let internal = |e: AuthError| UnlockError::Internal(e.to_string());
        let salt = b64d(&self.file.pin_salt).map_err(internal)?;
        let wrapped = b64d(&self.file.pin_wrapped).map_err(internal)?;
        let kek = self.pin_kek(pin, &salt).map_err(internal)?;
        match nf_crypto::open(&kek, &wrapped, AAD_PIN) {
            Ok(mut bytes) => {
                let mk = SecretKey::from_slice(&bytes).map_err(|e| UnlockError::Internal(e.to_string()));
                zeroize::Zeroize::zeroize(&mut bytes);
                self.record(&AttemptState::default());
                mk
            }
            Err(_) => {
                state.failures += 1;
                state.last_failure = now;
                self.record(&state);
                if state.recovery_required() {
                    return Err(UnlockError::RecoveryRequired);
                }
                Err(UnlockError::WrongPin {
                    failures: state.failures,
                    next_wait_seconds: state.wait_seconds(now),
                })
            }
        }
    }

    /// Unlock with the Recovery Key (also resets the failure counter).
    pub fn unlock_with_recovery(&self, input: &str) -> Result<SecretKey, UnlockError> {
        let rk = RecoveryKey::parse(input).map_err(|_| UnlockError::WrongRecoveryKey)?;
        let internal = |e: AuthError| UnlockError::Internal(e.to_string());
        let salt = b64d(&self.file.recovery_salt).map_err(internal)?;
        let wrapped = b64d(&self.file.recovery_wrapped).map_err(internal)?;
        let kek = nf_crypto::hkdf_derive(rk.as_bytes(), Some(&salt), b"nightfall/recovery-kek/v1");
        let mut bytes =
            nf_crypto::open(&kek, &wrapped, AAD_RECOVERY).map_err(|_| UnlockError::WrongRecoveryKey)?;
        let mk = SecretKey::from_slice(&bytes).map_err(|e| UnlockError::Internal(e.to_string()));
        zeroize::Zeroize::zeroize(&mut bytes);
        self.record(&AttemptState::default());
        mk
    }

    /// Replace the PIN (requires the unlocked master key).
    pub fn change_pin(&mut self, master: &SecretKey, new_pin: &str) -> Result<(), AuthError> {
        pin::validate(new_pin)?;
        self.verify_master(master)?;
        self.wrap_with_pin(master, new_pin)?;
        self.save()
    }

    /// Issue a new Recovery Key; the previous one stops working.
    pub fn regenerate_recovery(&mut self, master: &SecretKey) -> Result<RecoveryKey, AuthError> {
        self.verify_master(master)?;
        let rk = RecoveryKey::generate()?;
        self.wrap_with_recovery(master, &rk)?;
        self.save()?;
        Ok(rk)
    }

    fn verify_master(&self, master: &SecretKey) -> Result<(), AuthError> {
        // Every category key is sealed with the master key: opening one proves it.
        let any = self
            .file
            .category_keys
            .iter()
            .next()
            .ok_or_else(|| AuthError::Corrupt("no category keys".into()))?;
        nf_crypto::open(master, &b64d(any.1)?, any.0.as_bytes())?;
        Ok(())
    }

    /// Key for a data category. `None` if that category was purged.
    pub fn category_key(&self, master: &SecretKey, category: &str) -> Result<Option<SecretKey>, AuthError> {
        let Some(sealed) = self.file.category_keys.get(category) else {
            return Ok(None);
        };
        let mut bytes = nf_crypto::open(master, &b64d(sealed)?, category.as_bytes())?;
        let k = SecretKey::from_slice(&bytes)?;
        zeroize::Zeroize::zeroize(&mut bytes);
        Ok(Some(k))
    }

    /// Create a fresh key for a category previously purged.
    pub fn recreate_category(&mut self, master: &SecretKey, category: &str) -> Result<SecretKey, AuthError> {
        let k = SecretKey::generate()?;
        let sealed = nf_crypto::seal(master, k.as_bytes(), category.as_bytes())?;
        self.file.category_keys.insert(category.to_string(), B64.encode(sealed));
        self.save()?;
        Ok(k)
    }

    /// Cryptographic purge of one category: its key is erased from the key store.
    pub fn destroy_category(&mut self, category: &str) -> Result<(), AuthError> {
        self.file.category_keys.remove(category);
        self.save()
    }

    /// Cryptographic purge of everything: device secret, key store, counter.
    pub fn destroy_all(self) -> Result<(), AuthError> {
        let _ = self.device.destroy();
        for path in [self.paths.keystore(), self.paths.lock_state()] {
            if path.exists() {
                let len = std::fs::metadata(&path).map(|m| m.len() as usize).unwrap_or(0);
                let _ = std::fs::write(&path, vec![0u8; len]);
                std::fs::remove_file(&path)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> (tempfile::TempDir, ProfilePaths, NightfallLock, SecretKey, RecoveryKey) {
        let dir = tempfile::tempdir().unwrap();
        let paths = ProfilePaths::new(dir.path().join("profile"));
        let (lock, mk, rk) = NightfallLock::setup(&paths, "739204", ArgonParams::TESTING).unwrap();
        (dir, paths, lock, mk, rk)
    }

    #[test]
    fn setup_then_unlock() {
        let (_d, paths, _lock, mk, _rk) = setup();
        let lock = NightfallLock::load(&paths).unwrap();
        assert_eq!(lock.unlock_with_pin("739204", 1000).unwrap(), mk);
        assert!(matches!(
            NightfallLock::setup(&paths, "739204", ArgonParams::TESTING),
            Err(AuthError::AlreadyConfigured)
        ));
    }

    #[test]
    fn weak_pin_refused_at_setup() {
        let dir = tempfile::tempdir().unwrap();
        let paths = ProfilePaths::new(dir.path().join("p"));
        assert!(matches!(
            NightfallLock::setup(&paths, "123456", ArgonParams::TESTING),
            Err(AuthError::Pin(_))
        ));
        assert!(!NightfallLock::is_configured(&paths));
    }

    #[test]
    fn throttling_and_recovery_threshold() {
        let (_d, _paths, lock, mk, _rk) = setup();
        let mut now = 10_000;
        for i in 1..=4 {
            assert_eq!(
                lock.unlock_with_pin("000001", now),
                Err(UnlockError::WrongPin { failures: i, next_wait_seconds: 0 })
            );
        }
        assert_eq!(
            lock.unlock_with_pin("000001", now),
            Err(UnlockError::WrongPin { failures: 5, next_wait_seconds: 30 })
        );
        // Even the right PIN is refused while throttled.
        assert_eq!(lock.unlock_with_pin("739204", now + 10), Err(UnlockError::Throttled(20)));
        now += 30;
        assert_eq!(lock.unlock_with_pin("739204", now).unwrap(), mk);
        assert_eq!(lock.status(now).failures, 0);

        // Reach the recovery threshold.
        for _ in 0..throttle::RECOVERY_THRESHOLD {
            now += 3600;
            let _ = lock.unlock_with_pin("000001", now);
        }
        now += 3600;
        assert_eq!(lock.unlock_with_pin("739204", now), Err(UnlockError::RecoveryRequired));
    }

    #[test]
    fn recovery_key_unlocks_and_resets() {
        let (_d, _paths, mut lock, mk, rk) = setup();
        assert_eq!(lock.unlock_with_recovery(&rk.display()).unwrap(), mk);
        assert_eq!(
            lock.unlock_with_recovery("0000-0000-0000-0000-0000-0000-0000-0000"),
            Err(UnlockError::WrongRecoveryKey)
        );
        lock.change_pin(&mk, "58204716").unwrap();
        assert!(lock.unlock_with_pin("739204", 1).is_err());
        assert_eq!(lock.unlock_with_pin("58204716", 1).unwrap(), mk);
        let rk2 = lock.regenerate_recovery(&mk).unwrap();
        assert_eq!(lock.unlock_with_recovery(&rk.display()), Err(UnlockError::WrongRecoveryKey));
        assert_eq!(lock.unlock_with_recovery(&rk2.display()).unwrap(), mk);
    }

    #[test]
    fn copied_profile_is_useless_without_device_secret() {
        let (_d, paths, lock, _mk, _rk) = setup();
        // Simulate an attacker copying the profile but not the device secret.
        lock.device.destroy().unwrap();
        let reopened = NightfallLock::load(&paths);
        assert!(matches!(reopened, Err(AuthError::Device(_))));
    }

    #[test]
    fn category_keys_and_crypto_purge() {
        let (_d, paths, mut lock, mk, _rk) = setup();
        let k1 = lock.category_key(&mk, "cookies").unwrap().unwrap();
        assert_eq!(lock.category_key(&mk, "cookies").unwrap().unwrap(), k1);
        assert_ne!(lock.category_key(&mk, "vault").unwrap().unwrap(), k1);
        lock.destroy_category("cookies").unwrap();
        let reloaded = NightfallLock::load(&paths).unwrap();
        assert!(reloaded.category_key(&mk, "cookies").unwrap().is_none());
        let wrong = SecretKey::generate().unwrap();
        assert!(reloaded.category_key(&wrong, "vault").is_err());
        reloaded.destroy_all().unwrap();
        assert!(!NightfallLock::is_configured(&paths));
    }
}
