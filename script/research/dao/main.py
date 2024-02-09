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

import sys
from classnamespace import ClassNamespace

import crypto, money

class MoneyState:

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

def money_state_transition(state, tx):
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

class ProposerTxBuilder:

    def __init__(self, proposal, all_dao_bullas, ec):
        self.inputs = []
        self.proposal = proposal
        self.all_dao_bullas = all_dao_bullas

        self.ec = ec

    def add_input(self, all_coins, secret, note):
        input = ClassNamespace()
        input.all_coins = all_coins
        input.secret = secret
        input.note = note
        self.inputs.append(input)

    def set_dao(self, dao):
        self.dao = dao

    def build(self):
        tx = ProposerTx(self.ec)
        token_blind = self.ec.random_scalar()
        enc_bulla_blind = self.ec.random_base()

        total_value = sum(input.note.value for input in self.inputs)
        input_value_blinds = [self.ec.random_scalar() for _ in self.inputs]
        total_value_blinds = sum(input_value_blinds)

        tx.dao = ClassNamespace()
        tx.dao.__name__ = "ProposerTxDao"
        # We export proposer_limit as an encrypted value from the DAO
        tx.dao.proof = ProposerTxDaoProof(
            # Value commit
            total_value,
            total_value_blinds,
            # DAO params
            self.dao.proposer_limit,
            self.dao.quorum,
            self.dao.approval_ratio,
            self.dao.gov_token_id,
            self.dao.public_key,
            self.dao.bulla_blind,
            # Token commit
            token_blind,
            # Used by other DAO members to verify the bulla
            # used in this proof is for the actual DAO
            enc_bulla_blind,
            # Proposal
            self.proposal.dest,
            self.proposal.amount,
            self.proposal.serial,
            self.proposal.token_id,
            self.proposal.blind,
            # Merkle witness
            self.all_dao_bullas,
            self.ec
        )
        tx.dao.revealed = tx.dao.proof.get_revealed()

        # Members of the DAO need to themselves verify this is the correct
        # bulla they are voting on, so we encrypt the blind to them
        tx.note = ClassNamespace()
        tx.note.enc_bulla_blind = enc_bulla_blind
        tx.note.proposal = self.proposal

        signature_secrets = []
        for input, value_blind in zip(self.inputs, input_value_blinds):
            signature_secret = self.ec.random_scalar()
            signature_secrets.append(signature_secret)

            tx_input = ClassNamespace()
            tx_input.__name__ = "TransactionInput"
            tx_input.proof = ProposerTxInputProof(
                input.note.value, input.note.token_id, value_blind,
                token_blind, input.note.serial, input.note.coin_blind,
                input.secret, input.note.spend_hook, input.note.user_data,
                input.all_coins, signature_secret, self.ec)
            tx_input.revealed = tx_input.proof.get_revealed()
            tx.inputs.append(tx_input)

        unsigned_tx_data = tx.partial_encode()
        for (input, signature_secret) in zip(tx.inputs, signature_secrets):
            signature = crypto.sign(unsigned_tx_data, signature_secret, self.ec)
            input.signature = signature

        return tx

class ProposerTx:

    def __init__(self, ec):
        self.inputs = []
        self.dao = None
        self.note = None

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
        for input in self.inputs:
            public = input.revealed.signature_public
            if not crypto.verify(unsigned_tx_data, input.signature,
                                 public, self.ec):
                return False

        return True, None

    def _check_value_commits(self):
        valcom_total = (0, 1, 0)

        for input in self.inputs:
            value_commit = input.revealed.value_commit
            valcom_total = self.ec.add(valcom_total, value_commit)

        return valcom_total == self.dao.revealed.value_commit

    def _check_proofs(self):
        for input in self.inputs:
            if not input.proof.verify(input.revealed):
                return False
        if not self.dao.proof.verify(self.dao.revealed):
            return False
        return True

    def _verify_token_commitments(self):
        token_commit_value = self.dao.revealed.token_commit
        for input in self.inputs:
            if input.revealed.token_commit != token_commit_value:
                return False
        return True

class ProposerTxInputProof:

    def __init__(self, value, token_id, value_blind, token_blind, serial,
                 coin_blind, secret, spend_hook, user_data,
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
        self.all_coins = all_coins
        self.signature_secret = signature_secret

        self.ec = ec

    def get_revealed(self):
        revealed = ClassNamespace()

        revealed.value_commit = crypto.pedersen_encrypt(
            self.value, self.value_blind, self.ec
        )
        revealed.token_commit = crypto.pedersen_encrypt(
            self.token_id, self.token_blind, self.ec
        )

        # is_valid_merkle_root()
        revealed.all_coins = self.all_coins

        revealed.signature_public = self.ec.multiply(self.signature_secret,
                                                     self.ec.G)

        return revealed

    def verify(self, public):
        revealed = self.get_revealed()

        public_key = self.ec.multiply(self.secret, self.ec.G)
        coin = crypto.ff_hash(
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
            revealed.value_commit == public.value_commit,
            revealed.token_commit == public.token_commit,
            revealed.all_coins == public.all_coins,
            revealed.signature_public == public.signature_public
        ])

