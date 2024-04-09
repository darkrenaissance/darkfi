-- Wallet definition for Deployooor contractt
-- Native Contract ID: EJs7oEjKkvCeEVCmpRsd6fEoTGCFJ7WKUBfmAjwaegN

CREATE TABLE IF NOT EXISTS EJs7oEjKkvCeEVCmpRsd6fEoTGCFJ7WKUBfmAjwaegN_deploy_auth (
	deploy_authority BLOB PRIMARY KEY NOT NULL,
	is_frozen INTEGER NOT NULL
);
