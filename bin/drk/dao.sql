-- TODO: Update these once finished
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
-- Where ccb8XXX8af6 is the DAO's name.
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
    -- Bulla identifier of the DAO
    bulla BLOB PRIMARY KEY NOT NULL,
    -- Unique name identifier of the DAO
    name TEXT UNIQUE NOT NULL,
    -- DAO parameters
    params BLOB NOT NULL,
    -- These values are NULL until the DAO is minted on chain and received
    -- Leaf position of the DAO in the Merkle tree of DAOs
    leaf_position BLOB,
    -- The transaction hash where the DAO was deployed
    tx_hash BLOB,
    -- The call index in the transaction where the DAO was deployed
    call_index INTEGER
);

-- The merkle tree containing DAO bullas
CREATE TABLE IF NOT EXISTS Fd8kfCuqU8BoFFp6GcXv5pC8XXRkBK7gUPQX5XDz7iXj_dao_trees (
	daos_tree BLOB NOT NULL,
	proposals_tree BLOB NOT NULL
);

CREATE TABLE IF NOT EXISTS Fd8kfCuqU8BoFFp6GcXv5pC8XXRkBK7gUPQX5XDz7iXj_dao_proposals (
    -- Bulla identifier of the proposal
    bulla BLOB PRIMARY KEY NOT NULL,
    -- Bulla identifier of the DAO this proposal is for
    dao_bulla BLOB NOT NULL,
    -- The on chain representation of the proposal
    proposal BLOB NOT NULL,
    -- Plaintext proposal call data the members share between them
    data BLOB,
    -- These values are NULL until the proposal is minted on chain and received
    -- Leaf position of the proposal in the Merkle tree of proposals
    leaf_position BLOB,
    -- Money merkle tree snapshot for reproducing the snapshot Merkle root
    money_snapshot_tree BLOB,
    -- The transaction hash where the proposal was deployed
    tx_hash BLOB,
    -- The call index in the transaction where the proposal was deployed
    call_index INTEGER,

    FOREIGN KEY(dao_bulla) REFERENCES Fd8kfCuqU8BoFFp6GcXv5pC8XXRkBK7gUPQX5XDz7iXj_dao_daos(bulla) ON DELETE CASCADE ON UPDATE CASCADE
);

CREATE TABLE IF NOT EXISTS Fd8kfCuqU8BoFFp6GcXv5pC8XXRkBK7gUPQX5XDz7iXj_dao_votes (
    -- Numeric identifier of the vote
    vote_id INTEGER PRIMARY KEY AUTOINCREMENT NOT NULL,
    -- Bulla identifier of the proposal this vote is for
    proposal_bulla BLOB NOT NULL,
    -- The vote
    vote_option INTEGER NOT NULL,
    -- Blinding factor for the yes vote
    yes_vote_blind BLOB NOT NULL,
    -- Value of all votes
    all_vote_value BLOB NOT NULL,
    -- Blinding facfor of all votes
    all_vote_blind BLOB NOT NULL,
    -- Transaction hash where this vote was casted
    tx_hash BLOB NOT NULL,
    -- call index in the transaction where this vote was casted
    call_index INTEGER NOT NULL,

    FOREIGN KEY(proposal_bulla) REFERENCES Fd8kfCuqU8BoFFp6GcXv5pC8XXRkBK7gUPQX5XDz7iXj_dao_proposals(bulla) ON DELETE CASCADE ON UPDATE CASCADE
);
