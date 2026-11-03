//! Progressive delay after failed PIN attempts.
//!
//! The counter is stored in `lock.state` with an HMAC whose key comes from the
//! device secret. A missing, edited or forged counter is treated as a high
//! failure count (1 hour per attempt) rather than reset to zero.

use nf_crypto::{SecretKey, B64};
use base64::Engine as _;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// After this many consecutive failures only the Recovery Key unlocks.
pub const RECOVERY_THRESHOLD: u32 = 20;
/// Failure count assumed when the state file is missing or tampered.
pub const TAMPERED_FAILURES: u32 = 9;

/// Delay (seconds) required before the next attempt after `failures` failures.
pub fn delay_for(failures: u32) -> i64 {
    match failures {
        0..=4 => 0,
        5 => 30,
        6 => 60,
        7 => 5 * 60,
        8 => 15 * 60,
        _ => 60 * 60,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AttemptState {
    pub failures: u32,
    pub last_failure: i64,
}

impl AttemptState {
    /// Seconds to wait before another attempt is allowed (0 = allowed now).
    pub fn wait_seconds(&self, now: i64) -> i64 {
        let ready_at = self.last_failure + delay_for(self.failures);
        (ready_at - now).max(0)
    }

    pub fn recovery_required(&self) -> bool {
        self.failures >= RECOVERY_THRESHOLD
    }
}

#[derive(Serialize, Deserialize)]
struct StateFile {
    failures: u32,
    last_failure: i64,
    mac: String,
}

fn payload(s: &AttemptState) -> Vec<u8> {
    format!("nightfall-lock-state/v1|{}|{}", s.failures, s.last_failure).into_bytes()
}

/// Load the counter. `exists_keystore` tells whether a missing file is
/// suspicious (key store present) or normal (fresh install).
pub fn load(path: &Path, mac_key: &SecretKey, now: i64) -> AttemptState {
    let tampered = AttemptState { failures: TAMPERED_FAILURES, last_failure: now };
    let Ok(bytes) = std::fs::read(path) else {
        return tampered;
    };
    let Ok(file) = serde_json::from_slice::<StateFile>(&bytes) else {
        return tampered;
    };
    let state = AttemptState { failures: file.failures, last_failure: file.last_failure };
    let Ok(tag) = B64.decode(file.mac) else {
        return tampered;
    };
    if !nf_crypto::hmac_verify(mac_key, &payload(&state), &tag) {
        return tampered;
    }
    // A last_failure in the future means the clock was moved back: keep it.
    state
}

pub fn save(path: &Path, mac_key: &SecretKey, state: &AttemptState) -> std::io::Result<()> {
    let tag = nf_crypto::hmac_tag(mac_key, &payload(state));
    let file = StateFile {
        failures: state.failures,
        last_failure: state.last_failure,
        mac: B64.encode(tag),
    };
    let json = serde_json::to_vec(&file).expect("serializable");
    nf_core::paths::write_private(path, &json)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schedule() {
        assert_eq!(delay_for(0), 0);
        assert_eq!(delay_for(4), 0);
        assert_eq!(delay_for(5), 30);
        assert_eq!(delay_for(6), 60);
        assert_eq!(delay_for(7), 300);
        assert_eq!(delay_for(8), 900);
        assert_eq!(delay_for(9), 3600);
        assert_eq!(delay_for(50), 3600);
    }

    #[test]
    fn wait_seconds() {
        let s = AttemptState { failures: 5, last_failure: 1000 };
        assert_eq!(s.wait_seconds(1000), 30);
        assert_eq!(s.wait_seconds(1029), 1);
        assert_eq!(s.wait_seconds(1030), 0);
    }

    #[test]
    fn tamper_detection() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("lock.state");
        let key = SecretKey::from_bytes([3u8; 32]);
        let st = AttemptState { failures: 2, last_failure: 50 };
        save(&path, &key, &st).unwrap();
        assert_eq!(load(&path, &key, 100), st);

        // Edit the counter without the key: detected.
        let mut json: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        json["failures"] = 0.into();
        std::fs::write(&path, serde_json::to_vec(&json).unwrap()).unwrap();
        assert_eq!(load(&path, &key, 100).failures, TAMPERED_FAILURES);

        // Deleted file: also treated as tampered.
        std::fs::remove_file(&path).unwrap();
        assert_eq!(load(&path, &key, 100).failures, TAMPERED_FAILURES);
    }
}
