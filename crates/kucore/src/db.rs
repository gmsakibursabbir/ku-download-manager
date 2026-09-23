//! SQLite persistence. The database is the source of truth for crash recovery:
//! every state transition is written immediately, live progress is flushed in
//! batches.

use crate::settings::Settings;
use crate::types::{Queue, Schedule};
use anyhow::{Context, Result};
use ku_proto::{Download, DownloadMeta, DownloadOptions, Engine, Kind, Status};
use rusqlite::{params, Connection, OptionalExtension, Row};
use std::path::Path;
use std::sync::Mutex;

pub struct Db {
    conn: Mutex<Connection>,
}

const MIGRATIONS: &[&str] = &[
    // v1
    r#"
    CREATE TABLE downloads (
        id TEXT PRIMARY KEY,
        engine TEXT NOT NULL,
        kind TEXT NOT NULL,
        url TEXT NOT NULL,
        mirrors TEXT NOT NULL DEFAULT '[]',
        name TEXT NOT NULL,
        dir TEXT NOT NULL,
        file_path TEXT,
        status TEXT NOT NULL,
        total INTEGER NOT NULL DEFAULT 0,
        done INTEGER NOT NULL DEFAULT 0,
        uploaded INTEGER NOT NULL DEFAULT 0,
        connections INTEGER NOT NULL DEFAULT 0,
        category TEXT NOT NULL DEFAULT '',
        queue_id TEXT,
        position INTEGER NOT NULL DEFAULT 0,
        created_at INTEGER NOT NULL,
        completed_at INTEGER,
        error TEXT,
        gid TEXT,
        source TEXT NOT NULL DEFAULT '',
        options TEXT NOT NULL DEFAULT '{}',
        meta TEXT NOT NULL DEFAULT '{}'
    );
    CREATE INDEX downloads_status ON downloads(status);
    CREATE INDEX downloads_queue ON downloads(queue_id, position);
    CREATE TABLE kv (key TEXT PRIMARY KEY, value TEXT NOT NULL);
    CREATE TABLE queues (id TEXT PRIMARY KEY, data TEXT NOT NULL);
    CREATE TABLE schedules (id TEXT PRIMARY KEY, data TEXT NOT NULL);
    "#,
];

fn engine_str(e: Engine) -> &'static str {
    match e {
        Engine::Aria2 => "aria2",
        Engine::Ytdlp => "ytdlp",
        Engine::Kuhttp => "kuhttp",
    }
}

fn kind_str(k: Kind) -> &'static str {
    match k {
        Kind::Http => "http",
        Kind::Ftp => "ftp",
        Kind::Sftp => "sftp",
        Kind::Torrent => "torrent",
        Kind::Magnet => "magnet",
        Kind::Metalink => "metalink",
        Kind::Media => "media",
    }
}

fn parse_kind(s: &str) -> Kind {
    serde_json::from_value(serde_json::Value::String(s.into())).unwrap_or(Kind::Http)
}

fn row_to_download(r: &Row) -> rusqlite::Result<Download> {
    let engine: String = r.get("engine")?;
    let kind: String = r.get("kind")?;
    let status: String = r.get("status")?;
    let mirrors: String = r.get("mirrors")?;
    let options: String = r.get("options")?;
    let meta: String = r.get("meta")?;
    Ok(Download {
        id: r.get("id")?,
        engine: match engine.as_str() {
            "ytdlp" => Engine::Ytdlp,
            "kuhttp" => Engine::Kuhttp,
            _ => Engine::Aria2,
        },
        kind: parse_kind(&kind),
        url: r.get("url")?,
        mirrors: serde_json::from_str(&mirrors).unwrap_or_default(),
        name: r.get("name")?,
        dir: r.get("dir")?,
        file_path: r.get("file_path")?,
        status: Status::parse(&status),
        total: r.get("total")?,
        done: r.get("done")?,
        uploaded: r.get("uploaded")?,
        speed: 0,
        upload_speed: 0,
        connections: r.get::<_, i64>("connections")? as u32,
        active_connections: 0,
        category: r.get("category")?,
        queue_id: r.get("queue_id")?,
        position: r.get("position")?,
        created_at: r.get("created_at")?,
        completed_at: r.get("completed_at")?,
        error: r.get("error")?,
        gid: r.get("gid")?,
        source: r.get("source")?,
        options: serde_json::from_str::<DownloadOptions>(&options).unwrap_or_default(),
        meta: serde_json::from_str::<DownloadMeta>(&meta).unwrap_or_default(),
    })
}

