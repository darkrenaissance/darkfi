import hashlib
import sys
from collections import namedtuple
from classnamespace import ClassNamespace
from crypto import pallas_curve

class TransactionBuilder:

    def __init__(self, ec):
        self.clear_inputs = []
        self.inputs = []
        self.outputs = []

        self.ec = ec

    def add_clear_input(self, value, token_id, signature_secret):
        clear_input = ClassNamespace()
        clear_input.value = value
        clear_input.token_id = token_id
        clear_input.signature_secret = signature_secret
        self.clear_inputs.append(clear_input)

    def add_input(self, input):
        self.inputs.append(input)

    def add_output(self, value, token_id, public):
        output = ClassNamespace()
        output.value = value
        output.token_id = token_id
        output.public = public
        self.outputs.append(output)

    def compute_remainder_blind(self, clear_inputs, input_blinds,
                                output_blinds):
        total = 0
        total += sum(input.value_blind for input in clear_inputs)
        total += sum(input_blinds)
        total -= sum(output_blinds)
        return total % self.ec.order

    def build(self):
        tx = Transaction(self.ec)
        token_blind = self.ec.random_scalar()

        for input in self.clear_inputs:
            tx_clear_input = ClassNamespace()
            tx_clear_input.__name__ = "TransactionClearInput"
            tx_clear_input.value = input.value
            tx_clear_input.token_id = input.token_id
            tx_clear_input.value_blind = self.ec.random_scalar()
            tx_clear_input.token_blind = token_blind
            tx_clear_input.signature_public = self.ec.multiply(
                input.signature_secret, self.ec.G)
            tx.clear_inputs.append(tx_clear_input)

        input_blinds = []
        for input in self.inputs:
            tx_input = ClassNamespace()
            tx.inputs.append(tx_input)

        assert self.outputs
        output_blinds = []
        for i, output in enumerate(self.outputs):
            if i == len(self.outputs) - 1:
                value_blind = self.compute_remainder_blind(
                    tx.clear_inputs, input_blinds, output_blinds)
            else:
                value_blind = self.ec.random_scalar()
            output_blinds.append(value_blind)

            note = ClassNamespace()
            note.serial = self.ec.random_base()
            note.value = output.value
            note.token_id = output.token_id
            note.coin_blind = self.ec.random_base()
            note.value_blind = value_blind
            note.token_blind = token_blind

            tx_output = ClassNamespace()
            tx_output.__name__ = "TransactionOutput"

            tx_output.mint_proof = MintProof(
                note.value, note.token_id, note.value_blind,
                note.token_blind, note.serial, note.coin_blind,
                output.public, self.ec)
            tx_output.revealed = tx_output.mint_proof.get_revealed()
            assert tx_output.mint_proof.verify(tx_output.revealed)

            # Is normally encrypted
            tx_output.enc_note = note
            tx_output.enc_note.__name__ = "TransactionOutputEncryptedNote"

            tx.outputs.append(tx_output)

        unsigned_tx_data = tx.partial_encode()
        for (input, info) in zip(tx.clear_inputs, self.clear_inputs):
            secret = info.signature_secret
            signature = sign(unsigned_tx_data, secret, self.ec)
            input.signature = signature
        for (input, info) in zip(tx.inputs, self.inputs):
            secret = info.signature_secret
            signature = sign(unsigned_tx_data, secret, self.ec)
            input.signature = signature

        return tx

class MintProof:

    def __init__(self, value, token_id, value_blind, token_blind, serial,
                 coin_blind, public, ec):
        self.value = value
        self.token_id = token_id
        self.value_blind = value_blind
        self.token_blind = token_blind
        self.serial = serial
        self.coin_blind = coin_blind
        self.public = public

        self.ec = ec

    def get_revealed(self):
        revealed = ClassNamespace()
        revealed.coin = ff_hash(
            self.ec.p,
            self.public[0],
            self.public[1],
            self.value,
            self.token_id,
            self.serial,
            self.coin_blind
        )

        revealed.value_commit = pedersen_encrypt(
            self.value, self.value_blind, self.ec
        )
        revealed.token_commit = pedersen_encrypt(
            self.token_id, self.token_blind, self.ec
        )

        return revealed

    def verify(self, public):
        revealed = self.get_revealed()
        return all([
            revealed.coin == public.coin,
            revealed.value_commit == public.value_commit,
            revealed.token_commit == public.token_commit
        ])

