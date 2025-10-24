-- Wallet definition for Deployooor contractt
-- Native Contract ID: EJs7oEjKkvCeEVCmpRsd6fEoTGCFJ7WKUBfmAjwaegN

CREATE TABLE IF NOT EXISTS EJs7oEjKkvCeEVCmpRsd6fEoTGCFJ7WKUBfmAjwaegN_deploy_auth (
	-- TODO: this should be the contract id
	id INTEGER PRIMARY KEY AUTOINCREMENT,
	-- TODO: this should be just the secret
	deploy_authority BLOB UNIQUE NOT NULL,
	is_frozen INTEGER NOT NULL,
	-- TODO: rename to lock height
	freeze_height INTEGER
);
