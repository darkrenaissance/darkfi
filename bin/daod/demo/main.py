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

def main(argv):
    ec = pallas_curve()

    secret = ec.random_scalar()
    public = ec.multiply(secret, ec.G)

    initial_supply = 21000
    token_id = 110

    signature_secret = ec.random_scalar()

    builder = TransactionBuilder(ec)
    builder.add_clear_input(initial_supply, token_id, signature_secret)
    builder.add_output(initial_supply, token_id, public)
    tx = builder.build()

    state = State()
    if (update := state_transition(state, tx)) is None:
        return -1
    state.apply(update)

    assert len(tx.outputs) > 0
    note = tx.outputs[0].enc_note
    coin = ff_hash(
        ec.p,
        public[0],
        public[1],
        note.value,
        note.token_id,
        note.serial,
        note.coin_blind
    )
    assert coin == tx.outputs[0].mint_proof.get_revealed().coin
    all_coins = set([coin])

    builder = TransactionBuilder(ec)
    builder.add_input(all_coins, secret, note)

    secret2 = ec.random_scalar()
    public2 = ec.multiply(secret, ec.G)

    builder.add_output(1000, token_id, public2)
    # Change
    builder.add_output(note.value - 1000, token_id, public)

    tx = builder.build()

    if (update := state_transition(state, tx)) is None:
        return -1
    state.apply(update)

    return 0

if __name__ == "__main__":
    sys.exit(main(sys.argv))

