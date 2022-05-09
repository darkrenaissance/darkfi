import sys
from classnamespace import ClassNamespace
from crypto import pallas_curve, ff_hash
from tx import TransactionBuilder

class State:

    def __init__(self):
        self.all_coins = set()
        self.nullifiers = set()

    def is_valid_merkle(self, all_coins):
        return all_coins.issubset(self.all_coins)

    def nullifier_exists(self, nullifier):
        return nullifier in self.nullifiers

    def apply(self, update):
        self.nullifiers = self.nullifiers.union(update.nullifiers)

        for coin, enc_note in zip(update.coins, update.enc_notes):
            self.all_coins.add(coin)

            # Try to decrypt notes here
            print(f"Received {enc_note.value} DRK")

def state_transition(state, tx):
    for input in tx.clear_inputs:
        pk = input.signature_public
        # Check pk is correct

    for input in tx.inputs:
        if not state.is_valid_merkle(input.revealed.all_coins):
            print(f"invalid merkle root", file=sys.stderr)
            return None

        nullifier = input.revealed.nullifier
        if state.nullifier_exists(nullifier):
            print(f"duplicate nullifier found", file=sys.stderr)
            return None

    is_verify, reason = tx.verify()
    if not is_verify:
        print(f"tx verify failed: {reason}", file=sys.stderr)
        return None

    update = ClassNamespace()
    update.nullifiers = [input.revealed.nullifier for input in tx.inputs]
    update.coins = [output.revealed.coin for output in tx.outputs]
    update.enc_notes = [output.enc_note for output in tx.outputs]
    return update

class DaoBuilder:

    def __init__(self, proposal_auth_public_key, threshold, quorum, ec):
        self.proposal_auth_public_key = proposal_auth_public_key
        self.threshold = threshold
        self.quorum = quorum

        self.ec = ec

    def build(self):
        mint_proof = DaoMintProof(
            self.proposal_auth_public_key,
            self.threshold,
            self.quorum,
            self.ec
        )
        revealed = mint_proof.get_revealed()

        dao = Dao(revealed, mint_proof, self.ec)
        return dao

class Dao:

    def __init__(self, revealed, mint_proof, ec):
        self.revealed = revealed
        self.mint_proof = mint_proof
        self.ec = ec

    def verify(self):
        if not self.mint_proof.verify(self.revealed):
            return False, "mint proof failed to verify"
        return True, None

# class DaoExec .etc

class DaoMintProof:

    def __init__(self, proposal_auth_public_key, threshold, quorum, ec):
        self.proposal_auth_public_key = proposal_auth_public_key
        self.threshold = threshold
        self.quorum = quorum
        self.ec = ec

    def get_revealed(self):
        revealed = ClassNamespace()

        revealed.bulla = ff_hash(
            self.ec.p,
            self.proposal_auth_public_key[0],
            self.proposal_auth_public_key[1],
            self.threshold,
            self.quorum
        )

        return revealed

    def verify(self, public):
        revealed = self.get_revealed()
        return True

# Shared between DaoMint and DaoExec
class DaoState:

    def __init__(self):
        self.bullas = set()

    def apply(self, update):
        self.bullas.add(update.bulla)

    def apply_exec(self, update):
        pass

# contract interface functions
def dao_state_transition(state, tx):
    is_verify, reason = tx.verify()
    if not is_verify:
        print(f"dao tx verify failed: {reason}", file=sys.stderr)
        return None

    update = ClassNamespace()
    update.bulla = tx.revealed.bulla
    return update

###### DAO EXEC

class DaoExecBuilder:

    def __init__(self):
        pass

    def build(self):
        tx = DaoExec()
        return tx

class DaoExec:

    def __init__(self):
        pass

class DaoExecProof:

    def __init__(self):
        pass

def dao_exec_state_transition(state, tx):
    update = ClassNamespace()
    return update

def main(argv):
    ec = pallas_curve()

    secret = ec.random_scalar()
    public = ec.multiply(secret, ec.G)

    initial_supply = 21000
    token_id = 110

    signature_secret = ec.random_scalar()

    # Setup the DAO
    proposal_auth_secret = ec.random_scalar()
    proposal_auth_public = ec.multiply(proposal_auth_secret, ec.G)
    threshold = 110
    quorum = 110
    builder = DaoBuilder(proposal_auth_public, threshold, quorum, ec)
    dao_tx = builder.build()

    # Each deployment of a contract has a unique state
    # associated with it.
    dao_state = DaoState()
    if (update := dao_state_transition(dao_state, dao_tx)) is None:
        return -1
    dao_state.apply(update)

    builder = TransactionBuilder(ec)
    builder.add_clear_input(initial_supply, token_id, signature_secret)
    depends = [b"0xdao_ruleset"]
    attrs = []
    builder.add_output(initial_supply, token_id, public, depends, attrs)
    tx = builder.build()

    state = State()
    if (update := state_transition(state, tx)) is None:
        return -1
    state.apply(update)

    # Now the depends field specifies the function DaoExec
    # so the tx above must also be combined with a DaoExec tx
    for input in tx.inputs:
        assert input.revealed.depends == [b"0xdao_ruleset"]
    builder = DaoExecBuilder()
    dao_tx = builder.build()
    if (update := dao_exec_state_transition(dao_state, dao_tx)) is None:
        return -1
    dao_state.apply_exec(update)

    # State
    # functions that can be called on state with params
    # functions return an update
    # optional encrypted values that can be read by wallets
    # --> (do this outside??)
    # --> penalized if fail
    # apply update to state

    # payment state transition in coin specifies dependency
    # the tx exists and ruleset is applied

    assert len(tx.outputs) > 0
    note = tx.outputs[0].enc_note
    coin = ff_hash(
        ec.p,
        public[0],
        public[1],
        note.value,
        note.token_id,
        note.serial,
        note.coin_blind,
        depends,
        attrs
    )
    assert coin == tx.outputs[0].mint_proof.get_revealed().coin
    all_coins = set([coin])

    builder = TransactionBuilder(ec)
    builder.add_input(all_coins, secret, note)

    secret2 = ec.random_scalar()
    public2 = ec.multiply(secret, ec.G)

    builder.add_output(1000, token_id, public2, depends=[b"0x0000"], attrs=[])
    # Change
    builder.add_output(note.value - 1000, token_id, public, depends, attrs)

    tx = builder.build()

    if (update := state_transition(state, tx)) is None:
        return -1
    state.apply(update)

    return 0

if __name__ == "__main__":
    sys.exit(main(sys.argv))