def pedersen_encrypt(x, y, ec):
    vcv = ec.multiply(x, ec.G)
    vcr = ec.multiply(y, ec.H)
    return ec.add(vcv, vcr)

def ff_hash(p, *args):
    hasher = hashlib.sha256()
    for arg in args:
        match arg:
            case int() as arg:
                hasher.update(arg.to_bytes(32, byteorder="little"))
            case bytes() as arg:
                hasher.update(arg)
            case _:
                raise Exception(f"unknown hash arg '{arg}' type: {type(arg)}")
    value = int.from_bytes(hasher.digest(), byteorder="little")
    return value % p

def hash_point(point, message=None):
    hasher = hashlib.sha256()
    for x_i in point:
        hasher.update(x_i.to_bytes(32, byteorder="little"))
    # Optional message
    if message is not None:
        hasher.update(message)
    value = int.from_bytes(hasher.digest(), byteorder="little")
    return value

def sign(message, secret, ec):
    ephem_secret = ec.random_scalar()
    ephem_public = ec.multiply(ephem_secret, ec.G)
    challenge = hash_point(ephem_public, message) % ec.order
    response = (ephem_secret + challenge * secret) % ec.order
    return ephem_public, response

def verify(message, signature, public, ec):
    ephem_public, response = signature
    challenge = hash_point(ephem_public, message) % ec.order
    # sG
    lhs = ec.multiply(response, ec.G)
    # R + cP
    rhs_cP = ec.multiply(challenge, public)
    rhs = ec.add(ephem_public, rhs_cP)
    return lhs == rhs

class Transaction:

    def __init__(self, ec):
        self.clear_inputs = []
        self.inputs = []
        self.outputs = []

        self.ec = ec

    def partial_encode(self):
        return b"hello"

    def verify(self):
        if not self._check_value_commits():
            return False, "value commits do not match"

        if not self._check_proofs():
            return False, "proofs failed to verify"

        if not self._verify_token_commitments():
            return False, "token ID mismatch"

        unsigned_tx_data = self.partial_encode()
        for input in self.clear_inputs:
            public = input.signature_public
            if not verify(unsigned_tx_data, input.signature, public, self.ec):
                return False
        for input in self.inputs:
            public = input.revealed.signature_public
            if not verify(unsigned_tx_data, input.signature, public, self.ec):
                return False

        return True, None

    def _check_value_commits(self):
        valcom_total = (0, 1, 0)

        for input in self.clear_inputs:
            value_commit = pedersen_encrypt(input.value, input.value_blind,
                                            self.ec)
            valcom_total = self.ec.add(valcom_total, value_commit)
        for input in self.inputs:
            value_commit = input.revealed.value_commit
            valcom_total = self.ec.add(valcom_total, value_commit)
        for output in self.outputs:
            v = output.revealed.value_commit
            value_commit = (v[0], -v[1], v[2])
            valcom_total = self.ec.add(valcom_total, value_commit)

        return valcom_total == (0, 1, 0)

    def _check_proofs(self):
        for input in self.inputs:
            if not input.burn_proof.verify(input.revealed):
                return False
        for output in self.outputs:
            if not output.mint_proof.verify(output.revealed):
                return False
        return True

    def _verify_token_commitments(self):
        assert len(self.outputs) > 0
        token_commit_value = self.outputs[0].revealed.token_commit
        for input in self.clear_inputs:
            token_commit = pedersen_encrypt(input.token_id, input.token_blind,
                                            self.ec)
            if token_commit != token_commit_value:
                return False
        for input in self.inputs:
            if input.revealed.token_commit != token_commit_value:
                return False
        for output in self.outputs:
            if output.revealed.token_commit != token_commit_value:
                return False
        return True

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

    return 0

if __name__ == "__main__":
    sys.exit(main(sys.argv))

