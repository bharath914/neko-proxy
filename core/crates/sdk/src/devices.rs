//! Persistent registry of paired devices and apps.
//!
//! Lives in its own database file, not in the flow store: wiping captured
//! traffic must not unpair devices, and token hashes should not sit next to
//! data that gets exported.

use crate::config::DeviceConfig;
use rusqlite::{params, Connection, OptionalExtension};
use sha2::{Digest, Sha256};
use std::path::Path;
use std::sync::Mutex;
use subtle::ConstantTimeEq;

#[derive(Debug, thiserror::Error)]
pub enum DevicesError {
    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),
}

type Result<T> = std::result::Result<T, DevicesError>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PairState {
    Paired,
    /// A person pressed Deny. The device is told to back off for a long time.
    Denied,
    /// Pairing was revoked. The device may ask again.
    Revoked,
}

impl PairState {
    fn parse(s: &str) -> PairState {
        match s {
            "paired" => PairState::Paired,
            "denied" => PairState::Denied,
            _ => PairState::Revoked,
        }
    }
}

/// Facts a device reports in `Hello`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DeviceFacts {
    pub device_id: String,
    pub app_id: String,
    pub device_name: String,
    pub device_model: String,
    pub os_version: String,
    pub app_name: String,
    pub app_version: String,
    pub sdk_name: String,
    pub sdk_version: String,
}

#[derive(Clone, Debug)]
pub struct AppRow {
    pub facts: DeviceFacts,
    pub state: PairState,
    pub paired_at_ms: i64,
    pub last_seen_ms: i64,
    pub config: DeviceConfig,
}

#[derive(Debug, PartialEq, Eq)]
pub enum TokenCheck {
    Valid,
    /// Known, but this is not its token.
    Invalid,
    /// Never paired, or revoked.
    Unpaired,
    Denied,
}

pub fn token_hash(token: &[u8]) -> [u8; 32] {
    Sha256::digest(token).into()
}

pub struct Devices {
    conn: Mutex<Connection>,
}

impl Devices {
    pub fn open(path: &Path) -> Result<Devices> {
        let conn = Connection::open(path)?;
        Self::init(conn)
    }

