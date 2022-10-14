use super::State;

/// This function is the execution of the `Transfer` functionality.
pub fn exec(state: &mut State, tx: Transaction) -> ContractResult {
    // TODO: Clear inputs. Cashier+Faucet logic is bad and needs to
    // be solved in another better way.

    // Nullifiers in the transaction
    let mut nullifiers = Vec::with_capacity(tx.inputs.len());

    msg!("Iterating over inputs");
    for (i, input) in tx.inputs.iter().enumerate() {
        let merkle_root = input.revealed.merkle_root;
        let spend_hook = input.revealed.spend_hook;
        let nullifier = input.revealed.nullifier;

        // The Merkle root is used to know whether this is a coin
        // that existed in a previous state.
        if !state.is_valid_merkle(&merkle_root) {
            msg!("Error: Invalid Merkle root (input {})", i);
            msg!("Root: {:?}", merkle_root);
            return Err(ContractError::Custom(30))
        }

        // Check the spend_hook is satisfied.
        // The spend_hook says a coin must invoke another contract
        // function when being spent. If the value is set, then we
        // check the function call exists.
        if spend_hook != pallas::Base::zero() {
            todo!();
        }

        // The nullifiers should not already exist. This gives us
        // protection against double-spending.
        if state.nullifier_exists(&nullifier) || nullifiers.contains(&nullifier) {
            msg!("Duplicate nulliier found (input {})", i);
            msg!("Nullifier: {:?}", nullifier);
            return Err(ContractError::Custom(31))
        }

        // Add the nullifier to the list of seen nullifiers.
        nullifiers.push(nullifier);
    }

    // Verify transaction
    match tx.verify() {
        Ok(()) => msg!("Transaction verified successfully"),
        Err(e) => {
            msg!("Transaction failed to verify");
            return Err(e)
        }
    }

    msg!("Applying state update");
    state.nullifiers.extend_from_slice(&nullifiers);
    for output in tx.outputs {
        state.tree.append(&MerkleNode(coin.inner()));
        state.merkle_roots.push(state.tree.root(0).unwrap());
    }
}

// `Verification` could be a generic trait we implement for doing
// arbitrary verification in contracts.
impl Verification for Transaction {
    pub fn verify(&self) -> ContractResult {
        // Must have minimum 1 clear or anon input
        if self.clear_inputs.len() + self.inputs.len() == 0 {
            msg!("Error: Missing inputs in transaction");
            return Err(ContractError::Custom(32))
        }

        // Also minimum 1 output
        if self.outputs.is_empty() {
            msg!("Error: Missing outputs in transaction");
            return Err(ContractError::Custom(33))
        }

        // Accumulator for the value commitments
        let mut valcom_total = ValueCommit::identity();

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
        if valcom_total != ValueCommit::identity() {
            msg!("Error: Missing funds");
            return Err(ContractError::Custom(34))
        }

        // Verify that the token commitments match
        let tokval = self.outputs[0].revealed.token_commit;
        let mut failed = self.inputs.iter().any(|input| input.revealed.token_commit != tokval);
        failed = failed || self.outputs.iter().any(|output| output.revealed.token_commit != tokval);
        failed = failed ||
            self.clear_inputs.iter().any(|input| {
                pedersen_commitment_base(input.token_id, input.token_blind) != tokval
            });

        if failed {
            msg!("Error: Token ID mismatch");
            return Err(ContractError::Custom(35))
        }

        Ok(())
    }
}
