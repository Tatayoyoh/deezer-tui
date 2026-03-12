use blowfish::cipher::block_padding::NoPadding;
use blowfish::cipher::generic_array::GenericArray;
use blowfish::cipher::{BlockDecryptMut, InnerIvInit, KeyInit};
use md5::{Digest, Md5};
use regex::Regex;
use tracing::{debug, info};

use crate::api::models::DeezerError;

type BlowfishCbc = cbc::Decryptor<blowfish::Blowfish>;

/// Size of each audio block (2 KB).
const BLOCK_SIZE: usize = 2048;

/// Fixed IV for Blowfish CBC.
const IV: [u8; 8] = [0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07];

/// Expected MD5 of the valid master key, for validation.
const MASTER_KEY_MD5: &str = "7ebf40da848f4a0fb3cc56ddbe6c2d09";

/// URL to fetch the Deezer web player page.
const WEB_PLAYER_URL: &str = "https://www.deezer.com/en/channels/explore/";

/// Fetch the Blowfish master key from Deezer's web player JS at runtime.
///
/// 1. Fetch the web player HTML page
/// 2. Extract the app-web JS bundle URL
/// 3. Fetch the JS bundle
/// 4. Regex-extract two 8-byte halves (URL-encoded hex arrays)
/// 5. Reverse each half and interleave to produce the 16-byte key
/// 6. Validate via MD5
pub async fn fetch_master_key(http: &reqwest::Client) -> Result<[u8; 16], DeezerError> {
    debug!("Fetching master key from web player");

    // Step 1: fetch the web player page
    let html = http
        .get(WEB_PLAYER_URL)
        .send()
        .await
        .map_err(|e| DeezerError::Http(e.to_string()))?
        .text()
        .await
        .map_err(|e| DeezerError::Http(e.to_string()))?;

    // Step 2: find the app-web JS bundle URL
    let js_url_re = Regex::new(r#"https://[^"]+/app-web[^"]*\.js"#).expect("valid regex");
    let js_url = js_url_re
        .find(&html)
        .ok_or_else(|| DeezerError::Decrypt("Could not find app-web JS bundle URL".into()))?
        .as_str();

    debug!(url = js_url, "Found app-web JS bundle");

    // Step 3: fetch the JS bundle
    let js_source = http
        .get(js_url)
        .send()
        .await
        .map_err(|e| DeezerError::Http(e.to_string()))?
        .text()
        .await
        .map_err(|e| DeezerError::Http(e.to_string()))?;

    // Step 4: extract two 8-byte halves
    // First half: starts with 0x61 ('a'), ends with 0x67 ('g')
    let half_a_re = Regex::new(r"0x61%2C(0x[0-9a-f]{2}%2C){6}0x67").expect("valid regex");
    let half_a_match = half_a_re
        .find(&js_source)
        .ok_or_else(|| DeezerError::Decrypt("Could not find first half of master key".into()))?
        .as_str();

    // Second half: starts with 0x31 ('1'), ends with 0x34 ('4')
    let half_b_re = Regex::new(r"0x31%2C(0x[0-9a-f]{2}%2C){6}0x34").expect("valid regex");
    let half_b_match = half_b_re
        .find(&js_source)
        .ok_or_else(|| DeezerError::Decrypt("Could not find second half of master key".into()))?
        .as_str();

    // Step 5: parse, reverse, interleave
    let half_a = parse_half(half_a_match)?;
    let half_b = parse_half(half_b_match)?;

    let mut key = [0u8; 16];
    for i in 0..8 {
        key[i * 2] = half_a[i];
        key[i * 2 + 1] = half_b[i];
    }

    // Step 6: validate via MD5
    let hash = format!("{:x}", Md5::digest(key));
    if hash != MASTER_KEY_MD5 {
        return Err(DeezerError::Decrypt(format!(
            "Master key MD5 mismatch: expected {MASTER_KEY_MD5}, got {hash}"
        )));
    }

    info!("Master key extracted and validated");
    Ok(key)
}

/// Parse a URL-encoded hex array like "0x61%2C0xAA%2C...%2C0x67" into 8 bytes, reversed.
fn parse_half(half: &str) -> Result<Vec<u8>, DeezerError> {
    let parts: Vec<&str> = half.split("%2C").collect();
    let bytes: Vec<u8> = parts
        .into_iter()
        .rev()
        .filter_map(|s| {
            let hex = s.trim_start_matches("0x");
            u8::from_str_radix(hex, 16).ok()
        })
        .collect();

    if bytes.len() != 8 {
        return Err(DeezerError::Decrypt(format!(
            "Expected 8 bytes in key half, got {}",
            bytes.len()
        )));
    }

    Ok(bytes)
}

/// Derive the per-track Blowfish key from the track ID and master secret.
pub fn derive_track_key(track_id: &str, master_key: &[u8; 16]) -> [u8; 16] {
    let mut hasher = Md5::new();
    hasher.update(track_id.as_bytes());
    let id_md5 = format!("{:x}", hasher.finalize());
    let id_md5_bytes = id_md5.as_bytes();

    let mut key = [0u8; 16];
    for i in 0..16 {
        key[i] = id_md5_bytes[i] ^ id_md5_bytes[i + 16] ^ master_key[i];
    }
    key
}

/// Decrypt an entire Deezer audio buffer in-place.
///
/// Deezer uses "BF_CBC_STRIPE": only every 3rd 2048-byte block is encrypted.
/// Blocks 0, 3, 6, 9… are encrypted; blocks 1, 2, 4, 5, 7, 8… are cleartext.
pub fn decrypt_stream(data: &mut [u8], track_key: &[u8; 16]) -> Result<(), DeezerError> {
    let num_blocks = data.len() / BLOCK_SIZE;

    for block_idx in 0..num_blocks {
        if block_idx % 3 != 0 {
            continue;
        }

        let start = block_idx * BLOCK_SIZE;
        let end = start + BLOCK_SIZE;
        let block = &mut data[start..end];

        let bf = blowfish::Blowfish::new_from_slice(track_key)
            .map_err(|e| DeezerError::Decrypt(format!("Invalid key: {e}")))?;
        let iv = GenericArray::from_slice(&IV);
        let cipher = BlowfishCbc::inner_iv_init(bf, iv);
        cipher
            .decrypt_padded_mut::<NoPadding>(block)
            .map_err(|e| DeezerError::Decrypt(format!("Blowfish decryption failed: {e}")))?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_derive_track_key() {
        let master_key = b"g4el58wc0zvf9na1";
        let key = derive_track_key("123456", master_key);
        assert_eq!(key.len(), 16);
        assert_eq!(key, derive_track_key("123456", master_key));
        assert_ne!(key, derive_track_key("654321", master_key));
    }

    #[test]
    fn test_parse_half() {
        // Simulate "0x61%2C0x32%2C0x33%2C0x34%2C0x35%2C0x36%2C0x37%2C0x67"
        let input = "0x61%2C0x32%2C0x33%2C0x34%2C0x35%2C0x36%2C0x37%2C0x67";
        let result = parse_half(input).unwrap();
        // Should be reversed: [0x67, 0x37, 0x36, 0x35, 0x34, 0x33, 0x32, 0x61]
        assert_eq!(result, vec![0x67, 0x37, 0x36, 0x35, 0x34, 0x33, 0x32, 0x61]);
    }
}
