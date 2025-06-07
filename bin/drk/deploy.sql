-- Wallet definition for Deployooor contractt
-- Native Contract ID: EJs7oEjKkvCeEVCmpRsd6fEoTGCFJ7WKUBfmAjwaegN

CREATE TABLE IF NOT EXISTS EJs7oEjKkvCeEVCmpRsd6fEoTGCFJ7WKUBfmAjwaegN_deploy_auth (
	id INTEGER PRIMARY KEY AUTOINCREMENT,
	deploy_authority BLOB UNIQUE NOT NULL,
	is_frozen INTEGER NOT NULL,
	freeze_height INTEGER
);
