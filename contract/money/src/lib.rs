/// Available functions in this contract.
/// We only identify them with a single byte passed through the payload.
#[repr(u8)]
pub enum Function {
    Transfer = 0x00,
}

impl From<u8> for Function {
    fn from(b: u8) -> Self {
        match b {
            0x00 => Self::Transfer,
            _ => panic!("Invalid function ID: {:#04x?}", b),
        }
    }
}

/// This state is serialized and stored on-chain. See `src/blockchain/statestore.rs`.
#[repr(C)]
#[derive(SerialEncodable, SerialDecodable)]
pub struct State {
    pub tree: BridgeTree<MerkleNode, MERKLE_DEPTH>,
    pub merkle_roots: Vec<MerkleNode>,
    pub nullifiers: Vec<Nullifier>,
}

pub struct Update {
    pub nullifiers: Vec<Nullifier>,
    pub coins: Vec<Coin>,
}

impl State {
    pub fn is_valid_merkle(&self, merkle_root: &MerkleNode) -> bool {
        self.merkle_roots.iter().any(|m| m == merkle_root)
    }

    pub fn nullfier_exists(&self, nullifier: &Nullifier) -> bool {
        self.nullifiers.iter().any(|n| n == nullifier)
    }

    pub fn update(&mut self, state_update: Update) {
        self.nullifiers.extend_from_slice(&state_update.nullifiers);
        for coin in state_update.coins {
            self.tree.append(&MerkleNode(coin.inner()));
            self.merkle_roots.push(self.tree.root(0).unwrap());
        }
    }
}

fn transfer(state: &mut State, payload: &[u8]) -> ContractResult {
    let tx: Transaction = deserialize(payload)?;

    // TODO: Clear inputs. Cashier + Faucet logic is bad and needs to be
    // solved in another way.

    // Nullifiers in the transaction
    let mut nullifiers = Vec::with_capacity(tx.inputs.len());

    msg!("Iterate inputs");
    for (i, input) in tx.inputs.iter().enumerate() {
        let merkle_root = *input.revealed.merkle_root;
        let spend_hook = *input.revealed.spend_hook;
        let nullifier = *input.revealed.nullifier;

        // The Merkle root is used to know whether this is a coin that
        // existed in a previous state.
        if !state.is_valid_merkle(&merkle_root) {
            msg!("Error: Invalid Merkle root (input {})", i);
            msg!("Root: {:?}", merkle_root);
            return Err(ContractError::Custom(30))
        }

        // Check the spend_hook is satisfied.
        // The spend_hook says a coin must invoke another contract function
        // when being spent. If the value is set, then we check the function
        // call exists.
        if spend_hook != pallas::Base::zero() {
            // spend_hook is set, so we enforce the rules.
            todo!();
        }

        // The nullifiers should not already exists - double-spend protection.
        if state.nullifier_exists(&nullifier) || nullifiers.contains(&nullifier) {
            msg!("Duplicate nullifier found (input {})", i);
            msg!("Nullifier: {:?}", nullifier);
            return Err(ContractError::Custom(31))
        }

        nullifiers.push(nullifier);

        // Verify transaction
        match self.verify() {
            Ok(()) => msg!("tx verified successfully"),
            Err(e) => {
                msg!("tx failed to verify");
                return Err(e)
            }
        }

        let mut coins = Vec::with_capacity(tx.outputs.len());
        for output in tx.outputs {
            coins.push(output.revealed.coin);
        }

        let state_update = Update { nullifiers, coins };
        state.update(&state_update);
        apply_state(&serialize(&state))?;
        Ok(())
    }
}

impl Verification for Transaction {
    pub fn verify(&self) -> ContractResult {
        // Must have minimum 1 clear or anon input, and 1 output
        if self.clear_inputs.len() + self.inputs.len() == 0 {
            msg!("Error: Missing inputs in transaction");
            return Err(ContractError::Custom(32))
        }

        if self.outputs.is_empty() {
            msg!("Error: Missing outputs in transaction");
            return Err(ContractError::Custom(33))
        }

        // Accumulator for the value commitments
        let mut valcom_total = DrkValueCommit::identity();

        // Add values from the clear inputs
        for input in &self.clear_inputs {
            valcom_total += pedersen_commitment_u64(input.value, input.value_blind);
        }

        // Add values from the inputs
        for input in &self.inputs {
            valcom_total += input.revealed.value_commit;
        }

        // Subtract values from the outputs
        for output in &self.outputs {
            valcom_total -= output.revealed.value_commit;
        }

        // If the accumulator is not back in its initial state,
        // there's a value mismatch.
        if valcom_total != DrkValueCommit::identity() {
            msg!("Error: Missing funds");
            return Err(ContractError::Custom(34))
        }

        // Verify that the token commitments match
        let token_commit_value = self.outputs[0].revealed.token_commit;
        let mut failed =
            self.inputs.iter().any(|input| input.revealed.token_commit != token_commit_value);
        failed = failed ||
            self.outputs.iter().any(|output| output.revealed.token_commit != token_commit_value);
        failed = failed ||
            self.clear_inputs.iter().any(|input| {
                pedersen_commitment_base(input.token_id, input.token_blind) != token_commit_value
            });

        if !failed {
            msg!("Error: Token ID mismatch");
            return Err(ContractError::Custom(35))
        }

        Ok(())
    }
}

#[cfg(not(feature = "no-entrypoint"))]
entrypoint!(process_instruction);
fn process_instruction(contract_id: &ContractId, ix: &[u8]) -> ContractResult {
    // Using the `contract_id` (fed by the wasm runtime), we find our state in
    // the sled database, and try to deserialize it into the `State` struct that
    // is defined in this smart contract.
    // TODO: FIXME: The deserialization needs to be partial, because in the ledger
    // the smart contract deployer is supposed to allocate the space for this data
    // and whatever is unused should be zeroes.
    let mut state: State = deserialize(&lookup_state(contract_id)?)?;

    match Function::from(ix[0]) {
        Function::Transfer => transfer(&mut state, &ix[1..])?,
    }

    Ok(())
}
