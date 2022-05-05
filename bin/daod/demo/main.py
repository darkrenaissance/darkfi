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
        tx = Transaction()
        token_blind = self.ec.random_scalar()

        for input in self.clear_inputs:
            tx_clear_input = ClassNamespace()
            tx_clear_input.value = input.value
            tx_clear_input.token_id = input.token_id
            tx_clear_input.value_blind = self.ec.random_scalar()
            tx_clear_input.token_blind = input.token_blind
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

            tx_output = ClassNamespace()

            tx_output.mint_proof = MintProof(
                note.value, note.token_id, note.value_blind,
                token_blind, tx_output.serial, coin_blind, public)
            tx_output.revealed = tx_output.mint_proof.get_revealed()
            # Is normally encrypted
            tx_output.enc_note = note

            tx.outputs.append(tx_output)

        return tx

class MintProof:

    def __init__(self, value, token_id, value_blind, token_blind, serial,
                 coin_blind, public):
        self.value = value
        self.token_id = token_id
        self.value_blind = value_blind
        self.token_blind = token_blind
        self.serial = serial
        self.coin_blind = coin_blind
        self.public = public

    def get_revealed(self):
        revealed = ClassNamespace()
        return revealed

    def verify(self):
        pass

class Transaction:

    def __init__(self):
        self.clear_inputs = []
        self.inputs = []
        self.outputs = []

def main(argv):
    ec = pallas_curve()
    builder = TransactionBuilder(ec)
    builder.add_output(44, 110, "1234")
    tx = builder.build()

if __name__ == "__main__":
    sys.exit(main(sys.argv))