class ProposerTxDaoProof:

    def __init__(self, total_value, total_value_blinds,
                 proposer_limit, quorum, approval_ratio,
                 gov_token_id, dao_public_key, dao_bulla_blind,
                 token_blind, enc_bulla_blind,
                 proposal_dest, proposal_amount, proposal_serial,
                 proposal_token_id, proposal_blind,
                 all_dao_bullas, ec):
        self.total_value = total_value
        self.total_value_blinds = total_value_blinds
        self.proposer_limit = proposer_limit
        self.quorum = quorum
        self.approval_ratio = approval_ratio
        self.gov_token_id = gov_token_id
        self.dao_public_key = dao_public_key
        self.dao_bulla_blind = dao_bulla_blind
        self.token_blind = token_blind
        self.enc_bulla_blind = enc_bulla_blind
        self.proposal_dest = proposal_dest
        self.proposal_amount = proposal_amount
        self.proposal_serial = proposal_serial
        self.proposal_token_id = proposal_token_id
        self.proposal_blind = proposal_blind
        self.all_dao_bullas = all_dao_bullas
        self.ec = ec

    def get_revealed(self):
        revealed = ClassNamespace()
        # Value commit
        revealed.value_commit = crypto.pedersen_encrypt(
            self.total_value, self.total_value_blinds, self.ec
        )
        # Token ID
        revealed.token_commit = crypto.pedersen_encrypt(
            self.gov_token_id, self.token_blind, self.ec
        )
        # encrypted DAO bulla
        bulla = crypto.ff_hash(
            self.ec.p,
            self.proposer_limit,
            self.quorum,
            self.approval_ratio,
            self.gov_token_id,
            self.dao_public_key[0],
            self.dao_public_key[1],
            self.dao_bulla_blind
        )
        revealed.enc_bulla = crypto.ff_hash(self.ec.p, bulla, self.enc_bulla_blind)
        # encrypted proposal
        revealed.proposal_bulla = crypto.ff_hash(
            self.ec.p,
            self.proposal_dest[0],
            self.proposal_dest[1],
            self.proposal_amount,
            self.proposal_serial,
            self.proposal_token_id,
            self.proposal_blind,
            bulla
        )
        # The merkle root
        revealed.all_dao_bullas = self.all_dao_bullas
        return revealed

    def verify(self, public):
        revealed = self.get_revealed()

        bulla = crypto.ff_hash(
            self.ec.p,
            self.proposer_limit,
            self.quorum,
            self.approval_ratio,
            self.gov_token_id,
            self.dao_public_key[0],
            self.dao_public_key[1],
            self.dao_bulla_blind
        )
        # Merkle root check
        if bulla not in self.all_dao_bullas:
            return False

        # This should not be able to be bigger than 2^64
        assert self.proposal_amount > 0

        #
        #   total_value >= proposer_limit
        #
        if not self.total_value >= self.proposer_limit:
            return False

        return all([
            revealed.value_commit == public.value_commit,
            revealed.token_commit == public.token_commit,
            revealed.enc_bulla == public.enc_bulla,
            revealed.proposal_bulla == public.proposal_bulla,
            revealed.all_dao_bullas == public.all_dao_bullas
        ])

class VoteTxBuilder:

    def __init__(self, ec):
        self.inputs = []
        self.vote_option = None

        self.ec = ec

    def add_input(self, all_coins, secret, note):
        input = ClassNamespace()
        input.all_coins = all_coins
        input.secret = secret
        input.note = note
        self.inputs.append(input)

    def set_vote_option(self, vote_option):
        assert vote_option == 0 or vote_option == 1
        self.vote_option = vote_option

    def build(self):
        tx = VoteTx(self.ec)
        token_blind = self.ec.random_scalar()

        assert self.vote_option is not None
        vote_option_blind = self.ec.random_base()

        total_value, total_blind = 0, 0
        signature_secrets = []
        for input in self.inputs:
            value_blind = self.ec.random_scalar()
            total_blind = (total_blind + value_blind) % self.ec.order
            total_value = (total_value + input.note.value) % self.ec.order

            signature_secret = self.ec.random_scalar()
            signature_secrets.append(signature_secret)

            tx_input = ClassNamespace()
            tx_input.__name__ = "TransactionInput"
            tx_input.burn_proof = VoteBurnProof(
                input.note.value, input.note.token_id, value_blind,
                token_blind, input.note.serial, input.note.coin_blind,
                input.secret, input.note.spend_hook, input.note.user_data,
                input.all_coins, signature_secret,
                self.ec)
            tx_input.revealed = tx_input.burn_proof.get_revealed()
            tx.inputs.append(tx_input)

        assert len(self.inputs) > 0
        token_id = self.inputs[0].note.token_id

        vote_blind = self.ec.random_scalar()

        # This whole tx is like just burning tokens
        # except we produce an output commitment to the total value in
        tx.vote = ClassNamespace()
        tx.vote.__name__ = "Vote"
        tx.vote.proof = VoteProof(total_value, token_id,
                                  total_blind, token_blind, vote_blind,
                                  self.vote_option, vote_option_blind,
                                  self.ec)
        tx.vote.revealed = tx.vote.proof.get_revealed()

        # We can use Shamir's Secret Sharing to unlock this at the end
        # of the voting, or even with a time delay to avoid timing attacks
        tx.note = ClassNamespace()
        tx.note.__name__ = "EncryptedNoteForDaoMembers"
        tx.note.value = total_value
        tx.note.token_id = token_id
        tx.note.vote_option = self.vote_option
        tx.note.value_blind = total_blind
        tx.note.token_blind = token_blind
        tx.note.vote_blind = vote_blind
        tx.note.vote_option_blind = vote_option_blind

        unsigned_tx_data = tx.partial_encode()
        for (input, signature_secret) in zip(tx.inputs, signature_secrets):
            signature = crypto.sign(unsigned_tx_data, signature_secret, self.ec)
            input.signature = signature

        return tx

