use bs58;
use rand::Rng;
use sha2::{Digest, Sha256};

use crate::Result;

#[derive(Clone, Debug)]
pub struct Channel {
    channel_secret: [u8; 32],
    channel_name: String,
    address: String,
    id: Option<u32>,
}

impl Channel {
    pub fn new(
        channel_name: String,
        channel_secret: [u8; 32],
        address: String,
        id: u32,
    ) -> Channel {
        Channel { channel_secret, channel_name, address, id: Some(id) }
    }

    pub fn gen_new(channel_name: String) -> Channel {
        let channel_secret = rand::thread_rng().gen::<[u8; 32]>();
        let address = Self::gen_address(channel_secret);
        Channel { channel_secret, channel_name, address, id: None }
    }

    pub fn gen_new_with_addr(channel_name: String, channel_address: String) -> Result<Channel> {
        let decoded = bs58::decode(channel_address.clone()).into_vec()?;
        let mut channel_secret: [u8; 32] = [0; 32];
        channel_secret.copy_from_slice(&decoded[4..36]);
        Ok(Channel { channel_secret, channel_name, address: channel_address, id: None })
    }

    pub fn gen_address(channel_secret: [u8; 32]) -> String {
        let mut hasher = Sha256::new();

        let version: u32 = 1;
        let mut payload = version.to_be_bytes().to_vec();
        let mut channel_secret = channel_secret.to_vec();
        payload.append(&mut channel_secret);
        hasher.update(payload.clone());
        let result = hasher.finalize();

        let mut checksum: [u8; 4] = [0; 4];
        checksum.copy_from_slice(&result[..4]);

        payload.append(&mut checksum.to_vec());

        let encoded = bs58::encode(payload).into_string();

        encoded
    }

    pub fn get_channel_secret(&self) -> [u8; 32] {
        self.channel_secret
    }

    pub fn get_channel_name(&self) -> &String {
        &self.channel_name
    }

    pub fn get_channel_id(&self) -> &Option<u32> {
        &self.id
    }

    pub fn get_channel_address(&self) -> &String {
        &self.address
    }
}

#[cfg(test)]
mod tests {
    use super::Channel;
    use crate::Result;

    #[test]
    fn create_channel_form_address() -> Result<()> {
        let channel = Channel::gen_new(String::from("test"));
        let channel_address = channel.get_channel_address();
        let channel2 = Channel::gen_new_with_addr(String::from("test"), channel_address.clone())?;
        assert_eq!(channel.get_channel_secret(), channel2.get_channel_secret());
        assert_eq!(channel.get_channel_address(), channel2.get_channel_address());
        Ok(())
    }
}
