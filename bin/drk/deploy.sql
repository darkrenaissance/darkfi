-- Wallet definition for Deployooor contractt
-- Native Contract ID: EJs7oEjKkvCeEVCmpRsd6fEoTGCFJ7WKUBfmAjwaegN

CREATE TABLE IF NOT EXISTS EJs7oEjKkvCeEVCmpRsd6fEoTGCFJ7WKUBfmAjwaegN_deploy_auth (
    -- Authority Contract ID
    contract_id BLOB PRIMARY KEY NOT NULL,
    -- Authority keypair secret key
    secret_key BLOB NOT NULL,
    -- Contract lock flag
    is_locked INTEGER NOT NULL,
    -- Block height of the transaction this contract was locked on chain
    lock_height INTEGER
);

CREATE TABLE IF NOT EXISTS EJs7oEjKkvCeEVCmpRsd6fEoTGCFJ7WKUBfmAjwaegN_deploy_history (
    -- Transaction hash where this deployment action was executed
    tx_hash TEXT PRIMARY KEY NOT NULL,
    -- Authority identifier this deployment action is for
    contract BLOB NOT NULL,
    -- Type of this deployment action
    type TEXT NOT NULL,
    -- Block height of the transaction this deployment action was executed
    block_height INTEGER NOT NULL,
    -- Deployed WASM bincode of a deploy type action
    wasm_bincode BLOB,
    -- Serialized deploy instruction of a deploy type action
    deploy_ix BLOB,

    FOREIGN KEY(contract) REFERENCES EJs7oEjKkvCeEVCmpRsd6fEoTGCFJ7WKUBfmAjwaegN_deploy_auth(contract_id) ON DELETE CASCADE ON UPDATE CASCADE
);
