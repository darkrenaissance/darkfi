
use aes_gcm::aead::{generic_array::GenericArray, Aead, NewAead};
use aes_gcm::Aes256Gcm;

pub type AesKey = [u8; 32];
pub type Plaintext = Vec<u8>;
pub type Ciphertext = Vec<u8>;

pub fn aes_encrypt(
    shared_secret: &AesKey,
    nonce: &[u8; 12],
    plaintext: &[u8],
) -> Option<Ciphertext> {
    // Rust is gay, I need to convert to 'GenericArray' whatever the fuck that is...
    let key = GenericArray::from_slice(&shared_secret[..]);
    let cipher = Aes256Gcm::new(key);

    let nonce = GenericArray::from_slice(nonce);
    let ciphertext = cipher.encrypt(nonce, plaintext);
    ciphertext.ok()
}

pub fn aes_decrypt(
    shared_secret: &AesKey,
    nonce: &[u8; 12],
    ciphertext: &Ciphertext,
) -> Option<Plaintext> {
    // Rust is gay, I need to convert to 'GenericArray' whatever the fuck that is...
    let key = GenericArray::from_slice(&shared_secret[..]);
    let cipher = Aes256Gcm::new(key);

    let nonce = GenericArray::from_slice(nonce);

    let plaintext = cipher.decrypt(nonce, ciphertext.as_ref());
    plaintext.ok()
}

#[test]
fn test_aes() {
    let sh_secret = "e02e56a41320d8ebefa946753e9f69587c16d43876cf5bbac86c0ea0e9253d14".as_bytes();

    let mut channel_secret = [0u8; 32];
    channel_secret.copy_from_slice(&sh_secret[0..32]);

    let nonce = [3; 12];

    let ciphertext = aes_encrypt(&channel_secret, &nonce, b"plaintext message").unwrap();

    let plaintext = aes_decrypt(&channel_secret, &nonce, &ciphertext).unwrap();
    // OK it works!
    assert_eq!(&plaintext, b"plaintext message");
}
