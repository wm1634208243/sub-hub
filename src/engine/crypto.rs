use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use rand::RngCore;
use sha2::{Digest, Sha256};
use std::error::Error;

pub fn derive_user_key(username: &str, salt: &str) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(username.as_bytes());
    hasher.update(b":subhub_secure_salt:");
    hasher.update(salt.as_bytes());
    hasher.finalize().into()
}

pub fn encrypt_data(key: &[u8; 32], plaintext: &[u8]) -> Result<String, Box<dyn Error + Send + Sync>> {
    let cipher = Aes256Gcm::new_from_slice(key)?;
    let mut iv = [0u8; 12];
    rand::thread_rng().fill_bytes(&mut iv);
    let nonce = Nonce::from_slice(&iv);

    let ciphertext = cipher
        .encrypt(nonce, plaintext)
        .map_err(|e| format!("Encryption error: {:?}", e))?;

    let mut combined = Vec::with_capacity(iv.len() + ciphertext.len());
    combined.extend_from_slice(&iv);
    combined.extend_from_slice(&ciphertext);

    Ok(base64::Engine::encode(
        &base64::engine::general_purpose::STANDARD,
        &combined,
    ))
}

pub fn decrypt_data(key: &[u8; 32], b64_ciphertext: &str) -> Result<Vec<u8>, Box<dyn Error + Send + Sync>> {
    let combined = base64::Engine::decode(
        &base64::engine::general_purpose::STANDARD,
        b64_ciphertext.trim(),
    )?;

    if combined.len() < 12 {
        return Err("Invalid ciphertext length".into());
    }

    let (iv, ciphertext) = combined.split_at(12);
    let cipher = Aes256Gcm::new_from_slice(key)?;
    let nonce = Nonce::from_slice(iv);

    let plaintext = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|e| format!("Decryption error: {:?}", e))?;

    Ok(plaintext)
}