class VoteBurnProof:

    def __init__(self, value, token_id,
                 value_blind, token_blind, serial,
                 coin_blind, secret, spend_hook, user_data,
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
        self.all_coins = all_coins
        self.signature_secret = signature_secret

        self.ec = ec

    def get_revealed(self):
        revealed = ClassNamespace()
        revealed.nullifier = crypto.ff_hash(self.ec.p, self.secret, self.serial)

        revealed.value_commit = crypto.pedersen_encrypt(
            self.value, self.value_blind, self.ec
        )

        revealed.token_commit = crypto.pedersen_encrypt(
            self.token_id, self.token_blind, self.ec
        )

        # is_valid_merkle_root()
        revealed.all_coins = self.all_coins

        revealed.signature_public = self.ec.multiply(self.signature_secret,
                                                     self.ec.G)

        return revealed

    def verify(self, public):
        revealed = self.get_revealed()

        public_key = self.ec.multiply(self.secret, self.ec.G)
        coin = crypto.ff_hash(
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
        ])

class VoteProof:

    def __init__(self, value, token_id,
                 value_blind, token_blind, vote_blind,
                 vote_option, vote_option_blind, ec):
        self.value = value
        self.token_id = token_id
        self.value_blind = value_blind
        self.token_blind = token_blind
        self.vote_blind = vote_blind
        self.vote_option = vote_option
        self.vote_option_blind = vote_option_blind
        self.ec = ec

    def get_revealed(self):
        revealed = ClassNamespace()
        # Multiply the point by vote_option
        revealed.value_commit = crypto.pedersen_encrypt(
            self.value, self.value_blind, self.ec
        )
        revealed.vote_commit = crypto.pedersen_encrypt(
            self.vote_option * self.value, self.vote_blind, self.ec
        )
        revealed.token_commit = crypto.pedersen_encrypt(
            self.token_id, self.token_blind, self.ec
        )
        #revealed.vote_option_commit = crypto.ff_hash(
        #    self.ec.p, self.vote_option, self.vote_option_blind
        #)
        return revealed

    def verify(self, public):
        revealed = self.get_revealed()
        # vote option should be 0 or 1
        if ((self.vote_option - 0) * (self.vote_option - 1)) % self.ec.p != 0:
            return False
        return all([
            revealed.value_commit == public.value_commit,
            revealed.vote_commit == public.vote_commit,
            revealed.token_commit == public.token_commit,
            #revealed.vote_option_commit == public.vote_option_commit
        ])

class VoteTx:

    def __init__(self, ec):
        self.inputs = []
        self.vote = None

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

        return True, None

    def _check_value_commits(self):
        valcom_total = (0, 1, 0)
        for input in self.inputs:
            value_commit = input.revealed.value_commit
            valcom_total = self.ec.add(valcom_total, value_commit)

        return valcom_total == self.vote.revealed.value_commit

    def _check_proofs(self):
        for input in self.inputs:
            if not input.burn_proof.verify(input.revealed):
                return False
        if not self.vote.proof.verify(self.vote.revealed):
            return False
        return True

    def _verify_token_commitments(self):
        token_commit_value = self.vote.revealed.token_commit
        for input in self.inputs:
            if input.revealed.token_commit != token_commit_value:
                return False
        return True

