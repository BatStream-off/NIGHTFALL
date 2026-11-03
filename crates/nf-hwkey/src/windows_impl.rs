//! Windows backends: TPM 2.0 (preferred) and DPAPI (fallback without TPM).

use crate::{derive_from_seed, Backend, DeviceSecret, HwKeyError};
use nf_crypto::SecretKey;
use std::ffi::c_void;
use std::path::PathBuf;
use windows::core::{HSTRING, PCWSTR};
use windows::Win32::Foundation::{HLOCAL, LocalFree};
use windows::Win32::Security::Cryptography::*;

fn win_err(e: windows::core::Error) -> HwKeyError {
    HwKeyError::Unavailable(format!("{e}"))
}

/// TPM-resident RSA key. The device secret is SHA-256(PKCS#1 v1.5 signature
/// of a context string). PKCS#1 v1.5 signatures are deterministic, so the
/// same key and context always yield the same secret, and the private key
/// never leaves the TPM.
pub struct Tpm {
    key_name: String,
}

struct ProvHandle(NCRYPT_PROV_HANDLE);
impl Drop for ProvHandle {
    fn drop(&mut self) {
        unsafe {
            let _ = NCryptFreeObject(NCRYPT_HANDLE(self.0 .0));
        }
    }
}
struct KeyHandle(NCRYPT_KEY_HANDLE);
impl Drop for KeyHandle {
    fn drop(&mut self) {
        unsafe {
            let _ = NCryptFreeObject(NCRYPT_HANDLE(self.0 .0));
        }
    }
}

fn open_provider() -> Result<ProvHandle, HwKeyError> {
    let mut prov = NCRYPT_PROV_HANDLE::default();
    unsafe { NCryptOpenStorageProvider(&mut prov, MS_PLATFORM_CRYPTO_PROVIDER, 0) }
        .map_err(win_err)?;
    Ok(ProvHandle(prov))
}

impl Tpm {
    pub fn create(install_id: &str) -> Result<Self, HwKeyError> {
        let key_name = format!("NIGHTFALL-{install_id}");
        let prov = open_provider()?;
        let name = HSTRING::from(key_name.as_str());
        let mut key = NCRYPT_KEY_HANDLE::default();
        unsafe {
            NCryptCreatePersistedKey(
                prov.0,
                &mut key,
                BCRYPT_RSA_ALGORITHM,
                PCWSTR(name.as_ptr()),
                CERT_KEY_SPEC(0),
                NCRYPT_FLAGS(0),
            )
            .map_err(win_err)?;
            let key = KeyHandle(key);
            NCryptFinalizeKey(key.0, NCRYPT_FLAGS(0)).map_err(win_err)?;
        }
        let tpm = Self { key_name };
        // Prove the key works before the key store relies on it.
        tpm.secret("probe")?;
        Ok(tpm)
    }

    pub fn open(key_name: &str) -> Result<Self, HwKeyError> {
        let t = Self { key_name: key_name.to_string() };
        t.open_key().map_err(|_| HwKeyError::NotFound)?;
        Ok(t)
    }

    fn open_key(&self) -> Result<(ProvHandle, KeyHandle), HwKeyError> {
        let prov = open_provider()?;
        let name = HSTRING::from(self.key_name.as_str());
        let mut key = NCRYPT_KEY_HANDLE::default();
        unsafe {
            NCryptOpenKey(prov.0, &mut key, PCWSTR(name.as_ptr()), CERT_KEY_SPEC(0), NCRYPT_FLAGS(0))
                .map_err(win_err)?;
        }
        Ok((prov, KeyHandle(key)))
    }
}

impl DeviceSecret for Tpm {
    fn backend(&self) -> Backend {
        Backend::Tpm { key_name: self.key_name.clone() }
    }

