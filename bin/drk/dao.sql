-- # DAO::mint()
-- 
-- First one person will create the DAO
--
--   $ drk dao create_dao \
--       PROPOSER_LIMIT \
--       QUORUM \
--       APPROVAL_RATIO_BASE \
--       APPROVAL_RATIO_QUOTIENT \
--       GOV_TOKEN_ID > dao.dat
--
-- dat.dat contains:
--
-- * DAO parameters as listed above
-- * secret key for the DAO
-- * bulla blind
--
-- We can view the data like so:
--
--   $ drk dao view_dao < dao.dat
--
-- Now everyone inside the DAO exchanges dao.dat out of band will import
-- it into their wallets.
--
--   $ drk dao import_dao DAO_NAME < dao.dat
--   Imported DAO ccb8XXX8af6
--
-- Where ccb8XXX8af6 is the DAO's bulla.
--
-- Next one person will mint it on chain
--
--   $ drk dao mint DAO_NAME
--   Broadcasted DAO ccb8XXX8af6
--
-- And then the others will receive confirmation that the DAO they imported
-- into their wallet has also been accepted on chain.

-- # Minting and Receiving Coin
--
-- Assume that the governance tokens have been created and distributed
-- appropriately among DAO members. We will skip that part here.
--
-- Now the DAO can receive coins into its treasury. These coins simply
-- have both the coin's spend_hook and user_data fields set correctly
-- otherwise they are rejected as invalid/malformed.

-- # DAO::propose()
--
-- Create a proposal for the DAO
--
--   $ drk dao propose \
--       DAO_NAME \
--       RECV_PUBKEY \
--       AMOUNT \
--       SERIAL \
--       SENDCOIN_TOKEN_ID
--
-- If we don't have enough tokens to meet the proposer_limit threshold
-- then this call will simply fail with an error message. Nothing will
-- be added to the database or sent to the network.
--
-- The other participants should automatically receive the proposal
-- ready to vote on it.

-- # DAO::vote()
--
-- You have received a proposal which is active. You can now vote on it.
--
--   $ drk dao proposals DAO_NAME
--   [0] f6cae63ced53d02b372206a8d3ed5ac03fde18da306a520285fd56e8d031f6cf
--   [1] 1372622f4a38be6eb1c90fa67864474c6603d9f8d4228106e20e2d0d04f2395e
--   [2] 88b18cbc38dbd3af8d25237af3903e985f70ea06d1e25966bf98e3f08e23c992
--
--   $ drk dao show_proposal 1
--   Proposal: 1372622f4a38be6eb1c90fa67864474c6603d9f8d4228106e20e2d0d04f2395e
--     destination: ...
--     amount: ...
--     token_id: ...
--     dao_name: DAO_NAME
--     dao_bulla: ...
--   Current yes votes: X
--   Current no votes: Y
--   Total votes: X + Y
--   [================                    ] 45.56%
--   DAO quorum threshold: Z
--   DAO approval ratio: 60%
--
--   $ drk dao vote 1 yes

-- # DAO::exec()
--
-- Once there are enough yes votes to satisfy the quorum and approval ratio,
-- then any DAO member can execute the payment out of the treasury.
--
--   $ drk dao exec 1
--
-- FUTURE NOTE: We have to assume that 2 people could try to exec at once.
--              So if our exec fails, then the tx fee we pay should be returned
--              to our wallet.
--              We should design our tables for exec accordingly.
--              For now I didn't put anything, but we should keep this minor
--              point in mind and ruminate on it for later.

PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS Fd8kfCuqU8BoFFp6GcXv5pC8XXRkBK7gUPQX5XDz7iXj_dao_daos (
	dao_id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    name BLOB UNIQUE NOT NULL,
    proposer_limit BLOB NOT NULL,
    -- minimum threshold for total number of votes for proposal to pass.
    -- If there's too little activity then it cannot pass.
    quorum BLOB NOT NULL,
    -- Needed ratio of yes/total for proposal to pass.
    -- approval_ratio = approval_ratio_quot / approval_ratio_base
    approval_ratio_base INTEGER NOT NULL,
    approval_ratio_quot INTEGER NOT NULL,
	gov_token_id BLOB NOT NULL,
	secret BLOB NOT NULL,
	bulla_blind BLOB NOT NULL,
    -- these values are NULL until the DAO is minted on chain and received
	leaf_position BLOB,
    tx_hash BLOB,
    call_index INTEGER
);

-- The merkle tree containing DAO bullas
CREATE TABLE IF NOT EXISTS Fd8kfCuqU8BoFFp6GcXv5pC8XXRkBK7gUPQX5XDz7iXj_dao_trees (
	daos_tree BLOB NOT NULL,
	proposals_tree BLOB NOT NULL
);

CREATE TABLE IF NOT EXISTS Fd8kfCuqU8BoFFp6GcXv5pC8XXRkBK7gUPQX5XDz7iXj_dao_proposals (
    proposal_id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    dao_id INTEGER NOT NULL,
    -- Public key of person that would receive the funds
    recv_public BLOB NOT NULL,
    -- Amount of funds that would be sent
    amount BLOB NOT NULL,
    -- Token ID we propose to send
    sendcoin_token_id BLOB NOT NULL,
    bulla_blind BLOB NOT NULL,
    -- these values are NULL until the proposal is minted on chain
    -- and received by the DAO
	leaf_position BLOB,
	money_snapshot_tree BLOB,
    tx_hash BLOB,
    call_index INTEGER,
    -- this is NULL until we have voted on this proposal
    our_vote_id INTEGER UNIQUE,

    FOREIGN KEY(our_vote_id) REFERENCES dao_votes(vote_id) ON DELETE CASCADE ON UPDATE CASCADE,
    FOREIGN KEY(dao_id) REFERENCES dao_daos(dao_id) ON DELETE CASCADE ON UPDATE CASCADE
);

CREATE TABLE IF NOT EXISTS Fd8kfCuqU8BoFFp6GcXv5pC8XXRkBK7gUPQX5XDz7iXj_dao_votes (
    vote_id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    proposal_id INTEGER NOT NULL,
    vote_option INTEGER NOT NULL,
    yes_vote_blind BLOB NOT NULL,
    all_vote_value BLOB NOT NULL,
    all_vote_blind BLOB NOT NULL,
    -- these values are NULL until the vote is minted on chain
    -- and received by the DAO
    tx_hash BLOB,
    call_index INTEGER,
    -- My code has votes merkle tree and position for votes, but
    -- that might be a mistake...
    FOREIGN KEY(proposal_id) REFERENCES dao_proposals(proposal_id) ON DELETE CASCADE ON UPDATE CASCADE
);