    pub fn in_memory() -> Result<Devices> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> Result<Devices> {
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS sdk_app(
                device_id TEXT NOT NULL,
                app_id TEXT NOT NULL,
                device_name TEXT NOT NULL,
                device_model TEXT NOT NULL,
                os_version TEXT NOT NULL,
                app_name TEXT NOT NULL,
                app_version TEXT NOT NULL,
                sdk_name TEXT NOT NULL,
                sdk_version TEXT NOT NULL,
                state TEXT NOT NULL,
                token_hash BLOB,
                paired_at INTEGER NOT NULL DEFAULT 0,
                last_seen INTEGER NOT NULL DEFAULT 0,
                config_json TEXT NOT NULL DEFAULT '',
                PRIMARY KEY(device_id, app_id)
            );",
        )?;
        Ok(Devices {
            conn: Mutex::new(conn),
        })
    }

    pub fn get(&self, device_id: &str, app_id: &str) -> Result<Option<AppRow>> {
        let c = self.conn.lock().unwrap();
        Ok(c.query_row(
            &format!("{SELECT} WHERE device_id = ?1 AND app_id = ?2"),
            [device_id, app_id],
            row,
        )
        .optional()?)
    }

    pub fn list(&self) -> Result<Vec<AppRow>> {
        let c = self.conn.lock().unwrap();
        let mut st = c.prepare(&format!("{SELECT} ORDER BY last_seen DESC"))?;
        let rows = st.query_map([], row)?.collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Compare a presented token with the stored hash, in constant time.
    pub fn check_token(&self, device_id: &str, app_id: &str, token: &[u8]) -> Result<TokenCheck> {
        let c = self.conn.lock().unwrap();
        let found: Option<(String, Option<Vec<u8>>)> = c
            .query_row(
                "SELECT state, token_hash FROM sdk_app WHERE device_id = ?1 AND app_id = ?2",
                [device_id, app_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        let Some((state, hash)) = found else {
            return Ok(TokenCheck::Unpaired);
        };
        Ok(match PairState::parse(&state) {
            PairState::Denied => TokenCheck::Denied,
            PairState::Revoked => TokenCheck::Unpaired,
            PairState::Paired => {
                let presented = token_hash(token);
                // Compare against a fixed-size value even for an empty token,
                // so timing does not reveal whether a hash exists.
                let stored = hash.unwrap_or_default();
                let ok = !token.is_empty()
                    && stored.len() == 32
                    && bool::from(presented.as_slice().ct_eq(&stored));
                if ok {
                    TokenCheck::Valid
                } else {
                    TokenCheck::Invalid
                }
            }
        })
    }

    /// Record a successful pairing. Only the hash of the token is kept.
    pub fn pair(&self, f: &DeviceFacts, token: &[u8], now_ms: i64) -> Result<()> {
        let c = self.conn.lock().unwrap();
        upsert(&c, f, now_ms)?;
        c.execute(
            "UPDATE sdk_app SET state = 'paired', token_hash = ?3, paired_at = ?4
             WHERE device_id = ?1 AND app_id = ?2",
            params![f.device_id, f.app_id, token_hash(token).to_vec(), now_ms],
        )?;
        Ok(())
    }

    pub fn deny(&self, f: &DeviceFacts, now_ms: i64) -> Result<()> {
        let c = self.conn.lock().unwrap();
        upsert(&c, f, now_ms)?;
        c.execute(
            "UPDATE sdk_app SET state = 'denied', token_hash = NULL
             WHERE device_id = ?1 AND app_id = ?2",
            [&f.device_id, &f.app_id],
        )?;
        Ok(())
    }

    /// Delete the token. The device has to pair again.
    pub fn revoke(&self, device_id: &str, app_id: &str) -> Result<bool> {
        let c = self.conn.lock().unwrap();
        let n = c.execute(
            "UPDATE sdk_app SET state = 'revoked', token_hash = NULL
             WHERE device_id = ?1 AND app_id = ?2",
            [device_id, app_id],
        )?;
        Ok(n > 0)
    }

    /// Remove a device and all its apps, including denied ones.
    pub fn forget(&self, device_id: &str) -> Result<usize> {
        let c = self.conn.lock().unwrap();
        Ok(c.execute("DELETE FROM sdk_app WHERE device_id = ?1", [device_id])?)
    }

    /// Forget every pairing (used when the ingest certificate is replaced).
    pub fn revoke_all(&self) -> Result<usize> {
        let c = self.conn.lock().unwrap();
        Ok(c.execute(
            "UPDATE sdk_app SET state = 'revoked', token_hash = NULL WHERE state = 'paired'",
            [],
        )?)
    }

    /// Refresh what the device reported about itself and when it was seen.
    pub fn touch(&self, f: &DeviceFacts, now_ms: i64) -> Result<()> {
        let c = self.conn.lock().unwrap();
        upsert(&c, f, now_ms)?;
        Ok(())
    }

    pub fn config(&self, device_id: &str, app_id: &str) -> Result<DeviceConfig> {
        Ok(self
            .get(device_id, app_id)?
            .map(|r| r.config)
            .unwrap_or_default())
    }

    pub fn set_config(&self, device_id: &str, app_id: &str, cfg: &DeviceConfig) -> Result<bool> {
        let c = self.conn.lock().unwrap();
        let json = serde_json::to_string(cfg).unwrap_or_default();
        let n = c.execute(
            "UPDATE sdk_app SET config_json = ?3 WHERE device_id = ?1 AND app_id = ?2",
            params![device_id, app_id, json],
        )?;
        Ok(n > 0)
    }
}

const SELECT: &str = "SELECT device_id, app_id, device_name, device_model, os_version, app_name,
        app_version, sdk_name, sdk_version, state, paired_at, last_seen, config_json FROM sdk_app";

fn row(r: &rusqlite::Row) -> rusqlite::Result<AppRow> {
    let json: String = r.get(12)?;
    Ok(AppRow {
        facts: DeviceFacts {
            device_id: r.get(0)?,
            app_id: r.get(1)?,
            device_name: r.get(2)?,
            device_model: r.get(3)?,
            os_version: r.get(4)?,
            app_name: r.get(5)?,
            app_version: r.get(6)?,
            sdk_name: r.get(7)?,
            sdk_version: r.get(8)?,
        },
        state: PairState::parse(&r.get::<_, String>(9)?),
        paired_at_ms: r.get(10)?,
        last_seen_ms: r.get(11)?,
        config: serde_json::from_str::<DeviceConfig>(&json)
            .unwrap_or_default()
            .sanitized(),
    })
}

fn upsert(c: &Connection, f: &DeviceFacts, now_ms: i64) -> rusqlite::Result<()> {
    c.execute(
        "INSERT INTO sdk_app(device_id, app_id, device_name, device_model, os_version, app_name,
            app_version, sdk_name, sdk_version, state, last_seen)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,'revoked',?10)
         ON CONFLICT(device_id, app_id) DO UPDATE SET
            device_name = excluded.device_name, device_model = excluded.device_model,
            os_version = excluded.os_version, app_name = excluded.app_name,
            app_version = excluded.app_version, sdk_name = excluded.sdk_name,
            sdk_version = excluded.sdk_version, last_seen = excluded.last_seen",
        params![
            f.device_id,
            f.app_id,
            f.device_name,
            f.device_model,
            f.os_version,
            f.app_name,
            f.app_version,
            f.sdk_name,
            f.sdk_version,
            now_ms
        ],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts() -> DeviceFacts {
        DeviceFacts {
            device_id: "install-1".into(),
            app_id: "com.shop".into(),
            device_name: "Pixel 8".into(),
            ..Default::default()
        }
    }

    #[test]
    fn token_lifecycle() {
        let d = Devices::in_memory().unwrap();
        let tok = [5u8; 32];
        assert_eq!(d.check_token("install-1", "com.shop", &tok).unwrap(), TokenCheck::Unpaired);
        d.pair(&facts(), &tok, 100).unwrap();
        assert_eq!(d.check_token("install-1", "com.shop", &tok).unwrap(), TokenCheck::Valid);
        assert_eq!(d.check_token("install-1", "com.shop", &[6u8; 32]).unwrap(), TokenCheck::Invalid);
        assert_eq!(d.check_token("install-1", "com.shop", &[]).unwrap(), TokenCheck::Invalid);
        // A token is scoped to one device and app.
        assert_eq!(d.check_token("install-2", "com.shop", &tok).unwrap(), TokenCheck::Unpaired);
        assert_eq!(d.check_token("install-1", "com.other", &tok).unwrap(), TokenCheck::Unpaired);
        assert!(d.revoke("install-1", "com.shop").unwrap());
        assert_eq!(d.check_token("install-1", "com.shop", &tok).unwrap(), TokenCheck::Unpaired);
    }

    #[test]
    fn only_the_hash_is_stored() {
        let d = Devices::in_memory().unwrap();
        let tok = [0xABu8; 32];
        d.pair(&facts(), &tok, 1).unwrap();
        let c = d.conn.lock().unwrap();
        let stored: Vec<u8> = c
            .query_row("SELECT token_hash FROM sdk_app", [], |r| r.get(0))
            .unwrap();
        assert_ne!(stored, tok.to_vec());
        assert_eq!(stored, token_hash(&tok).to_vec());
    }

    #[test]
    fn deny_forget_and_config() {
        let d = Devices::in_memory().unwrap();
        d.deny(&facts(), 5).unwrap();
        assert_eq!(d.check_token("install-1", "com.shop", &[1]).unwrap(), TokenCheck::Denied);
        assert_eq!(d.forget("install-1").unwrap(), 1);
        assert_eq!(d.check_token("install-1", "com.shop", &[1]).unwrap(), TokenCheck::Unpaired);

        d.pair(&facts(), &[1; 32], 5).unwrap();
        let cfg = DeviceConfig {
            capture_enabled: false,
            ..Default::default()
        };
        assert!(d.set_config("install-1", "com.shop", &cfg).unwrap());
        assert_eq!(d.config("install-1", "com.shop").unwrap(), cfg);
        assert_eq!(d.config("nobody", "x").unwrap(), DeviceConfig::default());
        assert_eq!(d.list().unwrap().len(), 1);
        assert_eq!(d.revoke_all().unwrap(), 1);
    }
}
