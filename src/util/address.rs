use std::{io, str::FromStr};

use sha2::Digest;

use crate::{
    crypto::keypair::PublicKey,
    serial::{Decodable, Encodable, ReadExt, WriteExt},
    Error, Result,
};

enum AddressType {
    Payment = 0,
}

#[derive(Copy, Clone, PartialEq, Debug)]
pub struct Address(pub [u8; 37]);

impl Address {
    fn is_valid_address(address: Vec<u8>) -> bool {
        if address.starts_with(&[AddressType::Payment as u8]) && address.len() == 37 {
            // hash the version + publickey to check the checksum
            let mut hasher = sha2::Sha256::new();
            hasher.update(&address[..33]);
            let payload_hash = hasher.finalize().to_vec();

            payload_hash[..4] == address[33..]
        } else {
            false
        }
    }
}

impl std::fmt::Display for Address {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        // base58 encoding
        let address: String = bs58::encode(self.0).into_string();
        write!(f, "{}", address)
    }
}

impl FromStr for Address {
    type Err = Error;

    fn from_str(address: &str) -> Result<Self> {
        let bytes = bs58::decode(&address).into_vec();
        if bytes.is_ok() && Self::is_valid_address(bytes.as_ref().unwrap().clone()) {
            let mut bytes_arr = [0u8; 37];
            bytes_arr.copy_from_slice(bytes.unwrap().as_slice());
            Ok(Self(bytes_arr))
        } else {
            Err(Error::InvalidAddress)
        }
    }
}

impl From<PublicKey> for Address {
    fn from(publickey: PublicKey) -> Self {
        let mut publickey = publickey.to_bytes().to_vec();

        // add version
        let mut address = vec![AddressType::Payment as u8];

        // add public key
        address.append(&mut publickey);

        // hash the version + publickey
        let mut hasher = sha2::Sha256::new();
        hasher.update(address.clone());
        let payload_hash = hasher.finalize().to_vec();

        // add the 4 first bytes from the hash as checksum
        address.append(&mut payload_hash[..4].to_vec());

        let mut payment_address = [0u8; 37];
        payment_address.copy_from_slice(address.as_slice());

        Self(payment_address)
    }
}

impl Encodable for Address {
    fn encode<S: io::Write>(&self, mut s: S) -> Result<usize> {
        s.write_slice(&self.0)?;
        Ok(37)
    }
}

impl Decodable for Address {
    fn decode<D: io::Read>(mut d: D) -> Result<Self> {
        let mut bytes = [0u8; 37];
        d.read_slice(&mut bytes)?;
        Ok(Self(bytes))
    }
}

#[cfg(test)]
mod tests {
    use rand::rngs::OsRng;

    use super::*;
    use crate::crypto::keypair::{Keypair, PublicKey};

    #[test]
    fn test_address() -> Result<()> {
        // from/to PublicKey
        let keypair = Keypair::random(&mut OsRng);
        let address = Address::from(keypair.public);
        assert_eq!(keypair.public, PublicKey::try_from(address)?);

        // from/to string
        let address_str = address.to_string();
        let from_str = Address::from_str(&address_str)?;
        assert_eq!(from_str, address);

        Ok(())
    }
}
