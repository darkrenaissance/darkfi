#![allow(unused)]

pub mod state;
pub mod transfer;

/*
 money-contract/
 state.apply()
      transfer/
          Builder
          Partial *
          FuncCall

/////////////////////////////////////////////////

let token_id = pallas::Base::random(&mut OsRng);

let builder = TransactionBuilder {
    clear_inputs: vec![TransactionBuilderClearInputInfo {
        value: 110,
        token_id,
        signature_secret: cashier_signature_secret,
    }],
    inputs: vec![],
    outputs: vec![TransactionBuilderOutputInfo {
        value: 110,
        token_id,
        public: keypair.public,
    }],
};

let start = Instant::now();
let mint_pk = ProvingKey::build(11, &MintContract::default());
debug!("Mint PK: [{:?}]", start.elapsed());
let start = Instant::now();
let burn_pk = ProvingKey::build(11, &BurnContract::default());
debug!("Burn PK: [{:?}]", start.elapsed());
let tx = builder.build(&mint_pk, &burn_pk)?;

tx.verify(&money_state.mint_vk, &money_state.burn_vk)?;

let _note = tx.outputs[0].enc_note.decrypt(&keypair.secret)?;

let update = state_transition(&money_state, tx)?;
money_state.apply(update);

// Now spend
let owncoin = &money_state.own_coins[0];
let note = &owncoin.note;
let leaf_position = owncoin.leaf_position;
let root = money_state.tree.root(0).unwrap();
let merkle_path = money_state.tree.authentication_path(leaf_position, &root).unwrap();

let builder = TransactionBuilder {
    clear_inputs: vec![],
    inputs: vec![TransactionBuilderInputInfo {
        leaf_position,
        merkle_path,
        secret: keypair.secret,
        note: note.clone(),
    }],
    outputs: vec![TransactionBuilderOutputInfo {
        value: 110,
        token_id,
        public: keypair.public,
    }],
};

let tx = builder.build(&mint_pk, &burn_pk)?;

let update = state_transition(&money_state, tx)?;
money_state.apply(update);
*/
