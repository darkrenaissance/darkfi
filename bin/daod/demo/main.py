import sys
from classnamespace import ClassNamespace
from crypto import pallas_curve, ff_hash
from tx import TransactionBuilder

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

    is_verify, reason = tx.verify()
    if not is_verify:
        print(f"tx verify failed: {reason}", file=sys.stderr)
        return -1

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
    all_coins = [coin]

    builder = TransactionBuilder(ec)
    builder.add_input(all_coins, secret, note)

    secret2 = ec.random_scalar()
    public2 = ec.multiply(secret, ec.G)

    builder.add_output(1000, token_id, public2)
    # Change
    builder.add_output(note.value - 1000, token_id, public)

    tx = builder.build()

    is_verify, reason = tx.verify()
    if not is_verify:
        print(f"tx2 verify failed: {reason}", file=sys.stderr)
        return -1

    return 0

if __name__ == "__main__":
    sys.exit(main(sys.argv))

