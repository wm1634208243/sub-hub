use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce, Tag,
};
use scrypt::{scrypt, Params};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EncryptedBundle {
    #[serde(default)]
    pub _encrypted: bool,
    #[serde(default)]
    pub algorithm: String,
    pub iv: String,
    #[serde(rename = "authTag")]
    pub auth_tag: String,
    pub payload: String,
}

pub fn derive_user_key(user_secret: &str, username: &str) -> Result<[u8; 32], String> {
    let uname = username.trim().to_lowercase();
    let salt = format!("subhub_zk_tenant_{}_v1", uname);
    let mut key = [0u8; 32];
    // Node.js scrypt defaults: N=16384 (log_n=14), r=8, p=1
    let params = Params::new(14, 8, 1, 32).map_err(|e| format!("Invalid scrypt params: {}", e))?;
    scrypt(user_secret.as_bytes(), salt.as_bytes(), &params, &mut key)
        .map_err(|e| format!("scrypt key derivation error: {:?}", e))?;
    Ok(key)
}

pub fn decrypt_user_config_bundle(bundle_json: &serde_json::Value, user_secret: &str, username: &str) -> Result<serde_json::Value, String> {
    // If not encrypted, return directly
    if bundle_json.get("_encrypted").and_then(|v| v.as_bool()) != Some(true) {
        return Ok(bundle_json.clone());
    }

    let iv_hex = bundle_json.get("iv").and_then(|v| v.as_str()).ok_or("Missing iv")?;
    let tag_hex = bundle_json.get("authTag").and_then(|v| v.as_str()).ok_or("Missing authTag")?;
    let payload_hex = bundle_json.get("payload").and_then(|v| v.as_str()).ok_or("Missing payload")?;

    let iv_bytes = hex::decode(iv_hex).map_err(|e| format!("Invalid iv hex: {}", e))?;
    let tag_bytes = hex::decode(tag_hex).map_err(|e| format!("Invalid tag hex: {}", e))?;
    let mut payload_bytes = hex::decode(payload_hex).map_err(|e| format!("Invalid payload hex: {}", e))?;

    if iv_bytes.len() != 12 || tag_bytes.len() != 16 {
        return Err("Invalid IV or AuthTag length".into());
    }

    let key = derive_user_key(user_secret, username)?;
    let cipher = Aes256Gcm::new_from_slice(&key).map_err(|e| format!("AES init error: {:?}", e))?;
    let nonce = Nonce::from_slice(&iv_bytes);

    // In Rust aes-gcm, standard decrypt expects ciphertext concatenated with the 16-byte tag!
    payload_bytes.extend_from_slice(&tag_bytes);

    let decrypted = cipher.decrypt(nonce, payload_bytes.as_ref())
        .map_err(|e| format!("AES-256-GCM decryption failed: {:?}", e))?;

    let decrypted_str = String::from_utf8(decrypted).map_err(|e| format!("Invalid UTF-8: {}", e))?;
    serde_json::from_str(&decrypted_str).map_err(|e| format!("JSON parse error: {}", e))
}

pub fn encrypt_user_config_bundle(
    config_json: &serde_json::Value,
    user_secret: &str,
    username: &str,
) -> Result<serde_json::Value, String> {
    use rand::RngCore;

    let key = derive_user_key(user_secret, username)?;
    let cipher = Aes256Gcm::new_from_slice(&key).map_err(|e| format!("AES init error: {:?}", e))?;

    let mut iv = [0u8; 12];
    rand::thread_rng().fill_bytes(&mut iv);
    let nonce = Nonce::from_slice(&iv);

    let plaintext = serde_json::to_string(config_json).map_err(|e| format!("JSON serialize error: {}", e))?;
    let ciphertext_with_tag = cipher
        .encrypt(nonce, plaintext.as_bytes())
        .map_err(|e| format!("AES encrypt error: {:?}", e))?;

    if ciphertext_with_tag.len() < 16 {
        return Err("Ciphertext too short".into());
    }

    let split_idx = ciphertext_with_tag.len() - 16;
    let payload = &ciphertext_with_tag[..split_idx];
    let tag = &ciphertext_with_tag[split_idx..];

    Ok(serde_json::json!({
        "_encrypted": true,
        "algorithm": "aes-256-gcm",
        "iv": hex::encode(iv),
        "authTag": hex::encode(tag),
        "payload": hex::encode(payload)
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_crypto_round_trip() {
        let original_json = serde_json::json!({
            "subscriptions": [
                {
                    "id": "sub-1",
                    "name": "My Node Sub",
                    "url": "https://example.com/sub",
                    "enabled": true
                }
            ],
            "subscriptionToken": "secret-token-123456"
        });

        let secret = "$2b$10$vNqjA8.2d38e..dummyhash";
        let username = "admin";

        let encrypted = encrypt_user_config_bundle(&original_json, secret, username).expect("Encryption should succeed");
        assert_eq!(encrypted["_encrypted"], true);
        assert_eq!(encrypted["algorithm"], "aes-256-gcm");
        assert!(encrypted["iv"].as_str().is_some());
        assert!(encrypted["authTag"].as_str().is_some());
        assert!(encrypted["payload"].as_str().is_some());

        let decrypted = decrypt_user_config_bundle(&encrypted, secret, username).expect("Decryption should succeed");
        assert_eq!(decrypted, original_json);

        // Wrong secret should fail decryption
        let wrong_secret = "wrong-password-hash";
        assert!(decrypt_user_config_bundle(&encrypted, wrong_secret, username).is_err());
    }
}


