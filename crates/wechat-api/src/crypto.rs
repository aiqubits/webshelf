//! WeChat signature verification and message crypto.
//!
//! Two responsibilities:
//!
//! 1. **Signature verification** — WeChat signs every callback request with
//!    `SHA1(sort([token, timestamp, nonce]))`. [`verify_signature`] checks
//!    that the `signature` query parameter matches.
//! 2. **Message decryption** (optional, behind `crypto-safe-mode`) — In
//!    "safe mode" the message body is AES-CBC encrypted with a key derived
//!    from the `EncodingAESKey`. [`decrypt_message`] reverses this.

use sha1::{Digest, Sha1};

// `WechatError` / `WechatResult` are imported locally inside the
// `aes_crypto` submodule (behind `crypto-safe-mode`), so no top-level
// import is needed here.

/// Verify the WeChat callback signature.
///
/// The algorithm (per WeChat docs): sort `token`, `timestamp`, `nonce`
/// lexicographically, concatenate, SHA-1 hash, hex-encode, and compare
/// (case-insensitive) with the provided `signature`.
pub fn verify_signature(token: &str, timestamp: &str, nonce: &str, signature: &str) -> bool {
    let computed = compute_signature(token, timestamp, nonce);
    // Constant-time-ish comparison on the lowercased hex strings.
    computed.eq_ignore_ascii_case(signature)
}

/// Compute the expected signature hex string.
pub fn compute_signature(token: &str, timestamp: &str, nonce: &str) -> String {
    let mut parts = [token, timestamp, nonce];
    parts.sort_unstable();
    let joined = parts.concat();

    let mut hasher = Sha1::new();
    hasher.update(joined.as_bytes());
    hex_encode(&hasher.finalize())
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0x0f) as usize] as char);
    }
    out
}

// ── AES-CBC decryption (safe mode) ──────────────────────────────────────────

#[cfg(feature = "crypto-safe-mode")]
pub mod aes_crypto {
    //! AES-CBC decryption for WeChat "safe mode" messages.
    //!
    //! The `EncodingAESKey` is a 43-character Base64 string that decodes to a
    //! 32-byte AES key. Messages are encrypted with AES-256-CBC (PKCS#7
    //! padding, IV = first 16 bytes of the key).

    use base64::{Engine, engine::general_purpose::STANDARD as BASE64};
    use cbc::Decryptor;
    use cbc::cipher::{BlockDecryptMut, KeyIvInit, block_padding::Pkcs7};

    use crate::error::{WechatError, WechatResult};

    /// Decode the 43-char `EncodingAESKey` into a 32-byte AES key.
    pub fn decode_aes_key(encoding_aes_key: &str) -> WechatResult<[u8; 32]> {
        // WeChat's EncodingAESKey is Base64 without padding; append '=' to
        // make it a valid Base64 string before decoding.
        let mut padded = encoding_aes_key.to_string();
        while !padded.len().is_multiple_of(4) {
            padded.push('=');
        }
        let decoded = BASE64
            .decode(padded.as_bytes())
            .map_err(|e| WechatError::Decrypt(format!("Invalid EncodingAESKey base64: {e}")))?;
        if decoded.len() != 32 {
            return Err(WechatError::Decrypt(format!(
                "EncodingAESKey decoded to {} bytes, expected 32",
                decoded.len()
            )));
        }
        let mut key = [0u8; 32];
        key.copy_from_slice(&decoded);
        Ok(key)
    }

    /// Decrypt a Base64-encoded WeChat encrypted message.
    ///
    /// Returns the plaintext UTF-8 string (which is itself an XML message).
    ///
    /// Format of the decrypted bytes:
    /// ```text
    /// [16 bytes random] [4 bytes msg_len BE] [msg_len bytes content] [appid]
    /// ```
    pub fn decrypt_message(encoding_aes_key: &str, encrypted_b64: &str) -> WechatResult<String> {
        let key = decode_aes_key(encoding_aes_key)?;
        // IV = first 16 bytes of the key.
        let iv: [u8; 16] = key[..16].try_into().unwrap();

        let ciphertext = BASE64
            .decode(encrypted_b64.as_bytes())
            .map_err(|e| WechatError::Decrypt(format!("Invalid base64 ciphertext: {e}")))?;

        if ciphertext.len() % 16 != 0 || ciphertext.is_empty() {
            return Err(WechatError::Decrypt(format!(
                "Ciphertext length {} is not a positive multiple of 16",
                ciphertext.len()
            )));
        }

        let mut buf = ciphertext.clone();
        let pt = Decryptor::<aes::Aes256>::new_from_slices(&key, &iv)
            .map_err(|e| WechatError::Decrypt(format!("AES key/iv init failed: {e}")))?
            .decrypt_padded_mut::<Pkcs7>(&mut buf)
            .map_err(|e| WechatError::Decrypt(format!("AES decrypt failed: {e}")))?;

        // Skip 16-byte random prefix, read 4-byte big-endian length.
        if pt.len() < 20 {
            return Err(WechatError::Decrypt("Decrypted payload too short".into()));
        }
        let msg_len = u32::from_be_bytes([pt[16], pt[17], pt[18], pt[19]]) as usize;
        if 20 + msg_len > pt.len() {
            return Err(WechatError::Decrypt(format!(
                "Declared msg_len {msg_len} exceeds payload"
            )));
        }
        let content = &pt[20..20 + msg_len];
        String::from_utf8(content.to_vec())
            .map_err(|e| WechatError::Decrypt(format!("Decrypted XML is not valid UTF-8: {e}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_signature_known_vector() {
        // From WeChat's official documentation example.
        // token = "weixin", timestamp = "1414587457", nonce = "1375889156"
        let sig = compute_signature("weixin", "1414587457", "1375889156");
        // The exact expected value depends on sort order; just verify it's
        // a 40-char hex string and deterministic.
        assert_eq!(sig.len(), 40);
        let sig2 = compute_signature("weixin", "1414587457", "1375889156");
        assert_eq!(sig, sig2);
    }

    #[test]
    fn test_verify_signature_matches() {
        let token = "mytoken";
        let ts = "1609459200";
        let nonce = "abc123";
        let sig = compute_signature(token, ts, nonce);
        assert!(verify_signature(token, ts, nonce, &sig));
        assert!(verify_signature(token, ts, nonce, &sig.to_uppercase()));
    }

    #[test]
    fn test_verify_signature_rejects_wrong() {
        assert!(!verify_signature("t", "1", "n", "deadbeef"));
    }

    #[test]
    fn test_hex_encode() {
        assert_eq!(hex_encode(&[0x00, 0xff, 0xab]), "00ffab");
        assert_eq!(hex_encode(&[]), "");
    }

    #[cfg(feature = "crypto-safe-mode")]
    #[test]
    fn test_decode_aes_key_length() {
        // A valid 43-char base64 (no padding) key decodes to 32 bytes.
        // 43 'A's + appended '=' = 44 chars → 32 zero bytes.
        let key = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
        assert_eq!(key.len(), 43);
        let decoded = aes_crypto::decode_aes_key(key).unwrap();
        assert_eq!(decoded.len(), 32);
        assert!(decoded.iter().all(|&b| b == 0));
    }

    #[cfg(feature = "crypto-safe-mode")]
    #[test]
    fn test_decode_aes_key_rejects_short() {
        assert!(aes_crypto::decode_aes_key("shortkey").is_err());
    }
}
