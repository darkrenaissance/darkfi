use crypto_box::{
    aead::{Aead, AeadCore},
    SalsaBox,
};
use fxhash::FxHashMap;
use rand::rngs::OsRng;

use crate::{
    privmsg::Privmsg,
    settings::{ChannelInfo, ContactInfo},
    MAXIMUM_LENGTH_OF_NICKNAME,
};

/// Try decrypting a message given a NaCl box and a base58 string.
/// The format we're using is nonce+ciphertext, where nonce is 24 bytes.
fn try_decrypt_message(salt_box: &SalsaBox, ciphertext: &str) -> Option<String> {
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
pub fn encrypt_message(salt_box: &SalsaBox, plaintext: &str) -> String {
    let nonce = SalsaBox::generate_nonce(&mut OsRng);
    let mut ciphertext = salt_box.encrypt(&nonce, plaintext.as_bytes()).unwrap();

    let mut concat = vec![];
    concat.append(&mut nonce.as_slice().to_vec());
    concat.append(&mut ciphertext);

    bs58::encode(concat).into_string()
}

/// Decrypt PrivMsg target
pub fn decrypt_target(
    contact: &mut String,
    privmsg: &mut Privmsg,
    configured_chans: FxHashMap<String, ChannelInfo>,
    configured_contacts: FxHashMap<String, ContactInfo>,
) {
    for chan_name in configured_chans.keys() {
        let chan_info = configured_chans.get(chan_name).unwrap();
        if !chan_info.joined {
            continue
        }

        let salt_box = chan_info.salt_box.clone();

        if let Some(salt_box) = salt_box {
            let decrypted_target = try_decrypt_message(&salt_box, &privmsg.target);
            if decrypted_target.is_none() {
                continue
            }

            let target = decrypted_target.unwrap();
            if *chan_name == target {
                privmsg.target = target;
                return
            }
        }
    }

    for cnt_name in configured_contacts.keys() {
        let cnt_info = configured_contacts.get(cnt_name).unwrap();

        let salt_box = cnt_info.salt_box.clone();
        if let Some(salt_box) = salt_box {
            let decrypted_target = try_decrypt_message(&salt_box, &privmsg.target);
            if decrypted_target.is_none() {
                continue
            }

            let target = decrypted_target.unwrap();
            privmsg.target = target;
            *contact = cnt_name.into();
            return
        }
    }
}

/// Decrypt PrivMsg nickname and message
pub fn decrypt_privmsg(salt_box: &SalsaBox, privmsg: &mut Privmsg) {
    let decrypted_nick = try_decrypt_message(&salt_box.clone(), &privmsg.nickname);
    let decrypted_msg = try_decrypt_message(&salt_box.clone(), &privmsg.message);

    if decrypted_nick.is_none() | decrypted_msg.is_none() {
        return
    }

    privmsg.nickname = decrypted_nick.unwrap();
    if privmsg.nickname.len() > MAXIMUM_LENGTH_OF_NICKNAME {
        privmsg.nickname = privmsg.nickname[..MAXIMUM_LENGTH_OF_NICKNAME].to_string();
    }
    privmsg.message = decrypted_msg.unwrap();
}

/// Encrypt PrivMsg
pub fn encrypt_privmsg(salt_box: &SalsaBox, privmsg: &mut Privmsg) {
    privmsg.nickname = encrypt_message(salt_box, &privmsg.nickname);
    privmsg.target = encrypt_message(salt_box, &privmsg.target);
    privmsg.message = encrypt_message(salt_box, &privmsg.message);
}
