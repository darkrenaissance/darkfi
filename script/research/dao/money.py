/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2023 Dyne.org foundation
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as
 * published by the Free Software Foundation, either version 3 of the
 * License, or (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU Affero General Public License for more details.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

from classnamespace import ClassNamespace
from crypto import ff_hash, pedersen_encrypt, sign, verify

# Tx representing send_payment() contract call
class SendPaymentTxBuilder:

    def __init__(self, ec):
        self.clear_inputs = []
        self.inputs = []
        self.outputs = []
        self.input_blinds = []
        self.output_blinds = []

        self.ec = ec

    def add_clear_input(self, value, token_id, signature_secret):
        clear_input = ClassNamespace()
        clear_input.value = value
        clear_input.token_id = token_id
        clear_input.signature_secret = signature_secret
        self.clear_inputs.append(clear_input)

    def add_input(self, all_coins, secret, note, user_data_blind):
        input = ClassNamespace()
        input.all_coins = all_coins
        input.secret = secret
        input.note = note
        input.user_data_blind = user_data_blind
        self.inputs.append(input)

    def add_output(self, value, token_id, public, spend_hook, user_data):
        output = ClassNamespace()
        output.value = value
        output.token_id = token_id
        output.public = public
        output.spend_hook = spend_hook
        output.user_data = user_data
        self.outputs.append(output)

    def compute_remainder_blind(self, clear_inputs, input_blinds,
                                output_blinds):
        total = 0
        total += sum(input.value_blind for input in clear_inputs)
        total += sum(input_blinds)
        total -= sum(output_blinds)
        return total % self.ec.order

    def build(self):
        tx = SendPaymentTx(self.ec)
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

        self.input_blinds = []
        signature_secrets = []
        for input in self.inputs:
            value_blind = self.ec.random_scalar()
            self.input_blinds.append(value_blind)

            signature_secret = self.ec.random_scalar()
            signature_secrets.append(signature_secret)

            tx_input = ClassNamespace()
            tx_input.__name__ = "TransactionInput"
            tx_input.burn_proof = BurnProof(
                input.note.value, input.note.token_id, value_blind,
                token_blind, input.note.serial, input.note.coin_blind,
                input.secret, input.note.spend_hook, input.note.user_data,
                input.user_data_blind, input.all_coins, signature_secret,
                self.ec)
            tx_input.revealed = tx_input.burn_proof.get_revealed()
            tx.inputs.append(tx_input)

        assert self.outputs
        self.output_blinds = []
        for i, output in enumerate(self.outputs):
            if i == len(self.outputs) - 1:
                value_blind = self.compute_remainder_blind(
                    tx.clear_inputs, self.input_blinds, self.output_blinds)
            else:
                value_blind = self.ec.random_scalar()
            self.output_blinds.append(value_blind)

            note = ClassNamespace()
            note.serial = self.ec.random_base()
            note.value = output.value
            note.token_id = output.token_id
            note.coin_blind = self.ec.random_base()
            note.value_blind = value_blind
            note.token_blind = token_blind
            note.spend_hook = output.spend_hook
            note.user_data = output.user_data

            tx_output = ClassNamespace()
            tx_output.__name__ = "TransactionOutput"

            tx_output.mint_proof = MintProof(
                note.value, note.token_id, note.value_blind,
                note.token_blind, note.serial, note.coin_blind,
                output.public, output.spend_hook, output.user_data, self.ec)
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
        for (input, signature_secret) in zip(tx.inputs, signature_secrets):
            signature = sign(unsigned_tx_data, signature_secret, self.ec)
            input.signature = signature

        return tx

# Transaction representing Money::send_payment() function call
class SendPaymentTx:

    def __init__(self, ec):
        self.clear_inputs = []
        self.inputs = []
        self.outputs = []

        self.ec = ec

    def partial_encode(self):
        # There is no cake
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

class BurnProof:

    def __init__(self, value, token_id, value_blind, token_blind, serial,
                 coin_blind, secret, spend_hook, user_data, user_data_blind,
                 all_coins, signature_secret, ec):
        self.value = value
        self.token_id = token_id
        self.value_blind = value_blind
        self.token_blind = token_blind
        self.serial = serial
        self.coin_blind = coin_blind
        self.secret = secret
        self.spend_hook = spend_hook
        self.user_data = user_data
        self.user_data_blind = user_data_blind
        self.all_coins = all_coins
        self.signature_secret = signature_secret

        self.ec = ec

    def get_revealed(self):
        revealed = ClassNamespace()
        revealed.nullifier = ff_hash(self.ec.p, self.secret, self.serial)

        revealed.value_commit = pedersen_encrypt(
            self.value, self.value_blind, self.ec
        )
        revealed.token_commit = pedersen_encrypt(
            self.token_id, self.token_blind, self.ec
        )

        # is_valid_merkle_root()
        revealed.all_coins = self.all_coins

        revealed.signature_public = self.ec.multiply(self.signature_secret,
                                                     self.ec.G)

        # This is fully public, no merkle tree or anything
        revealed.spend_hook = self.spend_hook

        # Re-export user_data field for access by other contracts
        revealed.enc_user_data = ff_hash(
            self.ec.p,
            self.user_data,
            self.user_data_blind
        )

        return revealed

    def verify(self, public):
        revealed = self.get_revealed()

        public_key = self.ec.multiply(self.secret, self.ec.G)
        coin = ff_hash(
            self.ec.p,
            public_key[0],
            public_key[1],
            self.value,
            self.token_id,
            self.serial,
            self.coin_blind,
            self.spend_hook,
            self.user_data,
        )
        # Merkle root check
        if coin not in self.all_coins:
            return False

        return all([
            revealed.nullifier == public.nullifier,
            revealed.value_commit == public.value_commit,
            revealed.token_commit == public.token_commit,
            revealed.all_coins == public.all_coins,
            revealed.signature_public == public.signature_public,
            revealed.enc_user_data == public.enc_user_data
        ])

class MintProof:

    def __init__(self, value, token_id, value_blind, token_blind, serial,
                 coin_blind, public, spend_hook, user_data, ec):
        self.value = value
        self.token_id = token_id
        self.value_blind = value_blind
        self.token_blind = token_blind
        self.serial = serial
        self.coin_blind = coin_blind
        self.public = public
        self.spend_hook = spend_hook
        self.user_data = user_data

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
            self.coin_blind,
            self.spend_hook,
            self.user_data
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
            revealed.token_commit == public.token_commit,
        ])

