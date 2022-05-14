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

class DaoBuilder:

    def __init__(self, proposer_limit, quorum, approval_ratio, ec):
        self.proposer_limit = proposer_limit
        self.quorum = quorum
        self.approval_ratio = approval_ratio

        self.ec = ec

    def build(self):
        mint_proof = DaoMintProof(
            self.proposer_limit,
            self.quorum,
            self.approval_ratio,
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

    def __init__(self, proposer_limit, quorum, approval_ratio, ec):
        self.proposer_limit = proposer_limit
        self.quorum = quorum
        self.approval_ratio = approval_ratio
        self.ec = ec

    def get_revealed(self):
        revealed = ClassNamespace()

        revealed.bulla = crypto.ff_hash(
            self.ec.p,
            self.proposer_limit,
            self.quorum,
            self.approval_ratio
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

    builder = DaoBuilder(
        dao_proposer_limit,
        dao_quorum,
        dao_approval_ratio,
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
        dao_approval_ratio
    ) # DAO bulla

    # proposer proof

    # Now enforce DAO rules:
    # 1. proposals must be submitted by minimum amount
    #       - need protection so can't collude? must be a single signer??
    #         - stellar: doesn't have to be robust for this MVP
    # 2. number of votes >= quorum
    #       - just positive votes or all votes?
    #         - stellar: no that's all votes
    # 3. outcome > approval_ratio
    # 3. structure of outputs
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

