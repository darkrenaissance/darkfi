-- # DAO::mint()
-- 
-- First one person will create the DAO
--
--   $ drk dao create_dao \
--       PROPOSER_LIMIT \
--       QUORUM \
--       EARLY_EXEC_QUORUM \
--       APPROVAL_RATIO \
--       GOV_TOKEN_ID > dao.toml
--
-- dat.toml contains:
--
-- * DAO parameters as listed above
-- * Secret keys for the DAO
-- * Bulla blind
--
-- We can view the data like so:
--
--   $ drk dao view < dao.toml
--
-- Now everyone inside the DAO exchanges dao.toml out of band,
-- will import it into their wallets.
--
--   $ drk dao import DAO_NAME < dao.toml
--   Imported DAO ccb8XXX8af6
--
-- Where ccb8XXX8af6 is the DAO's name.
--
-- Next someone that holds all the keys will mint it on chain
--
--   $ drk dao mint DAO_NAME > dao_mint_tx
--   $ drk broadcast < dao_mint_tx
--
-- And then the others will receive confirmation that the DAO they imported
-- into their wallet has also been accepted on chain.
--
-- # Minting and Receiving Coin
--
-- Assume that the governance tokens have been created and distributed
-- appropriately among DAO members. We will skip that part here.
--
-- Now the DAO can receive coins into its treasury. These coins simply
-- have both the coin's spend_hook and user_data fields set correctly
-- otherwise they are rejected as invalid/malformed.
--
-- # DAO::propose()
--
-- Create a transfer proposal for the DAO
--
--   $ drk dao propose-transfer \
--       DAO_NAME \
--       DURATION \
--       AMOUNT \
--       SENDCOIN_TOKEN_ID \
--       RECV_PUBKEY
--
-- If we don't have enough tokens to meet the proposer_limit threshold
-- or don't hold the proposer key, then this call will simply fail with
-- an error message. Nothing will be added to the database or sent to the
-- network.
--
-- Once a proposal has been generated, it can be exported and shared
-- to other participants.
--
--  $ drk dao proposal PROPOSAL_BULLA --export > dao_transfer_proposal.dat
--  $ drk dao proposal-import < dao_transfer_proposal.dat
--
-- We can now mint the proposal on-chain
--
--  $ drk dao proposal PROPOSAL_BULLA --mint-proposal > dao_proposal_tx
--  $ drk broadcast < dao_proposal_tx
--
-- # DAO::vote()
--
-- You have received a proposal which is active. You can now vote on it.
-- You will see other votes only if you hold the DAO votes view key.
--
--   $ drk dao proposals DAO_NAME
--   0. f6cae63ced53d02b372206a8d3ed5ac03fde18da306a520285fd56e8d031f6cf
--   1. 1372622f4a38be6eb1c90fa67864474c6603d9f8d4228106e20e2d0d04f2395e
--   2. 88b18cbc38dbd3af8d25237af3903e985f70ea06d1e25966bf98e3f08e23c992
--
--   $ drk dao show_proposal f6cae...1f6cf
--    Proposal parameters
--    ===================
--    Bulla: f6cae...1f6cf
--    DAO Bulla: 2Wmyc...zQeke
--    Proposal leaf position: Position(1)
--    Proposal transaction hash: 07148...52f96
--    Proposal call index: 0
--    Creation block window: 0
--    Duration: 30 (Block windows)
--
--    Invoked contracts:
--      Contract: Fd8kf...z7iXj
--      Function: 4
--      Data:
--        Recipient: DQeQR...q31Fz
--        Amount: 690000000 (6.9)
--        Token: GY8xX...xY8Qu
--        Spend hook: 6iW9n...2GLuT
--        User data: 0x35431...e3678
--        Blind: 13EHb...xR6ng
--
--      Contract: BZHKG...4yf4o
--      Function: 3
--      Data: -
--
--    Votes:
--      ...
--    Total tokens votes: X + Y
--    Total tokens Yes votes: X (60%)
--    Total tokens No votes: Y
--    Voting status: Ongoing
--    Current proposal outcome: Rejected
--
--   $ drk dao vote f6cae...1f6cf 1 > dao_vote_tx
--   $ drk broadcast < dao_vote_tx
--
-- # DAO::exec()
--
-- Once there are enough yes votes to satisfy the quorum and approval ratio,
-- then any DAO member can execute the proposal.
--
--   $ drk dao exec f6cae...1f6cf > dao_exec_tx
--   $ drk broadcast < dao_exec_tx

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
    -- Money nullifiers SMT snapshot for reproducing the snapshot Merkle root
    nullifiers_smt_snapshot BLOB,
    -- Block height of the transaction this proposal was deployed
    mint_height INTEGER,
    -- The transaction hash where the proposal was deployed
    tx_hash BLOB,
    -- The call index in the transaction where the proposal was deployed
    call_index INTEGER,
    -- Block height of the transaction this proposal was executed on chain
    exec_height INTEGER,
    -- The transaction hash where the proposal is executed on chain
    exec_tx_hash BLOB,

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
    -- Block height of the transaction this vote was casted
    block_height INTEGER NOT NULL,
    -- Transaction hash where this vote was casted
    tx_hash BLOB NOT NULL,
    -- Call index in the transaction where this vote was casted
    call_index INTEGER NOT NULL,
    -- Vote input nullifiers
    nullifiers BLOB NOT NULL,

    FOREIGN KEY(proposal_bulla) REFERENCES Fd8kfCuqU8BoFFp6GcXv5pC8XXRkBK7gUPQX5XDz7iXj_dao_proposals(bulla) ON DELETE CASCADE ON UPDATE CASCADE
);
