use std::{fs::File, io::Write};

use darkfi::{consensus::state::State, util::expand_path, Result};

fn main() -> Result<()> {
    let genesis = 1648383795;
    for i in 0..4 {
        let path = format!("~/.config/darkfi/validatord_db_{:?}", i);
        let database_path = expand_path(&path).unwrap();
        println!("Export state from sled database: {:?}", database_path);
        let database = sled::open(database_path).unwrap();
        let state = State::load_current_state(genesis, i, &database).unwrap();
        let state_string = format!("{:#?}", state.read().unwrap());
        let path = format!("validatord_state_{:?}", i);
        let mut file = File::create(path)?;
        file.write(state_string.as_bytes())?;
    }

    Ok(())
}
