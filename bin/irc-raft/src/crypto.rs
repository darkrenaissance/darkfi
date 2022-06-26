use crypto_box::aead::Aead;
use rand::rngs::OsRng;

/// Try decrypting a message given a NaCl box and a base58 string.
/// The format we're using is nonce+ciphertext, where nonce is 24 bytes.
pub fn try_decrypt_message(salt_box: &crypto_box::Box, ciphertext: &str) -> Option<String> {
    let bytes = match bs58::decode(ciphertext).into_vec() {
        Ok(v) => v,
        Err(_) => return None,
    };

    if bytes.len() < 25 {
        return None
    }

    // Try extracting the nonce
    let nonce = match bytes[0..24].try_into() {
        Ok(v) => v,
        Err(_) => return None,
    };

    // Take the remaining ciphertext
    let message = &bytes[24..];

    // Try decrypting the message
    match salt_box.decrypt(nonce, message) {
        Ok(v) => Some(String::from_utf8_lossy(&v).to_string()),
        Err(_) => None,
    }
}

/// Encrypt a message given a NaCl box and a plaintext string.
/// The format we're using is nonce+ciphertext, where nonce is 24 bytes.
pub fn encrypt_message(salt_box: &crypto_box::Box, plaintext: &str) -> String {
    let nonce = crypto_box::generate_nonce(&mut OsRng);
    let mut ciphertext = salt_box.encrypt(&nonce, plaintext.as_bytes()).unwrap();

    let mut concat = vec![];
    concat.append(&mut nonce.as_slice().to_vec());
    concat.append(&mut ciphertext);

    bs58::encode(concat).into_string()
}
