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

    def __init__(self, all_dao_bullas, ec):
        self.inputs = []
        self.all_dao_bullas = all_dao_bullas

        self.ec = ec

    def add_input(self, all_coins, secret, note):
        input = ClassNamespace()
        input.all_coins = all_coins
        input.secret = secret
        input.note = note
        self.inputs.append(input)

    def set_dao(self, proposer_limit, quorum, approval_ratio,
                gov_token_id, dao_bulla_blind):
        self.dao_proposer_limit = proposer_limit
        self.dao_quorum = quorum
        self.dao_approval_ratio = approval_ratio
        self.gov_token_id = gov_token_id
        self.dao_bulla_blind = dao_bulla_blind

    def build(self):
        tx = ProposerTx(self.ec)
        token_blind = self.ec.random_scalar()
        proposer_limit_blind = self.ec.random_base()
        enc_bulla_blind = self.ec.random_base()

        total_value = sum(input.note.value for input in self.inputs)
        input_value_blinds = [self.ec.random_scalar() for _ in self.inputs]
        total_value_blinds = sum(input_value_blinds)

        tx.dao = ClassNamespace()
        tx.dao.__name__ = "ProposerTxDao"
        # We export proposer_limit as an encrypted value from the DAO
        tx.dao.proof = ProposerTxDaoProof(
            total_value, total_value_blinds,
            self.dao_proposer_limit, self.dao_quorum, self.dao_approval_ratio,
            self.gov_token_id, self.dao_bulla_blind,
            token_blind, proposer_limit_blind, enc_bulla_blind,
            self.all_dao_bullas, self.ec
        )
        tx.dao.revealed = tx.dao.proof.get_revealed()

        # Members of the DAO need to themselves verify this is the correct
        # bulla they are voting on, so we encrypt the blind to them
        tx.note = ClassNamespace()
        tx.note.enc_bulla_blind = enc_bulla_blind

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

        # TODO: sign tx
        # TODO: continue adding logic to tx.verify

        return tx

class ProposerTx:

    def __init__(self, ec):
        self.inputs = []
        self.dao = None
        self.note = None

        self.ec = ec

    def verify(self):
        if not self._check_value_commits():
            return False, "value commits do not match"

        return True, None

    def _check_value_commits(self):
        valcom_total = (0, 1, 0)

        for input in self.inputs:
            value_commit = input.revealed.value_commit
            valcom_total = self.ec.add(valcom_total, value_commit)

        return valcom_total == self.dao.revealed.value_commit

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
            revealed.value_commit == public.value_commit,
            revealed.token_commit == public.token_commit,
            revealed.all_coins == public.all_coins,
            revealed.signature_public == public.signature_public
        ])

class ProposerTxDaoProof:

    def __init__(self, total_value, total_value_blinds,
                 proposer_limit, quorum, approval_ratio,
                 gov_token_id, dao_bulla_blind,
                 token_blind, proposer_limit_blind, enc_bulla_blind,
                 all_dao_bullas, ec):
        self.total_value = total_value
        self.total_value_blinds = total_value_blinds
        self.proposer_limit = proposer_limit
        self.quorum = quorum
        self.approval_ratio = approval_ratio
        self.gov_token_id = gov_token_id
        self.dao_bulla_blind = dao_bulla_blind
        self.token_blind = token_blind
        self.proposer_limit_blind = proposer_limit_blind
        self.enc_bulla_blind = enc_bulla_blind
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
            self.dao_bulla_blind
        )
        revealed.enc_bulla = crypto.ff_hash(self.ec.p, bulla, self.enc_bulla_blind)
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
            self.dao_bulla_blind
        )
        # Merkle root check
        if bulla not in self.all_dao_bullas:
            return False

        #
        #   total_value >= proposer_limit
        #
        if not total_value >= self.proposer_limit:
            return False

        return all([
            revealed.value_commit == public.value_commit,
            revealed.token_commit == public.token_commit,
            revealed.enc_bulla == public.enc_bulla,
            revealed.all_dao_bullas == public.all_dao_bullas
        ])

# contract interface functions
def proposer_state_transition(state, tx):
    is_verify, reason = tx.verify()
    if not is_verify:
        print(f"dao tx verify failed: {reason}", file=sys.stderr)
        return None

    update = ClassNamespace()
    return update

