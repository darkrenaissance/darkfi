use crypto_box::{aead::Aead, Box, SecretKey};
use log::{debug, error};
use rand::rngs::OsRng;

use darkfi::{
    util::serial::{deserialize, serialize, SerialDecodable, SerialEncodable},
    Error, Result,
};

use crate::task_info::TaskInfo;

#[derive(Debug, Clone, SerialEncodable, SerialDecodable)]
pub struct EncryptedTask {
    nonce: Vec<u8>,
    payload: Vec<u8>,
}

/// Encrypt a task given the task and a secret key.
pub fn encrypt_task(task: &TaskInfo, secret_key: &SecretKey) -> Result<EncryptedTask> {
    debug!("start encrypting task");
    // Get public key from secret key and create a new Salsa box
    let public_key = secret_key.public_key();
    let msg_box = Box::new(&public_key, secret_key);

    // Generate a nonce and use it to encrypt serialized task (payload)
    let nonce = crypto_box::generate_nonce(&mut OsRng);
    let payload = &serialize(task)[..];
    let payload = match msg_box.encrypt(&nonce, payload) {
        Ok(p) => p,
        Err(e) => {
            error!("Unable to encrypt task: {}", e);
            return Err(Error::OperationFailed)
        }
    };

    let nonce = nonce.to_vec();
    Ok(EncryptedTask { nonce, payload })
}

/// Decrypt a task given the encrypted task and the secret key (same used to encrypt).
pub fn decrypt_task(encrypted_task: &EncryptedTask, secret_key: &SecretKey) -> Option<TaskInfo> {
    debug!("start decrypting task");
    // Get public key from secret key and create a new Salsa box
    let public_key = secret_key.public_key();
    let msg_box = Box::new(&public_key, secret_key);

    // Extract the nonce nad use it to decrypt the payload
    let nonce = encrypted_task.nonce.as_slice();
    let decrypted_task = match msg_box.decrypt(nonce.into(), &encrypted_task.payload[..]) {
        Ok(m) => m,
        Err(_) => return None,
    };

    // Deserialize to get the task
    deserialize(&decrypted_task).ok()
}
