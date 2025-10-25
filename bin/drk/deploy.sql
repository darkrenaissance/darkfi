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
