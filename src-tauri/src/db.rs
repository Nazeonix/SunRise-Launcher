//! SQLite storage: instances, settings and the Microsoft account.

use anyhow::{anyhow, Result};
use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::{Deserialize, Serialize};
use std::{
    path::Path,
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};

pub fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Instance {
    pub id: i64,
    pub name: String,
    pub mc_version: String,
    /// vanilla | fabric | forge | neoforge
    pub loader: String,
    /// "" = newest build
    pub loader_version: String,
    /// "" = default (global setting / auto)
    pub java_path: String,
    /// 0 = default from settings
    pub ram_mb: i64,
    pub color: String,
    pub last_played: Option<i64>,
    pub created_at: i64,
    /// instance folder; "" = <data>/instances/<id>
    pub dir: String,
    /// optional screenshot used as the instance banner
    pub banner_url: String,
    /// optional image used as the instance avatar
    pub icon_url: String,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct NewInstance {
    pub name: String,
    pub mc_version: String,
    #[serde(default)]
    pub loader: String,
    #[serde(default)]
    pub loader_version: String,
    #[serde(default)]
    pub java_path: String,
    #[serde(default)]
    pub ram_mb: i64,
    #[serde(default)]
    pub color: String,
    #[serde(default)]
    pub banner_url: String,
    #[serde(default)]
    pub icon_url: String,
    /// parent folder chosen in the dialog ("" = Settings default)
    #[serde(default)]
    pub dir_parent: String,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub theme: String,
    pub mode: String,
    pub username: String,
    pub ram_mb: i64,
    pub use_discrete_gpu: bool,
    pub java_path: String,
    pub custom_java: Vec<String>,
    pub ms_client_id: String,
    pub show_snapshots: bool,
    /// default parent folder for new instances ("" = <data>/instances)
    pub instances_dir: String,
    /// the user's own CurseForge API key (Modrinth needs none)
    pub curseforge_key: String,
    /// paths in the user's media queue
    pub music_tracks: Vec<String>,
    /// list | grid
    pub instance_layout: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: "lavender".into(),
            mode: "system".into(),
            username: "Player".into(),
            ram_mb: 4096,
            use_discrete_gpu: true,
            java_path: String::new(),
            custom_java: Vec::new(),
            ms_client_id: String::new(),
            show_snapshots: false,
            instances_dir: String::new(),
            curseforge_key: String::new(),
            music_tracks: Vec::new(),
            instance_layout: "list".into(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Account {
    pub id: i64,
    pub username: String,
    pub uuid: String,
    pub provider: String,
    pub api_root: String,
    pub client_token: String,
    pub skin_url: String,
    pub refresh_token: String,
}

const COLS: &str = "id,name,mc_version,loader,loader_version,java_path,ram_mb,color,last_played,created_at,dir,banner_url,icon_url";

fn row_to_instance(r: &Row) -> rusqlite::Result<Instance> {
    Ok(Instance {
        id: r.get(0)?,
        name: r.get(1)?,
        mc_version: r.get(2)?,
        loader: r.get(3)?,
        loader_version: r.get(4)?,
        java_path: r.get(5)?,
        ram_mb: r.get(6)?,
        color: r.get(7)?,
        last_played: r.get(8)?,
        created_at: r.get(9)?,
        dir: r.get(10)?,
        banner_url: r.get(11)?,
        icon_url: r.get(12)?,
    })
}

pub struct Db(Mutex<Connection>);

impl Db {
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)?;
        conn.execute_batch(
            "PRAGMA journal_mode=WAL;
             CREATE TABLE IF NOT EXISTS instances(
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 name TEXT NOT NULL,
                 mc_version TEXT NOT NULL,
                 loader TEXT NOT NULL DEFAULT 'vanilla',
                 loader_version TEXT NOT NULL DEFAULT '',
                 java_path TEXT NOT NULL DEFAULT '',
                 ram_mb INTEGER NOT NULL DEFAULT 0,
                 color TEXT NOT NULL DEFAULT '#6A52B3',
                 last_played INTEGER,
                 created_at INTEGER NOT NULL,
                 dir TEXT NOT NULL DEFAULT '',
                 banner_url TEXT NOT NULL DEFAULT '',
                 icon_url TEXT NOT NULL DEFAULT ''
             );
             CREATE TABLE IF NOT EXISTS settings(key TEXT PRIMARY KEY, value TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS account(
                 id INTEGER PRIMARY KEY CHECK (id = 1),
                 username TEXT NOT NULL,
                 uuid TEXT NOT NULL,
                 refresh_token TEXT NOT NULL,
                 provider TEXT NOT NULL DEFAULT 'microsoft',
                 api_root TEXT NOT NULL DEFAULT '',
                 client_token TEXT NOT NULL DEFAULT '',
                 skin_url TEXT NOT NULL DEFAULT ''
             );
             CREATE TABLE IF NOT EXISTS accounts(
                 id INTEGER PRIMARY KEY AUTOINCREMENT,
                 username TEXT NOT NULL,
                 uuid TEXT NOT NULL,
                 refresh_token TEXT NOT NULL,
                 provider TEXT NOT NULL,
                 api_root TEXT NOT NULL DEFAULT '',
                 client_token TEXT NOT NULL DEFAULT '',
                 skin_url TEXT NOT NULL DEFAULT '',
                 UNIQUE(provider, api_root, uuid)
             );
             CREATE TABLE IF NOT EXISTS account_state(
                 id INTEGER PRIMARY KEY CHECK(id=1),
                 active_account_id INTEGER
             );",
        )?;
        // Older databases contain only Microsoft account fields.
        let account_columns = {
            let mut st = conn.prepare("PRAGMA table_info(account)")?;
            let names = st
                .query_map([], |r| r.get::<_, String>(1))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            names
        };
        if !account_columns.iter().any(|n| n == "provider") {
            conn.execute(
                "ALTER TABLE account ADD COLUMN provider TEXT NOT NULL DEFAULT 'microsoft'",
                [],
            )?;
        }
        if !account_columns.iter().any(|n| n == "api_root") {
            conn.execute(
                "ALTER TABLE account ADD COLUMN api_root TEXT NOT NULL DEFAULT ''",
                [],
            )?;
        }
        if !account_columns.iter().any(|n| n == "client_token") {
            conn.execute(
                "ALTER TABLE account ADD COLUMN client_token TEXT NOT NULL DEFAULT ''",
                [],
            )?;
        }
        if !account_columns.iter().any(|n| n == "skin_url") {
            conn.execute(
                "ALTER TABLE account ADD COLUMN skin_url TEXT NOT NULL DEFAULT ''",
                [],
            )?;
        }
        // Move the previous single-account record into the new account collection.
        conn.execute_batch(
            "INSERT OR IGNORE INTO accounts(username,uuid,refresh_token,provider,api_root,client_token,skin_url)
                 SELECT username,uuid,refresh_token,provider,api_root,client_token,skin_url FROM account WHERE id=1;
             INSERT OR IGNORE INTO account_state(id,active_account_id)
                 VALUES(1,(SELECT id FROM accounts ORDER BY id LIMIT 1));
             UPDATE account_state SET active_account_id=(SELECT id FROM accounts ORDER BY id LIMIT 1)
                 WHERE id=1 AND active_account_id IS NULL AND EXISTS(SELECT 1 FROM accounts);
             DELETE FROM account;",
        )?;
        // older databases: add the `dir` column
        let has_dir = {
            let mut st = conn.prepare("PRAGMA table_info(instances)")?;
            let names = st
                .query_map([], |r| r.get::<_, String>(1))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            names.iter().any(|n| n == "dir")
        };
        if !has_dir {
            conn.execute(
                "ALTER TABLE instances ADD COLUMN dir TEXT NOT NULL DEFAULT ''",
                [],
            )?;
        }
        let has_banner_url = {
            let mut st = conn.prepare("PRAGMA table_info(instances)")?;
            let names = st
                .query_map([], |r| r.get::<_, String>(1))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            names.iter().any(|n| n == "banner_url")
        };
        if !has_banner_url {
            conn.execute(
                "ALTER TABLE instances ADD COLUMN banner_url TEXT NOT NULL DEFAULT ''",
                [],
            )?;
        }
        let has_icon_url = {
            let mut st = conn.prepare("PRAGMA table_info(instances)")?;
            let names = st.query_map([], |r| r.get::<_, String>(1))?.collect::<rusqlite::Result<Vec<_>>>()?;
            names.iter().any(|n| n == "icon_url")
        };
        if !has_icon_url {
            conn.execute("ALTER TABLE instances ADD COLUMN icon_url TEXT NOT NULL DEFAULT ''", [])?;
        }
        // the file holds the refresh token – keep it private
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
        }
        Ok(Db(Mutex::new(conn)))
    }

    // ── instances

    pub fn instances(&self) -> Result<Vec<Instance>> {
        let c = self.0.lock().unwrap();
        let mut st = c.prepare(&format!("SELECT {COLS} FROM instances ORDER BY id"))?;
        let rows = st.query_map([], row_to_instance)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn instance(&self, id: i64) -> Result<Instance> {
        let c = self.0.lock().unwrap();
        c.query_row(
            &format!("SELECT {COLS} FROM instances WHERE id=?1"),
            params![id],
            row_to_instance,
        )
        .map_err(|e| anyhow!("instance {id}: {e}"))
    }

    pub fn insert_instance(&self, n: &NewInstance, dir: &str) -> Result<Instance> {
        let id = {
            let c = self.0.lock().unwrap();
            c.execute(
                "INSERT INTO instances(name,mc_version,loader,loader_version,java_path,ram_mb,color,created_at,dir,banner_url,icon_url)
                 VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
                params![
                    n.name,
                    n.mc_version,
                    if n.loader.is_empty() { "vanilla" } else { n.loader.as_str() },
                    n.loader_version,
                    n.java_path,
                    n.ram_mb,
                    if n.color.is_empty() { "#6A52B3" } else { n.color.as_str() },
                    now(),
                    dir,
                    n.banner_url,
                    n.icon_url
                ],
            )?;
            c.last_insert_rowid()
        };
        self.instance(id)
    }

    pub fn update_instance(&self, i: &Instance) -> Result<()> {
        let c = self.0.lock().unwrap();
        c.execute(
            "UPDATE instances SET name=?1,mc_version=?2,loader=?3,loader_version=?4,java_path=?5,ram_mb=?6,color=?7,banner_url=?8,icon_url=?9 WHERE id=?10",
            params![i.name, i.mc_version, i.loader, i.loader_version, i.java_path, i.ram_mb, i.color, i.banner_url, i.icon_url, i.id],
        )?;
        Ok(())
    }

    pub fn delete_instance(&self, id: i64) -> Result<()> {
        let c = self.0.lock().unwrap();
        c.execute("DELETE FROM instances WHERE id=?1", params![id])?;
        Ok(())
    }

    pub fn touch_played(&self, id: i64) -> Result<()> {
        let c = self.0.lock().unwrap();
        c.execute(
            "UPDATE instances SET last_played=?1 WHERE id=?2",
            params![now(), id],
        )?;
        Ok(())
    }

    // ── settings

    pub fn settings(&self) -> Settings {
        let c = self.0.lock().unwrap();
        c.query_row("SELECT value FROM settings WHERE key='settings'", [], |r| {
            r.get::<_, String>(0)
        })
        .optional()
        .ok()
        .flatten()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
    }

    pub fn save_settings(&self, s: &Settings) -> Result<()> {
        let json = serde_json::to_string(s)?;
        let c = self.0.lock().unwrap();
        c.execute(
            "INSERT INTO settings(key,value) VALUES('settings',?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            params![json],
        )?;
        Ok(())
    }

    // ── account

    fn account_from_row(r: &Row<'_>) -> rusqlite::Result<Account> {
        Ok(Account {
            id: r.get(0)?,
            username: r.get(1)?,
            uuid: r.get(2)?,
            refresh_token: r.get(3)?,
            provider: r.get(4)?,
            api_root: r.get(5)?,
            client_token: r.get(6)?,
            skin_url: r.get(7)?,
        })
    }

    pub fn accounts(&self) -> Result<Vec<Account>> {
        let c = self.0.lock().unwrap();
        let mut st = c.prepare("SELECT id,username,uuid,refresh_token,provider,api_root,client_token,skin_url FROM accounts ORDER BY id")?;
        let rows = st.query_map([], Self::account_from_row)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn account(&self) -> Option<Account> {
        let c = self.0.lock().unwrap();
        c.query_row(
            "SELECT a.id,a.username,a.uuid,a.refresh_token,a.provider,a.api_root,a.client_token,a.skin_url
             FROM accounts a JOIN account_state s ON s.active_account_id=a.id WHERE s.id=1", [], Self::account_from_row,
        ).optional().ok().flatten()
    }

    pub fn save_account(&self, a: &Account) -> Result<Account> {
        let c = self.0.lock().unwrap();
        c.execute(
            "INSERT INTO accounts(username,uuid,refresh_token,provider,api_root,client_token,skin_url)
             VALUES(?1,?2,?3,?4,?5,?6,?7)
             ON CONFLICT(provider,api_root,uuid) DO UPDATE SET username=excluded.username,refresh_token=excluded.refresh_token,
                 client_token=excluded.client_token,skin_url=excluded.skin_url",
            params![a.username, a.uuid, a.refresh_token, a.provider, a.api_root, a.client_token, a.skin_url],
        )?;
        let id: i64 = c.query_row(
            "SELECT id FROM accounts WHERE provider=?1 AND api_root=?2 AND uuid=?3",
            params![a.provider, a.api_root, a.uuid],
            |r| r.get(0),
        )?;
        let mut saved = a.clone();
        saved.id = id;
        Ok(saved)
    }

    pub fn set_active_account(&self, id: Option<i64>) -> Result<()> {
        let c = self.0.lock().unwrap();
        if let Some(id) = id {
            let exists: bool = c.query_row(
                "SELECT EXISTS(SELECT 1 FROM accounts WHERE id=?1)",
                params![id],
                |r| r.get(0),
            )?;
            if !exists {
                return Err(anyhow!("Account not found"));
            }
        }
        c.execute("INSERT INTO account_state(id,active_account_id) VALUES(1,?1) ON CONFLICT(id) DO UPDATE SET active_account_id=excluded.active_account_id", params![id])?;
        Ok(())
    }

    pub fn delete_account(&self, id: i64) -> Result<()> {
        let c = self.0.lock().unwrap();
        c.execute("DELETE FROM accounts WHERE id=?1", params![id])?;
        let active: Option<i64> = c
            .query_row(
                "SELECT active_account_id FROM account_state WHERE id=1",
                [],
                |r| r.get(0),
            )
            .optional()?
            .flatten();
        if active == Some(id) {
            c.execute("UPDATE account_state SET active_account_id=(SELECT id FROM accounts ORDER BY id LIMIT 1) WHERE id=1", [])?;
        }
        Ok(())
    }
}