impl Db {
    pub fn open(path: &Path) -> Result<Db> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
        }
        let conn = Connection::open(path).with_context(|| format!("opening {}", path.display()))?;
        Self::init(conn)
    }

    pub fn open_in_memory() -> Result<Db> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> Result<Db> {
        conn.execute_batch(
            "PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL; PRAGMA foreign_keys=ON; PRAGMA busy_timeout=5000;",
        )?;
        let version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        for (i, sql) in MIGRATIONS.iter().enumerate().skip(version as usize) {
            let tx = conn.unchecked_transaction()?;
            tx.execute_batch(sql).with_context(|| format!("migration {}", i + 1))?;
            tx.execute_batch(&format!("PRAGMA user_version = {}", i + 1))?;
            tx.commit()?;
        }
        Ok(Db { conn: Mutex::new(conn) })
    }

    fn c(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.conn.lock().unwrap_or_else(|p| p.into_inner())
    }

    pub fn upsert_download(&self, d: &Download) -> Result<()> {
        self.c().execute(
            r#"INSERT INTO downloads (id, engine, kind, url, mirrors, name, dir, file_path, status, total, done,
                uploaded, connections, category, queue_id, position, created_at, completed_at, error, gid, source,
                options, meta)
               VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,?20,?21,?22,?23)
               ON CONFLICT(id) DO UPDATE SET engine=excluded.engine, kind=excluded.kind, url=excluded.url,
                mirrors=excluded.mirrors, name=excluded.name, dir=excluded.dir, file_path=excluded.file_path,
                status=excluded.status, total=excluded.total, done=excluded.done, uploaded=excluded.uploaded,
                connections=excluded.connections, category=excluded.category, queue_id=excluded.queue_id,
                position=excluded.position, completed_at=excluded.completed_at, error=excluded.error,
                gid=excluded.gid, source=excluded.source, options=excluded.options, meta=excluded.meta"#,
            params![
                d.id,
                engine_str(d.engine),
                kind_str(d.kind),
                d.url,
                serde_json::to_string(&d.mirrors)?,
                d.name,
                d.dir,
                d.file_path,
                d.status.as_str(),
                d.total,
                d.done,
                d.uploaded,
                d.connections,
                d.category,
                d.queue_id,
                d.position,
                d.created_at,
                d.completed_at,
                d.error,
                d.gid,
                d.source,
                serde_json::to_string(&d.options)?,
                serde_json::to_string(&d.meta)?,
            ],
        )?;
        Ok(())
    }

    /// Flush live progress for many downloads in one transaction.
    pub fn save_progress(&self, items: &[(String, i64, i64, i64)]) -> Result<()> {
        if items.is_empty() {
            return Ok(());
        }
        let mut c = self.c();
        let tx = c.transaction()?;
        {
            let mut stmt = tx.prepare_cached("UPDATE downloads SET done=?2, total=?3, uploaded=?4 WHERE id=?1")?;
            for (id, done, total, up) in items {
                stmt.execute(params![id, done, total, up])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn delete_downloads(&self, ids: &[String]) -> Result<()> {
        let mut c = self.c();
        let tx = c.transaction()?;
        for id in ids {
            tx.execute("DELETE FROM downloads WHERE id=?1", [id])?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn load_downloads(&self) -> Result<Vec<Download>> {
        let c = self.c();
        let mut stmt = c.prepare("SELECT * FROM downloads ORDER BY created_at")?;
        let rows = stmt.query_map([], row_to_download)?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn kv_get(&self, key: &str) -> Result<Option<String>> {
        Ok(self
            .c()
            .query_row("SELECT value FROM kv WHERE key=?1", [key], |r| r.get(0))
            .optional()?)
    }

    pub fn kv_set(&self, key: &str, value: &str) -> Result<()> {
        self.c().execute(
            "INSERT INTO kv(key, value) VALUES(?1, ?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    pub fn load_settings(&self) -> Result<Settings> {
        let mut s = match self.kv_get("settings")? {
            Some(json) => serde_json::from_str(&json).unwrap_or_default(),
            None => Settings::default(),
        };
        s.sanitize();
        Ok(s)
    }

    pub fn save_settings(&self, s: &Settings) -> Result<()> {
        self.kv_set("settings", &serde_json::to_string(s)?)
    }

    fn load_json_table<T: serde::de::DeserializeOwned>(&self, table: &str) -> Result<Vec<T>> {
        let c = self.c();
        let mut stmt = c.prepare(&format!("SELECT data FROM {table}"))?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        let mut out = Vec::new();
        for json in rows {
            if let Ok(v) = serde_json::from_str(&json?) {
                out.push(v);
            }
        }
        Ok(out)
    }

    fn save_json_row<T: serde::Serialize>(&self, table: &str, id: &str, v: &T) -> Result<()> {
        self.c().execute(
            &format!("INSERT INTO {table}(id, data) VALUES(?1, ?2) ON CONFLICT(id) DO UPDATE SET data=excluded.data"),
            params![id, serde_json::to_string(v)?],
        )?;
        Ok(())
    }

    pub fn load_queues(&self) -> Result<Vec<Queue>> {
        let mut q: Vec<Queue> = self.load_json_table("queues")?;
        if !q.iter().any(|q| q.id == crate::types::MAIN_QUEUE) {
            let main = Queue::default();
            self.save_queue(&main)?;
            q.insert(0, main);
        }
        q.sort_by_key(|q| q.position);
        Ok(q)
    }

    pub fn save_queue(&self, q: &Queue) -> Result<()> {
        self.save_json_row("queues", &q.id, q)
    }

    pub fn delete_queue(&self, id: &str) -> Result<()> {
        self.c().execute("DELETE FROM queues WHERE id=?1", [id])?;
        Ok(())
    }

    pub fn load_schedules(&self) -> Result<Vec<Schedule>> {
        self.load_json_table("schedules")
    }

    pub fn save_schedule(&self, s: &Schedule) -> Result<()> {
        self.save_json_row("schedules", &s.id, s)
    }

    pub fn delete_schedule(&self, id: &str) -> Result<()> {
        self.c().execute("DELETE FROM schedules WHERE id=?1", [id])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    pub fn sample(id: &str) -> Download {
        Download {
            id: id.into(),
            engine: Engine::Aria2,
            kind: Kind::Http,
            url: "https://example.com/a.zip".into(),
            mirrors: vec!["https://mirror.example.com/a.zip".into()],
            name: "a.zip".into(),
            dir: "/tmp".into(),
            file_path: None,
            status: Status::Downloading,
            total: 100,
            done: 10,
            uploaded: 0,
            speed: 5,
            upload_speed: 0,
            connections: 0,
            active_connections: 3,
            category: "archives".into(),
            queue_id: None,
            position: 0,
            created_at: 1,
            completed_at: None,
            error: None,
            gid: Some("0123456789abcdef".into()),
            source: "test".into(),
            options: DownloadOptions { referer: Some("https://example.com".into()), ..Default::default() },
            meta: DownloadMeta::default(),
        }
    }

    #[test]
    fn roundtrip_download_and_progress() {
        let db = Db::open_in_memory().unwrap();
        db.upsert_download(&sample("x")).unwrap();
        db.save_progress(&[("x".into(), 50, 100, 0)]).unwrap();
        let all = db.load_downloads().unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].done, 50);
        assert_eq!(all[0].mirrors.len(), 1);
        assert_eq!(all[0].options.referer.as_deref(), Some("https://example.com"));
        assert_eq!(all[0].status, Status::Downloading);
        db.delete_downloads(&["x".into()]).unwrap();
        assert!(db.load_downloads().unwrap().is_empty());
    }

    #[test]
    fn settings_and_queues() {
        let db = Db::open_in_memory().unwrap();
        let mut s = db.load_settings().unwrap();
        s.max_concurrent = 7;
        db.save_settings(&s).unwrap();
        assert_eq!(db.load_settings().unwrap().max_concurrent, 7);
        let q = db.load_queues().unwrap();
        assert_eq!(q[0].id, "main");
    }
}
