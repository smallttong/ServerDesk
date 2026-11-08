use aes_gcm::aead::{Aead, KeyInit, OsRng, Payload};
use aes_gcm::{AeadCore, Aes256Gcm, Nonce};
use argon2::{Algorithm, Argon2, Params, Version};
use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine as _;
use hkdf::Hkdf;
use serde::{Deserialize, Serialize};
use sha2::Sha256;

pub const KEY_LEN: usize = 32;
pub const NONCE_LEN: usize = 12;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KdfParams {
    pub m_cost: u32,
    pub t_cost: u32,
    pub p_cost: u32,
}

impl Default for KdfParams {
    fn default() -> Self {
        Self {
            m_cost: 64 * 1024, // 64 MiB
            t_cost: 3,
            p_cost: 1,
        }
    }
}

pub fn random_bytes<const N: usize>() -> [u8; N] {
    use rand::RngCore;
    let mut bytes = [0u8; N];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    bytes
}

pub fn b64e(bytes: &[u8]) -> String {
    B64.encode(bytes)
}

pub fn b64d(text: &str) -> Result<Vec<u8>, String> {
    B64.decode(text.trim())
        .map_err(|e| format!("base64 解码失败: {e}"))
}

/// Argon2id: the only place the password is used.
pub fn derive_kek(
    password: &str,
    salt: &[u8],
    params: &KdfParams,
) -> Result<[u8; KEY_LEN], String> {
    let params = Params::new(params.m_cost, params.t_cost, params.p_cost, Some(KEY_LEN))
        .map_err(|e| format!("Argon2 参数无效: {e}"))?;
    let argon = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let mut out = [0u8; KEY_LEN];
    argon
        .hash_password_into(password.as_bytes(), salt, &mut out)
        .map_err(|e| format!("密钥派生失败: {e}"))?;
    Ok(out)
}

pub fn aes_encrypt(
    key: &[u8; KEY_LEN],
    plaintext: &[u8],
    aad: &[u8],
) -> Result<(Vec<u8>, Vec<u8>), String> {
    let cipher = Aes256Gcm::new_from_slice(key).map_err(|e| format!("密钥无效: {e}"))?;
    let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
    let ciphertext = cipher
        .encrypt(
            &nonce,
            Payload {
                msg: plaintext,
                aad,
            },
        )
        .map_err(|_| "加密失败".to_string())?;
    Ok((nonce.to_vec(), ciphertext))
}

pub fn aes_decrypt(
    key: &[u8; KEY_LEN],
    nonce: &[u8],
    ciphertext: &[u8],
    aad: &[u8],
) -> Result<Vec<u8>, String> {
    if nonce.len() != NONCE_LEN {
        return Err("nonce 长度错误".into());
    }
    let cipher = Aes256Gcm::new_from_slice(key).map_err(|e| format!("密钥无效: {e}"))?;
    cipher
        .decrypt(
            Nonce::from_slice(nonce),
            Payload {
                msg: ciphertext,
                aad,
            },
        )
        .map_err(|_| "解密失败（密码错误或数据被篡改）".to_string())
}

/// One key per credential, so a grant can hand out exactly one of them.
pub fn derive_cek(dek: &[u8; KEY_LEN], credential_id: &str) -> Result<[u8; KEY_LEN], String> {
    let hk = Hkdf::<Sha256>::new(None, dek);
    let mut out = [0u8; KEY_LEN];
    hk.expand(
        format!("serverdesk-vault/credential/{credential_id}").as_bytes(),
        &mut out,
    )
    .map_err(|_| "派生凭据密钥失败".to_string())?;
    Ok(out)
}

/// Windows DPAPI. Ties the blob to the current Windows user on this machine:
/// it stops "copy the file elsewhere", not "same user runs malware".
#[cfg(windows)]
pub fn dpapi_protect(data: &[u8]) -> Result<Vec<u8>, String> {
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Cryptography::{CryptProtectData, CRYPT_INTEGER_BLOB};

    let input = CRYPT_INTEGER_BLOB {
        cbData: data.len() as u32,
        pbData: data.as_ptr() as *mut u8,
    };
    let mut output = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: std::ptr::null_mut(),
    };
    let ok = unsafe {
        CryptProtectData(
            &input,
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null(),
            0,
            &mut output,
        )
    };
    if ok == 0 {
        return Err("DPAPI 加密失败".into());
    }
    let bytes =
        unsafe { std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec() };
    unsafe {
        LocalFree(output.pbData as *mut core::ffi::c_void);
    }
    Ok(bytes)
}

#[cfg(windows)]
pub fn dpapi_unprotect(data: &[u8]) -> Result<Vec<u8>, String> {
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Cryptography::{CryptUnprotectData, CRYPT_INTEGER_BLOB};

    let input = CRYPT_INTEGER_BLOB {
        cbData: data.len() as u32,
        pbData: data.as_ptr() as *mut u8,
    };
    let mut output = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: std::ptr::null_mut(),
    };
    let ok = unsafe {
        CryptUnprotectData(
            &input,
            std::ptr::null_mut(),
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null(),
            0,
            &mut output,
        )
    };
    if ok == 0 {
        return Err("DPAPI 解密失败".into());
    }
    let bytes =
        unsafe { std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec() };
    unsafe {
        LocalFree(output.pbData as *mut core::ffi::c_void);
    }
    Ok(bytes)
}

#[cfg(not(windows))]
pub fn dpapi_protect(_data: &[u8]) -> Result<Vec<u8>, String> {
    Err("DPAPI 仅在 Windows 上可用".into())
}

#[cfg(not(windows))]
pub fn dpapi_unprotect(_data: &[u8]) -> Result<Vec<u8>, String> {
    Err("DPAPI 仅在 Windows 上可用".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_and_aad_binding() {
        let key = random_bytes::<KEY_LEN>();
        let (nonce, cipher) = aes_encrypt(&key, b"hunter2", b"rec-1").unwrap();
        assert_eq!(
            aes_decrypt(&key, &nonce, &cipher, b"rec-1").unwrap(),
            b"hunter2"
        );
        // Same ciphertext, different record id must fail authentication.
        assert!(aes_decrypt(&key, &nonce, &cipher, b"rec-2").is_err());
    }

    #[test]
    fn per_credential_keys_differ() {
        let dek = random_bytes::<KEY_LEN>();
        let a = derive_cek(&dek, "cred-a").unwrap();
        let b = derive_cek(&dek, "cred-b").unwrap();
        assert_ne!(a, b);
    }

    #[test]
    fn kdf_is_deterministic() {
        let salt = random_bytes::<16>();
        let params = KdfParams {
            m_cost: 8 * 1024,
            t_cost: 1,
            p_cost: 1,
        };
        let a = derive_kek("pass phrase", &salt, &params).unwrap();
        let b = derive_kek("pass phrase", &salt, &params).unwrap();
        assert_eq!(a, b);
        let c = derive_kek("pass phrasf", &salt, &params).unwrap();
        assert_ne!(a, c);
    }
}
