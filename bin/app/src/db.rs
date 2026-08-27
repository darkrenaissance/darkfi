/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as
 * published by the Free Software Foundation, either version 3 of the
 * License, or (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU Affero General Public License for more details.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

use std::{path::PathBuf, sync::Arc};

use smol::lock::Mutex as AsyncMutex;
use turso::{Connection, Value};

use crate::{
    app::schema::menu::{channel::Channel, contact::Contact},
    error::{Error, Result},
};

pub type AppDbPtr = Arc<AppDb>;

const APP_VERSION_KEY: &str = "app_version";

/// Single turso SQL database owning all app persistent state: channels,
/// contacts, darkirc identity, settings, and app flags. The kvdb-overlay
/// database remains exclusively for the event graph and chat history trees.
pub struct AppDb {
    conn: AsyncMutex<Connection>,
}

impl AppDb {
    pub async fn new(path: &str) -> Result<AppDbPtr> {
        let db = turso::Builder::new_local(path).build().await.map_err(Error::from)?;
        let conn = db.connect().map_err(Error::from)?;
        let self_ = Arc::new(Self { conn: AsyncMutex::new(conn) });
        self_.init_schema().await?;
        Ok(self_)
    }

    async fn init_schema(&self) -> Result<()> {
        let conn = self.conn.lock().await;
        conn.execute_batch(include_str!("../app.sql")).await?;
        // First run: generate the DM identity secret right away. On
        // subsequent runs the row exists and the insert is ignored.
        let secret: [u8; 32] = rand::random();
        conn.execute(
            "INSERT OR IGNORE INTO profiles (id, nick, dm_secret) VALUES (1, 'anon', ?1)",
            vec![Value::Blob(secret.to_vec())],
        )
        .await?;
        Ok(())
    }

    pub async fn channels(&self) -> Result<Vec<Channel>> {
        let conn = self.conn.lock().await;
        let mut stmt = conn.prepare("SELECT name, secret FROM channels ORDER BY name").await?;
        let mut rows = stmt.query(()).await?;
        let mut out = vec![];
        while let Some(row) = rows.next().await? {
            let name = row.get_value(0)?.as_text().ok_or(Error::TursoErr)?.to_string();
            let secret = match row.get_value(1)? {
                Value::Blob(b) if b.len() == 32 => Some(b.as_slice().try_into().unwrap()),
                _ => None,
            };
            out.push(Channel { name, secret });
        }
        Ok(out)
    }

    pub async fn channel_get(&self, name: &str) -> Result<Option<Channel>> {
        let conn = self.conn.lock().await;
        let mut stmt = conn.prepare("SELECT name, secret FROM channels WHERE name = ?1").await?;
        let mut rows = stmt.query(vec![Value::Text(name.to_string())]).await?;
        let Some(row) = rows.next().await? else { return Ok(None) };
        let name = row.get_value(0)?.as_text().ok_or(Error::TursoErr)?.to_string();
        let secret = match row.get_value(1)? {
            Value::Blob(b) if b.len() == 32 => Some(b.as_slice().try_into().unwrap()),
            _ => None,
        };
        Ok(Some(Channel { name, secret }))
    }

    pub async fn channel_insert(&self, channel: &Channel) -> Result<()> {
        let conn = self.conn.lock().await;
        let secret = channel.secret.map(|s| Value::Blob(s.to_vec())).unwrap_or(Value::Null);
        conn.execute(
            "INSERT OR REPLACE INTO channels (name, secret) VALUES (?1, ?2)",
            vec![Value::Text(channel.name.clone()), secret],
        )
        .await?;
        Ok(())
    }

    pub async fn contacts(&self) -> Result<Vec<Contact>> {
        let conn = self.conn.lock().await;
        let mut stmt = conn.prepare("SELECT name, public FROM contacts ORDER BY name").await?;
        let mut rows = stmt.query(()).await?;
        let mut out = vec![];
        while let Some(row) = rows.next().await? {
            let name = row.get_value(0)?.as_text().ok_or(Error::TursoErr)?.to_string();
            let public =
                row.get_value(1)?.as_blob().ok_or(Error::TursoErr)?.as_slice().try_into().unwrap();
            out.push(Contact { name, public });
        }
        Ok(out)
    }

    pub async fn contact_get(&self, name: &str) -> Result<Option<Contact>> {
        let conn = self.conn.lock().await;
        let mut stmt = conn.prepare("SELECT name, public FROM contacts WHERE name = ?1").await?;
        let mut rows = stmt.query(vec![Value::Text(name.to_string())]).await?;
        let Some(row) = rows.next().await? else { return Ok(None) };
        let name = row.get_value(0)?.as_text().ok_or(Error::TursoErr)?.to_string();
        let public =
            row.get_value(1)?.as_blob().ok_or(Error::TursoErr)?.as_slice().try_into().unwrap();
        Ok(Some(Contact { name, public }))
    }

    pub async fn contact_insert(&self, contact: &Contact) -> Result<()> {
        let conn = self.conn.lock().await;
        conn.execute(
            "INSERT OR REPLACE INTO contacts (name, public) VALUES (?1, ?2)",
            vec![Value::Text(contact.name.clone()), Value::Blob(contact.public.to_vec())],
        )
        .await?;
        Ok(())
    }

    pub async fn nick_get(&self) -> Result<Option<String>> {
        let conn = self.conn.lock().await;
        let mut stmt = conn.prepare("SELECT nick FROM profiles WHERE id = 1").await?;
        let mut rows = stmt.query(()).await?;
        match rows.next().await? {
            Some(row) => Ok(Some(row.get_value(0)?.as_text().ok_or(Error::TursoErr)?.to_string())),
            None => Ok(None),
        }
    }

    pub async fn nick_set(&self, nick: &str) -> Result<()> {
        let conn = self.conn.lock().await;
        conn.execute(
            "UPDATE profiles SET nick = ?1 WHERE id = 1",
            vec![Value::Text(nick.to_string())],
        )
        .await?;
        Ok(())
    }

    /// The identity row is created with a fresh random secret during
    /// schema init, so this only reads.
    pub async fn dm_secret(&self) -> Result<[u8; 32]> {
        let conn = self.conn.lock().await;
        let mut stmt = conn.prepare("SELECT dm_secret FROM profiles WHERE id = 1").await?;
        let mut rows = stmt.query(()).await?;
        let Some(row) = rows.next().await? else { return Err(Error::TursoErr) };
        let Value::Blob(b) = row.get_value(0)? else { return Err(Error::TursoErr) };
        if b.len() != 32 {
            return Err(Error::TursoErr)
        }
        Ok(b.as_slice().try_into().unwrap())
    }

    /// Load all settings rows as (name, idx, value bytes).
    pub async fn settings_all(&self) -> Result<Vec<(String, u32, Vec<u8>)>> {
        let conn = self.conn.lock().await;
        let mut stmt =
            conn.prepare("SELECT name, idx, value FROM settings ORDER BY name, idx").await?;
        let mut rows = stmt.query(()).await?;
        let mut out = vec![];
        while let Some(row) = rows.next().await? {
            let name = row.get_value(0)?.as_text().ok_or(Error::TursoErr)?.to_string();
            let idx = *row.get_value(1)?.as_integer().ok_or(Error::TursoErr)?;
            let value = row.get_value(2)?.as_blob().ok_or(Error::TursoErr)?.to_vec();
            out.push((name, idx as u32, value));
        }
        Ok(out)
    }

    pub async fn setting_get(&self, name: &str, idx: u32) -> Result<Option<Vec<u8>>> {
        let conn = self.conn.lock().await;
        let mut stmt =
            conn.prepare("SELECT value FROM settings WHERE name = ?1 AND idx = ?2").await?;
        let mut rows =
            stmt.query(vec![Value::Text(name.to_string()), Value::Integer(idx as i64)]).await?;
        match rows.next().await? {
            Some(row) => Ok(Some(row.get_value(0)?.as_blob().ok_or(Error::TursoErr)?.to_vec())),
            None => Ok(None),
        }
    }

    pub async fn setting_put(&self, name: &str, idx: u32, typ: &str, value: &[u8]) -> Result<()> {
        let conn = self.conn.lock().await;
        conn.execute(
            "INSERT OR REPLACE INTO settings (name, idx, type, value) VALUES (?1, ?2, ?3, ?4)",
            vec![
                Value::Text(name.to_string()),
                Value::Integer(idx as i64),
                Value::Text(typ.to_string()),
                Value::Blob(value.to_vec()),
            ],
        )
        .await?;
        Ok(())
    }

    pub async fn setting_remove_idx(&self, name: &str, idx: u32) -> Result<()> {
        let conn = self.conn.lock().await;
        conn.execute(
            "DELETE FROM settings WHERE name = ?1 AND idx = ?2",
            vec![Value::Text(name.to_string()), Value::Integer(idx as i64)],
        )
        .await?;
        Ok(())
    }

    /// Semver version of the app build that last ran, or `None` on a
    /// fresh database.
    pub async fn app_version_get(&self) -> Result<Option<String>> {
        let conn = self.conn.lock().await;
        let mut stmt = conn.prepare("SELECT value FROM flags WHERE name = ?1").await?;
        let mut rows = stmt.query(vec![Value::Text(APP_VERSION_KEY.to_string())]).await?;
        match rows.next().await? {
            Some(row) => Ok(Some(row.get_value(0)?.as_text().ok_or(Error::TursoErr)?.to_string())),
            None => Ok(None),
        }
    }

    pub async fn app_version_set(&self, version: &str) -> Result<()> {
        let conn = self.conn.lock().await;
        conn.execute(
            "INSERT OR REPLACE INTO flags (name, value) VALUES (?1, ?2)",
            vec![Value::Text(APP_VERSION_KEY.to_string()), Value::Text(version.to_string())],
        )
        .await?;
        Ok(())
    }
}

#[cfg(target_os = "android")]
pub fn get_app_db_path() -> PathBuf {
    crate::android::get_appdata_path().join("app.db")
}

#[cfg(not(target_os = "android"))]
pub fn get_app_db_path() -> PathBuf {
    dirs::data_local_dir().unwrap().join("darkfi/app/app.db")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crypto_box::SecretKey;

    fn temp_db_path(tag: &str) -> String {
        let path = std::env::temp_dir()
            .join(format!("darkfi-app-db-test-{tag}-{}.db", std::process::id()));
        let _ = std::fs::remove_file(&path);
        path.to_str().unwrap().to_string()
    }

    /// Covers the app-storage spec: fresh start creates schema + seeds,
    /// channels/contacts/settings/version roundtrip, identity (nick + DM
    /// secret) survives a reopen.
    #[test]
    fn app_db_persistence() {
        let path = temp_db_path("persist");

        let seed_secret: [u8; 32] = core::array::from_fn(|i| i as u8);
        let dm_public = {
            let db = smol::block_on(AppDb::new(&path)).unwrap();

            assert!(smol::block_on(db.channels()).unwrap().is_empty());
            smol::block_on(db.channel_insert(&Channel { name: "dev".into(), secret: None }))
                .unwrap();
            smol::block_on(db.channel_insert(&Channel {
                name: "secret_chan".into(),
                secret: Some(seed_secret),
            }))
            .unwrap();

            smol::block_on(
                db.contact_insert(&Contact { name: "alice".into(), public: seed_secret }),
            )
            .unwrap();

            smol::block_on(db.nick_set("testnick")).unwrap();

            smol::block_on(db.setting_put("net.localnet", 0, "bool", &[1])).unwrap();
            smol::block_on(db.setting_put("net.localnet", 2, "bool", &[0])).unwrap();

            assert_eq!(smol::block_on(db.app_version_get()).unwrap(), None);
            smol::block_on(db.app_version_set(env!("CARGO_PKG_VERSION"))).unwrap();

            let secret = smol::block_on(db.dm_secret()).unwrap();
            SecretKey::from_bytes(secret).public_key().to_bytes()
        };

        {
            let db = smol::block_on(AppDb::new(&path)).unwrap();

            let channels = smol::block_on(db.channels()).unwrap();
            assert_eq!(channels.len(), 2);
            assert_eq!(channels[0].name, "dev");
            assert_eq!(channels[0].secret, None);
            assert_eq!(channels[1].name, "secret_chan");
            assert_eq!(channels[1].secret, Some(seed_secret));

            let chan = smol::block_on(db.channel_get("dev")).unwrap().unwrap();
            assert_eq!(chan.name, "dev");

            let contacts = smol::block_on(db.contacts()).unwrap();
            assert_eq!(contacts.len(), 1);
            assert_eq!(contacts[0].name, "alice");
            assert_eq!(contacts[0].public, seed_secret);

            assert_eq!(smol::block_on(db.nick_get()).unwrap(), Some("testnick".into()));

            assert_eq!(smol::block_on(db.setting_get("net.localnet", 0)).unwrap(), Some(vec![1u8]));
            assert_eq!(smol::block_on(db.setting_get("net.localnet", 2)).unwrap(), Some(vec![0u8]));
            assert_eq!(smol::block_on(db.setting_get("net.localnet", 1)).unwrap(), None);

            smol::block_on(db.setting_remove_idx("net.localnet", 2)).unwrap();
            assert_eq!(smol::block_on(db.setting_get("net.localnet", 2)).unwrap(), None);
            assert_eq!(smol::block_on(db.settings_all()).unwrap().len(), 1);

            assert_eq!(
                smol::block_on(db.app_version_get()).unwrap(),
                Some(env!("CARGO_PKG_VERSION").to_string())
            );

            // DM identity must be stable across reopen
            let secret = smol::block_on(db.dm_secret()).unwrap();
            assert_eq!(SecretKey::from_bytes(secret).public_key().to_bytes(), dm_public);
        }

        let _ = std::fs::remove_file(&path);
    }

    /// Covers: no-nick start keeps the default row, first-run generates and
    /// persists a fresh DM identity (nonzero).
    #[test]
    fn app_db_fresh_identity() {
        let path = temp_db_path("fresh");
        let db = smol::block_on(AppDb::new(&path)).unwrap();

        assert_eq!(smol::block_on(db.nick_get()).unwrap(), Some("anon".into()));

        let secret = smol::block_on(db.dm_secret()).unwrap();
        assert!(secret.iter().any(|&x| x != 0));

        let again = smol::block_on(db.dm_secret()).unwrap();
        assert_eq!(secret, again);

        let _ = std::fs::remove_file(&path);
    }
}