    fn secret(&self, context: &str) -> Result<SecretKey, HwKeyError> {
        let (_prov, key) = self.open_key()?;
        let digest = nf_crypto::sha256(format!("nightfall/tpm/v1/{context}").as_bytes());
        let padding = BCRYPT_PKCS1_PADDING_INFO { pszAlgId: BCRYPT_SHA256_ALGORITHM };
        let pad_ptr = Some(&padding as *const _ as *const c_void);
        let mut len = 0u32;
        unsafe {
            NCryptSignHash(key.0, pad_ptr, &digest, None, &mut len, NCRYPT_PAD_PKCS1_FLAG)
                .map_err(win_err)?;
            let mut sig = vec![0u8; len as usize];
            NCryptSignHash(key.0, pad_ptr, &digest, Some(&mut sig), &mut len, NCRYPT_PAD_PKCS1_FLAG)
                .map_err(win_err)?;
            sig.truncate(len as usize);
            let out = nf_crypto::hkdf_derive(&sig, Some(b"nightfall/tpm-secret/v1"), context.as_bytes());
            for b in sig.iter_mut() {
                std::ptr::write_volatile(b, 0);
            }
            Ok(out)
        }
    }

    fn destroy(&self) -> Result<(), HwKeyError> {
        let (_prov, key) = self.open_key()?;
        unsafe {
            // NCryptDeleteKey frees the handle on success.
            let raw = key.0;
            std::mem::forget(key);
            NCryptDeleteKey(raw, 0).map_err(win_err)?;
        }
        Ok(())
    }
}

/// DPAPI fallback: a random seed encrypted with the Windows user's DPAPI key.
pub struct Dpapi {
    file: PathBuf,
}

impl Dpapi {
    pub fn create(file: PathBuf) -> Result<Self, HwKeyError> {
        let seed = nf_crypto::random_bytes(32)?;
        let protected = protect(&seed)?;
        nf_core::paths::write_private(&file, &protected)?;
        Ok(Self { file })
    }

    pub fn open(file: PathBuf) -> Result<Self, HwKeyError> {
        if !file.exists() {
            return Err(HwKeyError::NotFound);
        }
        Ok(Self { file })
    }
}

impl DeviceSecret for Dpapi {
    fn backend(&self) -> Backend {
        Backend::Dpapi { file: self.file.clone() }
    }

    fn secret(&self, context: &str) -> Result<SecretKey, HwKeyError> {
        let protected = std::fs::read(&self.file).map_err(|_| HwKeyError::NotFound)?;
        let mut seed = unprotect(&protected)?;
        let key = derive_from_seed(&seed, context);
        for b in seed.iter_mut() {
            unsafe { std::ptr::write_volatile(b, 0) };
        }
        Ok(key)
    }

    fn destroy(&self) -> Result<(), HwKeyError> {
        if self.file.exists() {
            std::fs::write(&self.file, [0u8; 64])?;
            std::fs::remove_file(&self.file)?;
        }
        Ok(())
    }
}

fn blob(data: &[u8]) -> CRYPT_INTEGER_BLOB {
    CRYPT_INTEGER_BLOB { cbData: data.len() as u32, pbData: data.as_ptr() as *mut u8 }
}

fn take_blob(out: CRYPT_INTEGER_BLOB) -> Vec<u8> {
    unsafe {
        let v = std::slice::from_raw_parts(out.pbData, out.cbData as usize).to_vec();
        let _ = LocalFree(Some(HLOCAL(out.pbData as *mut c_void)));
        v
    }
}

fn protect(data: &[u8]) -> Result<Vec<u8>, HwKeyError> {
    let input = blob(data);
    let mut out = CRYPT_INTEGER_BLOB::default();
    unsafe {
        CryptProtectData(&input, PCWSTR::null(), None, None, None, CRYPTPROTECT_UI_FORBIDDEN, &mut out)
            .map_err(win_err)?;
    }
    Ok(take_blob(out))
}

fn unprotect(data: &[u8]) -> Result<Vec<u8>, HwKeyError> {
    let input = blob(data);
    let mut out = CRYPT_INTEGER_BLOB::default();
    unsafe {
        CryptUnprotectData(&input, None, None, None, None, CRYPTPROTECT_UI_FORBIDDEN, &mut out)
            .map_err(win_err)?;
    }
    Ok(take_blob(out))
}