class DaoBuilder:

    def __init__(self, proposer_limit, quorum, approval_ratio,
                 gov_token_id, dao_bulla_blind, ec):
        self.proposer_limit = proposer_limit
        self.quorum = quorum
        self.approval_ratio = approval_ratio
        self.gov_token_id = gov_token_id
        self.dao_bulla_blind = dao_bulla_blind

        self.ec = ec

    def build(self):
        mint_proof = DaoMintProof(
            self.proposer_limit,
            self.quorum,
            self.approval_ratio,
            self.gov_token_id,
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
                 gov_token_id, dao_bulla_blind, ec):
        self.proposer_limit = proposer_limit
        self.quorum = quorum
        self.approval_ratio = approval_ratio
        self.gov_token_id = gov_token_id
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
            self.dao_bulla_blind
        )

        return revealed

    def verify(self, public):
        revealed = self.get_revealed()
        return revealed.bulla == public.bulla

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

    # payment state transition in coin specifies dependency
    # the tx exists and ruleset is applied

    assert len(tx.outputs) > 0
    note = tx.outputs[0].enc_note
    coin = crypto.ff_hash(
        ec.p,
        dao_public_key[0],
        dao_public_key[1],
        note.value,
        note.token_id,
        note.serial,
        note.coin_blind,
        spend_hook,
        user_data
    )
    assert coin == tx.outputs[0].mint_proof.get_revealed().coin

    for coin, enc_note in zip(update.coins, update.enc_notes):
        # Try decrypt note here
        print(f"Received {enc_note.value} DRK")

    ################################################
    # Mint the governance token
    # Send it to two hodlers
    ################################################

    # Hodler 1
    gov_secret_1 = ec.random_scalar()
    gov_public_1 = ec.multiply(gov_secret_1, ec.G)
    # Hodler 2
    gov_secret_2 = ec.random_scalar()
    gov_public_2 = ec.multiply(gov_secret_2, ec.G)

    # Only used for this tx. Discarded after
    signature_secret = ec.random_scalar()

    builder = money.SendPaymentTxBuilder(ec)
    builder.add_clear_input(gov_initial_supply, gov_token_id,
                            signature_secret)
    assert 2 * 5000 == gov_initial_supply
    builder.add_output(5000, gov_token_id, gov_public_1,
                       b"0x0000", b"0x0000")
    builder.add_output(5000, gov_token_id, gov_public_1,
                       b"0x0000", b"0x0000")
    tx = builder.build()

    # This state_transition function is the ruleset for anon payments
    if (update := money_state_transition(gov_state, tx)) is None:
        return -1
    gov_state.apply(update)

    # Decrypt output notes
    assert len(tx.outputs) == 2
    gov_user_1_note = tx.outputs[0].enc_note
    gov_user_2_note = tx.outputs[1].enc_note

    for coin, enc_note in zip(update.coins, update.enc_notes):
        # Try decrypt note here
        print(f"Received {enc_note.value} GOV")

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
    proposal.blind = ec.random_base()

    # For vote to become valid, the proposer must prove
    # that they own more than proposer_limit number of gov tokens.
    enc_proposal = crypto.ff_hash(
        ec.p,
        proposal.dest[0],
        proposal.dest[1],
        proposal.amount,
        proposal.blind
    )

    builder = ProposerTxBuilder(dao_state.bullas, ec)
    witness = gov_state.all_coins
    builder.add_input(witness, gov_secret_1, gov_user_1_note)
    builder.set_dao(
        dao_proposer_limit,
        dao_quorum,
        dao_approval_ratio,
        gov_token_id,
        dao_bulla_blind
    )
    tx = builder.build()

    # No state changes actually happen so ignore the update
    if (_ := proposer_state_transition(gov_state, tx)) is None:
        return -1

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

    votes_yes = 10
    votes_no = 5

    ################################################
    # Execute the vote
    ################################################

    # Used to export user_data from this coin so it can be accessed
    # by 0xdao_ruleset
    user_data_blind = ec.random_base()

    builder = money.SendPaymentTxBuilder(ec)
    witness = money_state.all_coins
    builder.add_input(witness, dao_shared_secret, note, user_data_blind)

    builder.add_output(1000, money_token_id, user_public,
                       spend_hook=b"0x0000", user_data=b"0x0000")
    # Change
    builder.add_output(note.value - 1000, money_token_id, dao_public_key,
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
        dao_bulla_blind
    ) # DAO bulla

    # proposer proof

    # Now enforce DAO rules:
    # 1. gov token IDs must match on all inputs
    # 2. proposals must be submitted by minimum amount
    #       - need protection so can't collude? must be a single signer??
    #         - stellar: doesn't have to be robust for this MVP
    # 4. number of votes >= quorum
    #       - just positive votes or all votes?
    #         - stellar: no that's all votes
    # 4. outcome > approval_ratio
    # 5. structure of outputs
    #   output 0: value and address
    #   output 1: change address
    builder = DaoExecBuilder()
    tx = builder.build()
    if (update := dao_exec_state_transition(dao_state, tx)) is None:
        return -1
    dao_state.apply_exec(update)

    return 0

if __name__ == "__main__":
    sys.exit(main(sys.argv))

