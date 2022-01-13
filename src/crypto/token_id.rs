/// Hash the external token ID and NetworkName param.
/// If it fails, change the last 4 bytes and hash it again.
/// Keep repeating until it works.
pub fn generate_id(tkn_str: &str, network: &NetworkName) -> Result<DrkTokenId> {
    let mut id_string = network.to_string();
    id_string.push_str(tkn_str);

    let mut data: Vec<u8> = serialize(&id_string);

    let token_id = match deserialize::<DrkTokenId>(&data) {
        Ok(v) => v,
        Err(_) => {
            let mut counter = 0;
            loop {
                data.truncate(28);
                let serialized_counter = serialize(&counter);
                data.extend(serialized_counter.iter());
                let mut hasher = Sha256::new();
                hasher.update(&data);
                let hash = hasher.finalize();
                let token_id = deserialize::<DrkTokenId>(&hash);
                if token_id.is_err() {
                    counter += 1;
                    continue
                }

                return Ok(token_id.unwrap())
            }
        }
    };

    Ok(token_id)
}

/// YOLO
pub fn generate_id2(tkn_str: &str, network: &NetworkName) -> Result<DrkTokenId> {
    let mut num = 0_u64;

    match network {
        NetworkName::Solana => {
            for i in ['s', 'o', 'l'] {
                num += i as u64;
            }
        }
        NetworkName::Bitcoin => {
            for i in ['b', 't', 'c'] {
                num += i as u64;
            }
        }
        NetworkName::Ethereum => {
            for i in ['e', 't', 'h'] {
                num += i as u64;
            }
        }
        NetworkName::Empty => unimplemented!(),
    }

    for i in tkn_str.chars() {
        num += i as u64;
    }

    Ok(DrkTokenId::from(num))
}