class DaoBuilder:

    def __init__(self, proposer_limit, quorum, approval_ratio,
                 gov_token_id, dao_public_key, dao_bulla_blind, ec):
        self.proposer_limit = proposer_limit
        self.quorum = quorum
        self.approval_ratio = approval_ratio
        self.gov_token_id = gov_token_id
        self.dao_public_key = dao_public_key
        self.dao_bulla_blind = dao_bulla_blind

        self.ec = ec

    def build(self):
        mint_proof = DaoMintProof(
            self.proposer_limit,
            self.quorum,
            self.approval_ratio,
            self.gov_token_id,
            self.dao_public_key,
            self.dao_bulla_blind,
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

    def __init__(self, proposer_limit, quorum, approval_ratio,
                 gov_token_id, dao_public_key, dao_bulla_blind, ec):
        self.proposer_limit = proposer_limit
        self.quorum = quorum
        self.approval_ratio = approval_ratio
        self.gov_token_id = gov_token_id
        self.dao_public_key = dao_public_key
        self.dao_bulla_blind = dao_bulla_blind
        self.ec = ec

    def get_revealed(self):
        revealed = ClassNamespace()

        revealed.bulla = crypto.ff_hash(
            self.ec.p,
            self.proposer_limit,
            self.quorum,
            self.approval_ratio,
            self.gov_token_id,
            self.dao_public_key[0],
            self.dao_public_key[1],
            self.dao_bulla_blind
        )

        return revealed

    def verify(self, public):
        revealed = self.get_revealed()
        return revealed.bulla == public.bulla

# Shared between DaoMint and DaoExec
class DaoState:

    def __init__(self):
        self.dao_bullas = set()
        self.proposals = set()
        # Closed proposals
        self.proposal_nullifiers = set()

    def is_valid_merkle(self, all_dao_bullas):
        return all_dao_bullas.issubset(self.dao_bullas)

    def is_valid_merkle_proposals(self, all_proposal_bullas):
        return all_proposal_bullas.issubset(self.proposals)

    def proposal_nullifier_exists(self, nullifier):
        return nullifier in self.proposal_nullifiers

    def apply_proposal_tx(self, update):
        self.proposals.add(update.proposal)

    def apply_exec_tx(self, update):
        self.proposal_nullifiers.add(update.proposal_nullifier)

    # Apply DAO mint tx update
    def apply(self, update):
        self.dao_bullas.add(update.bulla)

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

    def __init__(self,
        proposal,
        all_proposals,
        dao,
        win_votes,
        total_votes,
        total_value_blinds,
        total_vote_blinds,
        pay_tx_serial_0,
        pay_tx_serial_1,
        pay_tx_coin_blind_0,
        pay_tx_coin_blind_1,
        pay_tx_input_value,
        pay_tx_input_blinds,
        ec
    ):
        self.proposal = proposal
        self.all_proposals = all_proposals
        self.dao = dao
        self.win_votes = win_votes
        self.total_votes = total_votes
        self.total_value_blinds = total_value_blinds
        self.total_vote_blinds = total_vote_blinds
        self.pay_tx_serial_0 = pay_tx_serial_0
        self.pay_tx_serial_1 = pay_tx_serial_1
        self.pay_tx_coin_blind_0 = pay_tx_coin_blind_0
        self.pay_tx_coin_blind_1 = pay_tx_coin_blind_1
        self.pay_tx_input_value = pay_tx_input_value
        self.pay_tx_input_blinds = pay_tx_input_blinds

        self.ec = ec

    def build(self):
        tx = DaoExecTx()
        tx.proof = DaoExecProof(
            self.proposal,
            self.all_proposals,
            self.dao,
            self.win_votes,
            self.total_votes,
            self.total_value_blinds,
            self.total_vote_blinds,
            self.pay_tx_serial_0,
            self.pay_tx_serial_1,
            self.pay_tx_coin_blind_0,
            self.pay_tx_coin_blind_1,
            self.pay_tx_input_value,
            self.pay_tx_input_blinds,
            self.ec
        )
        tx.revealed = tx.proof.get_revealed()
        return tx

class DaoExecTx:

    def verify(self):
        if not self._check_proofs():
            return False, "proofs failed to verify"

        return True, None

    def _check_proofs(self):
        if not self.proof.verify(self.revealed):
            return False
        return True

class DaoExecProof:

    def __init__(self,
        proposal,
        all_proposals,
        dao,
        win_votes,
        total_votes,
        total_value_blinds,
        total_vote_blinds,
        pay_tx_serial_0,
        pay_tx_serial_1,
        pay_tx_coin_blind_0,
        pay_tx_coin_blind_1,
        pay_tx_input_value,
        pay_tx_input_blinds,
        ec
    ):
        self.proposal = proposal
        self.all_proposals = all_proposals
        self.dao = dao
        self.win_votes = win_votes
        self.total_votes = total_votes
        self.total_value_blinds = total_value_blinds
        self.total_vote_blinds = total_vote_blinds
        self.pay_tx_serial_0 = pay_tx_serial_0
        self.pay_tx_serial_1 = pay_tx_serial_1
        self.pay_tx_coin_blind_0 = pay_tx_coin_blind_0
        self.pay_tx_coin_blind_1 = pay_tx_coin_blind_1
        self.pay_tx_input_value = pay_tx_input_value
        self.pay_tx_input_blinds = pay_tx_input_blinds

        self.ec = ec

    def get_revealed(self):
        revealed = ClassNamespace()
        # Corresponds to proposals merkle root
        revealed.all_proposals = self.all_proposals

        dao_bulla = crypto.ff_hash(
            self.ec.p,
            self.dao.proposer_limit,
            self.dao.quorum,
            self.dao.approval_ratio,
            self.dao.gov_token_id,
            self.dao.public_key[0],
            self.dao.public_key[1],
            self.dao.bulla_blind
        )
        proposal_bulla = crypto.ff_hash(
            self.ec.p,
            self.proposal.dest[0],
            self.proposal.dest[1],
            self.proposal.amount,
            self.proposal.serial,
            self.proposal.token_id,
            self.proposal.blind,
            dao_bulla
        )
        revealed.proposal_nullifier = crypto.ff_hash(
            self.ec.p, self.proposal.serial)

        revealed.coin_0 = crypto.ff_hash(
            self.ec.p,
            self.proposal.dest[0],
            self.proposal.dest[1],
            self.proposal.amount,
            self.proposal.token_id,
            self.pay_tx_serial_0,
            self.pay_tx_coin_blind_0,
            b"0x0000",
            b"0x0000"
        )

        change_amount = self.pay_tx_input_value - self.proposal.amount
        assert change_amount > 0

        # Need the same DAO public key
        # Need the input amount for pay_tx for treasury
        # Need user_data blind
        revealed.coin_1 = crypto.ff_hash(
            self.ec.p,
            self.dao.public_key[0],
            self.dao.public_key[1],
            change_amount,
            self.proposal.token_id,
            self.pay_tx_serial_1,
            self.pay_tx_coin_blind_1,
            b"0xdao_ruleset",
            dao_bulla
        )

        # Money that went into the pay tx
        revealed.inputs_value_commit = crypto.pedersen_encrypt(
            self.pay_tx_input_value, self.pay_tx_input_blinds, self.ec)

        revealed.total_value_commit = crypto.pedersen_encrypt(
            self.total_votes, self.total_value_blinds, self.ec)
        revealed.total_vote_commit = crypto.pedersen_encrypt(
            self.win_votes, self.total_vote_blinds, self.ec)

        return revealed

    def verify(self, public):
        revealed = self.get_revealed()

        # Check proposal exists
        dao_bulla = crypto.ff_hash(
            self.ec.p,
            self.dao.proposer_limit,
            self.dao.quorum,
            self.dao.approval_ratio,
            self.dao.gov_token_id,
            self.dao.public_key[0],
            self.dao.public_key[1],
            self.dao.bulla_blind
        )
        proposal_bulla = crypto.ff_hash(
            self.ec.p,
            self.proposal.dest[0],
            self.proposal.dest[1],
            self.proposal.amount,
            self.proposal.serial,
            self.proposal.token_id,
            self.proposal.blind,
            dao_bulla
        )
        # This being true also implies the DAO is valid
        assert proposal_bulla in self.all_proposals

        assert self.total_votes >= self.dao.quorum

        # Approval ratio should be actually 2 values ffs
        #assert self.win_votes / self.total_votes >= self.dao.approval_ratio
        assert self.win_votes >= self.dao.approval_ratio * self.total_votes

        return all([
            revealed.all_proposals == public.all_proposals,
            revealed.proposal_nullifier == public.proposal_nullifier,
            revealed.coin_0 == public.coin_0,
            revealed.coin_1 == public.coin_1,
            revealed.inputs_value_commit == public.inputs_value_commit,
            revealed.total_value_commit == public.total_value_commit,
            revealed.total_vote_commit == public.total_vote_commit,
        ])

def dao_exec_state_transition(state, tx, pay_tx, ec):
    is_verify, reason = tx.verify()
    if not is_verify:
        print(f"dao exec tx verify failed: {reason}", file=sys.stderr)
        return None

    if not state.is_valid_merkle_proposals(tx.revealed.all_proposals):
        print(f"invalid merkle root proposals", file=sys.stderr)
        return None

    nullifier = tx.revealed.proposal_nullifier
    if state.proposal_nullifier_exists(nullifier):
        print(f"duplicate nullifier found", file=sys.stderr)
        return None

    # Check the structure of the payment tx is correct
    if len(pay_tx.outputs) != 2:
        print(f"only 2 outputs allowed", file=sys.stderr)
        return None
    if tx.revealed.coin_0 != pay_tx.outputs[0].revealed.coin:
        print(f"coin0 incorrectly formed", file=sys.stderr)
        return None

    inputs_value_commit = (0, 1, 0)
    for input in pay_tx.inputs:
        value_commit = input.revealed.value_commit
        inputs_value_commit = ec.add(inputs_value_commit, value_commit)
    if inputs_value_commit != tx.revealed.inputs_value_commit:
        print(f"value commitment for inputs doesn't match", file=sys.stderr)
        return None

    if tx.revealed.coin_1 != pay_tx.outputs[1].revealed.coin:
        print(f"coin1 incorrectly formed", file=sys.stderr)
        return None

    update = ClassNamespace()
    update.proposal_nullifier = tx.revealed.proposal_nullifier
    return update

# contract interface functions
def proposal_state_transition(dao_state, gov_state, tx):
    is_verify, reason = tx.verify()
    if not is_verify:
        print(f"dao tx verify failed: {reason}", file=sys.stderr)
        return None

    if not dao_state.is_valid_merkle(tx.dao.revealed.all_dao_bullas):
        print(f"invalid merkle root dao", file=sys.stderr)
        return None

    for input in tx.inputs:
        if not gov_state.is_valid_merkle(input.revealed.all_coins):
            print(f"invalid merkle root", file=sys.stderr)
            return None

    update = ClassNamespace()
    update.proposal = tx.dao.revealed.proposal_bulla
    return update

class VoteState:

    def __init__(self):
        self.votes = set()
        self.nullifiers = set()

    def nullifier_exists(self, nullifier):
        return nullifier in self.nullifiers

    def apply(self, update):
        self.nullifiers = self.nullifiers.union(update.nullifiers)
        self.votes.add(update.vote)

def vote_state_transition(vote_state, gov_state, tx):
    for input in tx.inputs:
        if not gov_state.is_valid_merkle(input.revealed.all_coins):
            print(f"invalid merkle root", file=sys.stderr)
            return None

        nullifier = input.revealed.nullifier
        if gov_state.nullifier_exists(nullifier):
            print(f"duplicate nullifier found", file=sys.stderr)
            return None

        if vote_state.nullifier_exists(nullifier):
            print(f"duplicate nullifier found (already voted)", file=sys.stderr)
            return None

    is_verify, reason = tx.verify()
    if not is_verify:
        print(f"dao tx verify failed: {reason}", file=sys.stderr)
        return None

    update = ClassNamespace()
    update.nullifiers = [input.revealed.nullifier for input in tx.inputs]
    update.vote = tx.vote.revealed.value_commit
    return update

def main(argv):
    ec = crypto.pallas_curve()

    money_state = MoneyState()
    gov_state = MoneyState()
    dao_state = DaoState()

    # Money parameters
    money_initial_supply = 21000
    money_token_id = 110

    # Governance token parameters
    gov_initial_supply = 10000
    gov_token_id = 4

    # DAO parameters
    dao_proposer_limit = 110
    dao_quorum = 110
    dao_approval_ratio = 2

    ################################################
    # Create the DAO bulla
    ################################################
    # Setup the DAO
    dao_shared_secret = ec.random_scalar()
    dao_public_key = ec.multiply(dao_shared_secret, ec.G)

    dao_bulla_blind = ec.random_base()

    builder = DaoBuilder(
        dao_proposer_limit,
        dao_quorum,
        dao_approval_ratio,
        gov_token_id,
        dao_public_key,
        dao_bulla_blind,
        ec
    )
    tx = builder.build()

    # Each deployment of a contract has a unique state
    # associated with it.
    if (update := dao_state_transition(dao_state, tx)) is None:
        return -1
    dao_state.apply(update)

    dao_bulla = tx.revealed.bulla

    ################################################
    # Mint the initial supply of treasury token
    # and send it all to the DAO directly
    ################################################

    # Only used for this tx. Discarded after
    signature_secret = ec.random_scalar()

    builder = money.SendPaymentTxBuilder(ec)
    builder.add_clear_input(money_initial_supply, money_token_id,
                            signature_secret)
    # Address of deployed contract in our example is 0xdao_ruleset
    # This field is public, you can see it's being sent to a DAO
    # but nothing else is visible.
    spend_hook = b"0xdao_ruleset"
    # This can be a simple hash of the items passed into the ZK proof
    # up to corresponding linked ZK proof to interpret however they need.
    # In out case, it's the bulla for the DAO
    user_data = dao_bulla
    builder.add_output(money_initial_supply, money_token_id, dao_public_key,
                       spend_hook, user_data)
    tx = builder.build()

    # This state_transition function is the ruleset for anon payments
    if (update := money_state_transition(money_state, tx)) is None:
        return -1
    money_state.apply(update)

    # NOTE: maybe we want to add additional zk proof here that the tx
    #       sending money to the DAO was constructed correctly.
    #       For example that the user_data is set correctly

    # payment state transition in coin specifies dependency
    # the tx exists and ruleset is applied

    assert len(tx.outputs) > 0
    coin_note = tx.outputs[0].enc_note
    coin = crypto.ff_hash(
        ec.p,
        dao_public_key[0],
        dao_public_key[1],
        coin_note.value,
        coin_note.token_id,
        coin_note.serial,
        coin_note.coin_blind,
        spend_hook,
        user_data
    )
    assert coin == tx.outputs[0].mint_proof.get_revealed().coin

    for coin, enc_note in zip(update.coins, update.enc_notes):
        # Try decrypt note here
        print(f"Received {enc_note.value} DRK")

    ################################################
    # Mint the governance token
    # Send it to three hodlers
    ################################################

    # Hodler 1
    gov_secret_1 = ec.random_scalar()
    gov_public_1 = ec.multiply(gov_secret_1, ec.G)
    # Hodler 2
    gov_secret_2 = ec.random_scalar()
    gov_public_2 = ec.multiply(gov_secret_2, ec.G)
    # Hodler 3: the tiebreaker
    gov_secret_3 = ec.random_scalar()
    gov_public_3 = ec.multiply(gov_secret_3, ec.G)

    # Only used for this tx. Discarded after
    signature_secret = ec.random_scalar()

    builder = money.SendPaymentTxBuilder(ec)
    builder.add_clear_input(gov_initial_supply, gov_token_id,
                            signature_secret)
    assert 2 * 4000 + 2000 == gov_initial_supply
    builder.add_output(4000, gov_token_id, gov_public_1,
                       b"0x0000", b"0x0000")
    builder.add_output(4000, gov_token_id, gov_public_2,
                       b"0x0000", b"0x0000")
    builder.add_output(2000, gov_token_id, gov_public_3,
                       b"0x0000", b"0x0000")
    tx = builder.build()

    # This state_transition function is the ruleset for anon payments
    if (update := money_state_transition(gov_state, tx)) is None:
        return -1
    gov_state.apply(update)

    # Decrypt output notes
    assert len(tx.outputs) == 3
    gov_user_1_note = tx.outputs[0].enc_note
    gov_user_2_note = tx.outputs[1].enc_note
    gov_user_3_note = tx.outputs[2].enc_note

    for coin, enc_note in zip(update.coins, update.enc_notes):
        # Try decrypt note here
        print(f"Received {enc_note.value} GOV")

    ################################################
    # DAO rules:
    # 1. gov token IDs must match on all inputs
    # 2. proposals must be submitted by minimum amount
    #       - need protection so can't collude? must be a single signer??
    #         - stellar: doesn't have to be robust for this MVP
    # 3. number of votes >= quorum
    #       - just positive votes or all votes?
    #         - stellar: no that's all votes
    # 4. outcome > approval_ratio
    # 5. structure of outputs
    #   output 0: value and address
    #   output 1: change address
    ################################################

    ################################################
    # Propose the vote
    # In order to make a valid vote, first the proposer must
    # meet a criteria for a minimum number of gov tokens
    ################################################

    user_secret = ec.random_scalar()
    user_public = ec.multiply(user_secret, ec.G)

    # There is a struct that corresponds to the configuration of this
    # particular vote.
    # For MVP, just use a single-option list of [destination, amount]
    # Send user 1000 DRK
    proposal = ClassNamespace()
    proposal.dest = user_public
    proposal.amount = 1000
    # Used to produce the nullifier when the vote is executed
    proposal.serial = ec.random_base()
    proposal.token_id = money_token_id
    proposal.blind = ec.random_base()

    # For vote to become valid, the proposer must prove
    # that they own more than proposer_limit number of gov tokens.

    dao = ClassNamespace()
    dao.proposer_limit = dao_proposer_limit
    dao.quorum = dao_quorum
    dao.approval_ratio = dao_approval_ratio
    dao.gov_token_id = gov_token_id
    dao.public_key = dao_public_key
    dao.bulla_blind = dao_bulla_blind

    builder = ProposerTxBuilder(proposal, dao_state.dao_bullas, ec)
    witness = gov_state.all_coins
    builder.add_input(witness, gov_secret_1, gov_user_1_note)
    builder.set_dao(dao)
    tx = builder.build()

    # No state changes actually happen so ignore the update
    # We just verify the tx is correct basically.
    if (update := proposal_state_transition(dao_state, gov_state, tx)) is None:
        return -1
    dao_state.apply_proposal_tx(update)

    ################################################
    # Proposal is accepted!
    # Start the voting
    ################################################

    # Lets the voting begin
    # Voters have access to the proposal and dao data
    vote_state = VoteState()
    # We don't need to copy nullifier set because it is checked from gov_state
    # in vote_state_transition() anyway

    # TODO: what happens if voters don't unblind their vote
    # Answer:
    #   1. there is a time limit
    #   2. both the MPC or users can unblind

    # TODO: bug if I vote then send money, then we can double vote
    # TODO: all timestamps missing
    #       - timelock (future voting starts in 2 days)
    # Fix: use nullifiers from money gov state only from
    # beginning of gov period
    # Cannot use nullifiers from before voting period

    # User 1: YES
    builder = VoteTxBuilder(ec)
    builder.add_input(witness, gov_secret_1, gov_user_1_note)
    builder.set_vote_option(1)
    tx1 = builder.build()

    if (update := vote_state_transition(vote_state, gov_state, tx1)) is None:
        return -1
    vote_state.apply(update)

    note_vote_1 = tx1.note

    # User 2: NO
    builder = VoteTxBuilder(ec)
    builder.add_input(witness, gov_secret_2, gov_user_2_note)
    builder.set_vote_option(0)
    tx2 = builder.build()

    if (update := vote_state_transition(vote_state, gov_state, tx2)) is None:
        return -1
    vote_state.apply(update)

    note_vote_2 = tx2.note

    # User 3: YES
    builder = VoteTxBuilder(ec)
    builder.add_input(witness, gov_secret_3, gov_user_3_note)
    builder.set_vote_option(1)
    tx3 = builder.build()

    if (update := vote_state_transition(vote_state, gov_state, tx3)) is None:
        return -1
    vote_state.apply(update)

    note_vote_3 = tx3.note

    # State
    # functions that can be called on state with params
    # functions return an update
    # optional encrypted values that can be read by wallets
    # --> (do this outside??)
    # --> penalized if fail
    # apply update to state

    # Every votes produces a semi-homomorphic encryption of their vote.
    # Which is either yes or no
    # We copy the state tree for the governance token so coins can be used
    # to vote on other proposals at the same time.
    # With their vote, they produce a ZK proof + nullifier
    # The votes are unblinded by MPC to a selected party at the end of the
    # voting period.
    # (that's if we want votes to be hidden during voting)

    win_votes = 0
    total_votes = 0
    total_vote_blinds = 0
    total_value_blinds = 0
    total_value_commit = (0, 1, 0)
    total_vote_commit = (0, 1, 0)
    for i, (note, tx) in enumerate(
        zip([note_vote_1, note_vote_2, note_vote_3], [tx1, tx2, tx3])):

        assert note.token_id == gov_token_id
        token_commit = crypto.pedersen_encrypt(
            gov_token_id, note.token_blind, ec)
        assert tx.vote.revealed.token_commit == token_commit

        #vote_option_commit = crypto.ff_hash(
        #    ec.p, note.vote_option, note.vote_option_blind)
        #assert tx.vote.revealed.vote_option_commit == vote_option_commit

        value_commit = crypto.pedersen_encrypt(
            note.value, note.value_blind, ec)
        assert tx.vote.revealed.value_commit == value_commit
        total_value_commit = ec.add(total_value_commit, value_commit)
        total_value_blinds += note.value_blind
        
        vote_commit = crypto.pedersen_encrypt(
            note.vote_option * note.value, note.vote_blind, ec)
        assert tx.vote.revealed.vote_commit == vote_commit
        total_vote_commit = ec.add(total_vote_commit, vote_commit)
        total_vote_blinds += note.vote_blind

        vote_option = note.vote_option
        assert vote_option == 0 or vote_option == 1

        if vote_option == 1:
            win_votes += note.value

        total_votes += note.value

        if vote_option == 1:
            vote_result = "yes"
        else:
            vote_result = "no"
        print(f"Voter {i} voted {vote_result}")

    print(f"Outcome = {win_votes} / {total_votes}")

    assert total_value_commit == crypto.pedersen_encrypt(
        total_votes, total_value_blinds, ec)
    assert total_vote_commit == crypto.pedersen_encrypt(
        win_votes, total_vote_blinds, ec)

    ################################################
    # Execute the vote
    ################################################

    # Used to export user_data from this coin so it can be accessed
    # by 0xdao_ruleset
    user_data_blind = ec.random_base()

    builder = money.SendPaymentTxBuilder(ec)
    witness = money_state.all_coins
    builder.add_input(witness, dao_shared_secret, coin_note, user_data_blind)

    builder.add_output(1000, money_token_id, user_public,
                       spend_hook=b"0x0000", user_data=b"0x0000")
    # Change
    builder.add_output(coin_note.value - 1000, money_token_id, dao_public_key,
                       spend_hook, user_data)

    tx = builder.build()

    if (update := money_state_transition(money_state, tx)) is None:
        return -1
    money_state.apply(update)

    # Now the spend_hook field specifies the function DaoExec
    # so the tx above must also be combined with a DaoExec tx
    assert len(tx.inputs) == 1
    # At least one input has this field value which means the 0xdao_ruleset
    # is invoked.
    input = tx.inputs[0]
    assert input.revealed.spend_hook == b"0xdao_ruleset"
    assert (input.revealed.enc_user_data ==
        crypto.ff_hash(
            ec.p,
            user_data,
            user_data_blind
        ))
    # Verifier cannot see DAO bulla
    # They see the enc_user_data which is also in the DAO exec contract
    assert user_data == crypto.ff_hash(
        ec.p,
        dao_proposer_limit,
        dao_quorum,
        dao_approval_ratio,
        gov_token_id,
        dao_public_key[0],
        dao_public_key[1],
        dao_bulla_blind
    ) # DAO bulla

    pay_tx = tx

    # execution proof
    # 1. total votes >= quorum
    # 2. win_votes / total_votes >= approval_ratio
    # 3. structure of outputs
    #   output 0: value and address
    #   output 1: change address

    # - check proposal exists
    # - create proposal nullifier
    #     - verifier: check it doesn't already exist
    # - check dest, amount, token_id match
    #     - export both output value_commits
    #     - export token_id commit used in send_payment tx
    #     - export output 0 and 1 dest
    #     - check all these fields match the tx
    # - is linked to DAO
    #     - read DAO params
    #     - re-export as enc_user_data
    #     - verifier: check it matches the tx
    # - total_votes >= quorum
    #     - verifier: check sum of vote_commits is correct
    # - win_votes / total_votes >= approval_ratio

    assert len(pay_tx.outputs) == 2
    pay_tx_serial_0 = pay_tx.outputs[0].enc_note.serial
    pay_tx_serial_1 = pay_tx.outputs[1].enc_note.serial
    pay_tx_coin_blind_0 = pay_tx.outputs[0].enc_note.coin_blind
    pay_tx_coin_blind_1 = pay_tx.outputs[1].enc_note.coin_blind
    pay_tx_input_value = coin_note.value
    pay_tx_input_blinds = sum(builder.input_blinds) % ec.order

    builder = DaoExecBuilder(
        proposal,
        dao_state.proposals,
        dao,
        win_votes,
        total_votes,
        total_value_blinds,
        total_vote_blinds,
        pay_tx_serial_0,
        pay_tx_serial_1,
        pay_tx_coin_blind_0,
        pay_tx_coin_blind_1,
        pay_tx_input_value,
        pay_tx_input_blinds,
        ec
    )
    tx = builder.build()
    if (update := dao_exec_state_transition(dao_state, tx, pay_tx, ec)) is None:
        return -1
    dao_state.apply_exec_tx(update)

    # These checks are also run by the verifier
    assert tx.revealed.total_value_commit == total_value_commit
    assert tx.revealed.total_vote_commit == total_vote_commit

    return 0

if __name__ == "__main__":
    sys.exit(main(sys.argv))

