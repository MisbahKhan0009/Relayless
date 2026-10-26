//! SQLite persistence. WAL mode; chunk state is only committed after the
//! corresponding file data has been flushed, so the database never claims
//! more progress than is durably on disk.

use std::path::Path;

use anyhow::Result;
use parking_lot::Mutex;
use rusqlite::{params, Connection, OptionalExtension};

use crate::types::{ActivityDto, JobStatus, Settings};
use crate::util::now_ms;

pub struct Db {
    conn: Mutex<Connection>,
}

#[derive(Clone, Debug)]
pub struct ShareRow {
    pub id: String,
    pub root_path: String,
    pub name: String,
    pub active: bool,
    pub created_at: i64,
}

#[derive(Clone, Debug)]
pub struct InvitationRow {
    pub id: String,
    pub share_id: String,
    pub token_hash: String,
    pub expires_at: i64,
    pub max_uses: Option<u32>,
    pub use_count: u32,
    pub require_approval: bool,
    pub revoked_at: Option<i64>,
    pub created_at: i64,
}

impl InvitationRow {
    pub fn state(&self, now: i64) -> &'static str {
        if self.revoked_at.is_some() {
            "revoked"
        } else if self.expires_at <= now {
            "expired"
        } else if self.max_uses.is_some_and(|m| self.use_count >= m) {
            "used_up"
        } else {
            "active"
        }
    }
}

#[derive(Clone, Debug)]
pub struct GrantRow {
    pub peer_id: String,
    pub share_id: String,
    pub device_name: String,
    pub created_at: i64,
    pub last_seen_at: i64,
}

#[derive(Clone, Debug)]
pub struct RemoteRow {
    pub peer_id: String,
    pub name: String,
    pub relay_url: Option<String>,
    pub created_at: i64,
    pub last_seen_at: i64,
}

#[derive(Clone, Debug)]
pub struct JobRow {
    pub id: String,
    pub peer_id: String,
    pub share_id: String,
    pub remote_path: Vec<String>,
    pub display_name: String,
    pub size: u64,
    pub mtime_ms: i64,
    pub chunk_size: u64,
    pub total_chunks: u64,
    pub dest_path: String,
    pub temp_path: String,
    pub verified_bytes: u64,
    pub status: JobStatus,
    pub error: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
    pub completed_at: Option<i64>,
}

const SCHEMA_V1: &str = r#"
CREATE TABLE IF NOT EXISTS settings(key TEXT PRIMARY KEY, value TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS shares(
  id TEXT PRIMARY KEY, root_path TEXT NOT NULL, display_name TEXT NOT NULL,
  active INTEGER NOT NULL DEFAULT 1, created_at INTEGER NOT NULL);
CREATE TABLE IF NOT EXISTS invitations(
  id TEXT PRIMARY KEY, share_id TEXT NOT NULL, token_hash TEXT NOT NULL UNIQUE,
  expires_at INTEGER NOT NULL, max_uses INTEGER, use_count INTEGER NOT NULL DEFAULT 0,
  require_approval INTEGER NOT NULL DEFAULT 1, revoked_at INTEGER, created_at INTEGER NOT NULL);
CREATE TABLE IF NOT EXISTS grants(
  peer_id TEXT NOT NULL, share_id TEXT NOT NULL, device_name TEXT NOT NULL,
  created_at INTEGER NOT NULL, last_seen_at INTEGER NOT NULL, PRIMARY KEY(peer_id, share_id));
CREATE TABLE IF NOT EXISTS remotes(
  peer_id TEXT PRIMARY KEY, name TEXT NOT NULL, relay_url TEXT,
  created_at INTEGER NOT NULL, last_seen_at INTEGER NOT NULL);
CREATE TABLE IF NOT EXISTS remote_shares(
  peer_id TEXT NOT NULL, share_id TEXT NOT NULL, name TEXT NOT NULL, PRIMARY KEY(peer_id, share_id));
CREATE TABLE IF NOT EXISTS jobs(
  id TEXT PRIMARY KEY, peer_id TEXT NOT NULL, share_id TEXT NOT NULL, remote_path TEXT NOT NULL,
  display_name TEXT NOT NULL, size INTEGER NOT NULL, mtime_ms INTEGER NOT NULL,
  chunk_size INTEGER NOT NULL, total_chunks INTEGER NOT NULL, dest_path TEXT NOT NULL,
  temp_path TEXT NOT NULL, verified_bytes INTEGER NOT NULL DEFAULT 0, status TEXT NOT NULL,
  error TEXT, created_at INTEGER NOT NULL, updated_at INTEGER NOT NULL, completed_at INTEGER);
CREATE TABLE IF NOT EXISTS chunks(
  job_id TEXT NOT NULL, idx INTEGER NOT NULL, hash BLOB NOT NULL, PRIMARY KEY(job_id, idx)) WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS activity(
  id INTEGER PRIMARY KEY AUTOINCREMENT, ts INTEGER NOT NULL, level TEXT NOT NULL,
  message TEXT NOT NULL, job_id TEXT);
"#;

impl Db {
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        let version: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
        if version < 1 {
            conn.execute_batch(SCHEMA_V1)?;
            conn.pragma_update(None, "user_version", 1)?;
        }
        Ok(Db { conn: Mutex::new(conn) })
    }

    // ---------------------------------------------------------------- settings

    pub fn load_settings(&self) -> Option<Settings> {
        let c = self.conn.lock();
        let v: Option<String> = c
            .query_row("SELECT value FROM settings WHERE key='settings'", [], |r| r.get(0))
            .optional()
            .ok()
            .flatten();
        v.and_then(|s| serde_json::from_str(&s).ok())
    }

    pub fn save_settings(&self, s: &Settings) -> Result<()> {
        let c = self.conn.lock();
        c.execute(
            "INSERT INTO settings(key,value) VALUES('settings',?1)
             ON CONFLICT(key) DO UPDATE SET value=excluded.value",
            params![serde_json::to_string(s)?],
        )?;
        Ok(())
    }

    // ---------------------------------------------------------------- shares

    pub fn insert_share(&self, s: &ShareRow) -> Result<()> {
        self.conn.lock().execute(
            "INSERT INTO shares(id,root_path,display_name,active,created_at) VALUES(?1,?2,?3,?4,?5)",
            params![s.id, s.root_path, s.name, s.active, s.created_at],
        )?;
        Ok(())
    }

    pub fn list_shares(&self) -> Result<Vec<ShareRow>> {
        let c = self.conn.lock();
        let mut st = c.prepare(
            "SELECT id,root_path,display_name,active,created_at FROM shares ORDER BY created_at",
        )?;
        let rows = st
            .query_map([], |r| {
                Ok(ShareRow {
                    id: r.get(0)?,
                    root_path: r.get(1)?,
                    name: r.get(2)?,
                    active: r.get(3)?,
                    created_at: r.get(4)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    pub fn get_share(&self, id: &str) -> Result<Option<ShareRow>> {
        Ok(self.list_shares()?.into_iter().find(|s| s.id == id))
    }

    pub fn set_share_active(&self, id: &str, active: bool) -> Result<()> {
        self.conn
            .lock()
            .execute("UPDATE shares SET active=?2 WHERE id=?1", params![id, active])?;
        Ok(())
    }

    pub fn delete_share(&self, id: &str) -> Result<()> {
        let mut c = self.conn.lock();
        let tx = c.transaction()?;
        tx.execute("DELETE FROM shares WHERE id=?1", params![id])?;
        tx.execute("DELETE FROM invitations WHERE share_id=?1", params![id])?;
        tx.execute("DELETE FROM grants WHERE share_id=?1", params![id])?;
        tx.commit()?;
        Ok(())
    }

    // ---------------------------------------------------------------- invitations

    pub fn insert_invitation(&self, i: &InvitationRow) -> Result<()> {
        self.conn.lock().execute(
            "INSERT INTO invitations(id,share_id,token_hash,expires_at,max_uses,use_count,require_approval,revoked_at,created_at)
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)",
            params![
                i.id,
                i.share_id,
                i.token_hash,
                i.expires_at,
                i.max_uses,
                i.use_count,
                i.require_approval,
                i.revoked_at,
                i.created_at
            ],
        )?;
        Ok(())
    }

    fn map_invitation(r: &rusqlite::Row) -> rusqlite::Result<InvitationRow> {
        Ok(InvitationRow {
            id: r.get(0)?,
            share_id: r.get(1)?,
            token_hash: r.get(2)?,
            expires_at: r.get(3)?,
            max_uses: r.get(4)?,
            use_count: r.get(5)?,
            require_approval: r.get(6)?,
            revoked_at: r.get(7)?,
            created_at: r.get(8)?,
        })
    }

    pub fn list_invitations(&self) -> Result<Vec<InvitationRow>> {
        let c = self.conn.lock();
        let mut st = c.prepare(
            "SELECT id,share_id,token_hash,expires_at,max_uses,use_count,require_approval,revoked_at,created_at
             FROM invitations ORDER BY created_at DESC",
        )?;
        let rows = st
            .query_map([], Self::map_invitation)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    pub fn find_invitation_by_hash(&self, token_hash: &str) -> Result<Option<InvitationRow>> {
        let c = self.conn.lock();
        Ok(c.query_row(
            "SELECT id,share_id,token_hash,expires_at,max_uses,use_count,require_approval,revoked_at,created_at
             FROM invitations WHERE token_hash=?1",
            params![token_hash],
            Self::map_invitation,
        )
        .optional()?)
    }

    pub fn increment_invitation_use(&self, id: &str) -> Result<()> {
        self.conn.lock().execute(
            "UPDATE invitations SET use_count=use_count+1 WHERE id=?1",
            params![id],
        )?;
        Ok(())
    }

    pub fn revoke_invitation(&self, id: &str) -> Result<()> {
        self.conn.lock().execute(
            "UPDATE invitations SET revoked_at=?2 WHERE id=?1 AND revoked_at IS NULL",
            params![id, now_ms()],
        )?;
        Ok(())
    }

    // ---------------------------------------------------------------- grants

    pub fn upsert_grant(&self, peer_id: &str, share_id: &str, device_name: &str) -> Result<()> {
        let now = now_ms();
        self.conn.lock().execute(
            "INSERT INTO grants(peer_id,share_id,device_name,created_at,last_seen_at) VALUES(?1,?2,?3,?4,?4)
             ON CONFLICT(peer_id,share_id) DO UPDATE SET device_name=excluded.device_name, last_seen_at=excluded.last_seen_at",
            params![peer_id, share_id, device_name, now],
        )?;
        Ok(())
    }

    pub fn list_grants(&self) -> Result<Vec<GrantRow>> {
        let c = self.conn.lock();
        let mut st = c.prepare(
            "SELECT peer_id,share_id,device_name,created_at,last_seen_at FROM grants ORDER BY last_seen_at DESC",
        )?;
        let rows = st
            .query_map([], |r| {
                Ok(GrantRow {
                    peer_id: r.get(0)?,
                    share_id: r.get(1)?,
                    device_name: r.get(2)?,
                    created_at: r.get(3)?,
                    last_seen_at: r.get(4)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    pub fn touch_grants(&self, peer_id: &str) -> Result<()> {
        self.conn.lock().execute(
            "UPDATE grants SET last_seen_at=?2 WHERE peer_id=?1",
            params![peer_id, now_ms()],
        )?;
        Ok(())
    }

    pub fn delete_grants_for_peer(&self, peer_id: &str) -> Result<()> {
        self.conn
            .lock()
            .execute("DELETE FROM grants WHERE peer_id=?1", params![peer_id])?;
        Ok(())
    }

    // ---------------------------------------------------------------- remotes

    pub fn upsert_remote(&self, peer_id: &str, name: &str, relay_url: Option<&str>) -> Result<()> {
        let now = now_ms();
        self.conn.lock().execute(
            "INSERT INTO remotes(peer_id,name,relay_url,created_at,last_seen_at) VALUES(?1,?2,?3,?4,?4)
             ON CONFLICT(peer_id) DO UPDATE SET name=excluded.name,
               relay_url=COALESCE(excluded.relay_url, remotes.relay_url), last_seen_at=excluded.last_seen_at",
            params![peer_id, name, relay_url, now],
        )?;
        Ok(())
    }

    pub fn set_remote_shares(&self, peer_id: &str, shares: &[(String, String)]) -> Result<()> {
        let mut c = self.conn.lock();
        let tx = c.transaction()?;
        tx.execute("DELETE FROM remote_shares WHERE peer_id=?1", params![peer_id])?;
        for (id, name) in shares {
            tx.execute(
                "INSERT INTO remote_shares(peer_id,share_id,name) VALUES(?1,?2,?3)",
                params![peer_id, id, name],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    pub fn list_remotes(&self) -> Result<Vec<(RemoteRow, Vec<(String, String)>)>> {
        let c = self.conn.lock();
        let mut st = c.prepare(
            "SELECT peer_id,name,relay_url,created_at,last_seen_at FROM remotes ORDER BY last_seen_at DESC",
        )?;
        let remotes = st
            .query_map([], |r| {
                Ok(RemoteRow {
                    peer_id: r.get(0)?,
                    name: r.get(1)?,
                    relay_url: r.get(2)?,
                    created_at: r.get(3)?,
                    last_seen_at: r.get(4)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        let mut st2 = c.prepare("SELECT share_id,name FROM remote_shares WHERE peer_id=?1 ORDER BY name")?;
        let mut out = Vec::new();
        for r in remotes {
            let shares = st2
                .query_map(params![r.peer_id], |row| Ok((row.get(0)?, row.get(1)?)))?
                .collect::<rusqlite::Result<Vec<_>>>()?;
            out.push((r, shares));
        }
        Ok(out)
    }

    pub fn get_remote(&self, peer_id: &str) -> Result<Option<RemoteRow>> {
        let c = self.conn.lock();
        Ok(c.query_row(
            "SELECT peer_id,name,relay_url,created_at,last_seen_at FROM remotes WHERE peer_id=?1",
            params![peer_id],
            |r| {
                Ok(RemoteRow {
                    peer_id: r.get(0)?,
                    name: r.get(1)?,
                    relay_url: r.get(2)?,
                    created_at: r.get(3)?,
                    last_seen_at: r.get(4)?,
                })
            },
        )
        .optional()?)
    }

    pub fn delete_remote(&self, peer_id: &str) -> Result<()> {
        let mut c = self.conn.lock();
        let tx = c.transaction()?;
        tx.execute("DELETE FROM remotes WHERE peer_id=?1", params![peer_id])?;
        tx.execute("DELETE FROM remote_shares WHERE peer_id=?1", params![peer_id])?;
        tx.commit()?;
        Ok(())
    }

    // ---------------------------------------------------------------- jobs

    pub fn insert_job(&self, j: &JobRow) -> Result<()> {
        self.conn.lock().execute(
            "INSERT INTO jobs(id,peer_id,share_id,remote_path,display_name,size,mtime_ms,chunk_size,total_chunks,
              dest_path,temp_path,verified_bytes,status,error,created_at,updated_at,completed_at)
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17)",
            params![
                j.id,
                j.peer_id,
                j.share_id,
                serde_json::to_string(&j.remote_path)?,
                j.display_name,
                j.size as i64,
                j.mtime_ms,
                j.chunk_size as i64,
                j.total_chunks as i64,
                j.dest_path,
                j.temp_path,
                j.verified_bytes as i64,
                j.status.as_str(),
                j.error,
                j.created_at,
                j.updated_at,
                j.completed_at
            ],
        )?;
        Ok(())
    }

    fn map_job(r: &rusqlite::Row) -> rusqlite::Result<JobRow> {
        let path: String = r.get(3)?;
        let status: String = r.get(12)?;
        Ok(JobRow {
            id: r.get(0)?,
            peer_id: r.get(1)?,
            share_id: r.get(2)?,
            remote_path: serde_json::from_str(&path).unwrap_or_default(),
            display_name: r.get(4)?,
            size: r.get::<_, i64>(5)? as u64,
            mtime_ms: r.get(6)?,
            chunk_size: r.get::<_, i64>(7)? as u64,
            total_chunks: r.get::<_, i64>(8)? as u64,
            dest_path: r.get(9)?,
            temp_path: r.get(10)?,
            verified_bytes: r.get::<_, i64>(11)? as u64,
            status: JobStatus::parse(&status),
            error: r.get(13)?,
            created_at: r.get(14)?,
            updated_at: r.get(15)?,
            completed_at: r.get(16)?,
        })
    }

    const JOB_COLS: &'static str = "id,peer_id,share_id,remote_path,display_name,size,mtime_ms,chunk_size,total_chunks,
        dest_path,temp_path,verified_bytes,status,error,created_at,updated_at,completed_at";

    pub fn get_job(&self, id: &str) -> Result<Option<JobRow>> {
        let c = self.conn.lock();
        Ok(c.query_row(
            &format!("SELECT {} FROM jobs WHERE id=?1", Self::JOB_COLS),
            params![id],
            Self::map_job,
        )
        .optional()?)
    }

    pub fn list_jobs(&self) -> Result<Vec<JobRow>> {
        let c = self.conn.lock();
        let mut st = c.prepare(&format!(
            "SELECT {} FROM jobs ORDER BY created_at DESC",
            Self::JOB_COLS
        ))?;
        let rows = st
            .query_map([], Self::map_job)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    pub fn set_job_status(&self, id: &str, status: JobStatus, error: Option<&str>) -> Result<()> {
        self.conn.lock().execute(
            "UPDATE jobs SET status=?2, error=?3, updated_at=?4 WHERE id=?1",
            params![id, status.as_str(), error, now_ms()],
        )?;
        Ok(())
    }

    pub fn set_job_completed(&self, id: &str, dest_path: &str) -> Result<()> {
        let now = now_ms();
        self.conn.lock().execute(
            "UPDATE jobs SET status='completed', error=NULL, dest_path=?2, updated_at=?3, completed_at=?3 WHERE id=?1",
            params![id, dest_path, now],
        )?;
        Ok(())
    }

    /// Points a job at a new version of its source file and drops all progress.
    pub fn update_job_source(&self, id: &str, size: u64, mtime_ms: i64, total_chunks: u64) -> Result<()> {
        let mut c = self.conn.lock();
        let tx = c.transaction()?;
        tx.execute(
            "UPDATE jobs SET size=?2, mtime_ms=?3, total_chunks=?4, verified_bytes=0, status='queued', error=NULL, updated_at=?5 WHERE id=?1",
            params![id, size as i64, mtime_ms, total_chunks as i64, now_ms()],
        )?;
        tx.execute("DELETE FROM chunks WHERE job_id=?1", params![id])?;
        tx.commit()?;
        Ok(())
    }

    pub fn delete_job(&self, id: &str) -> Result<()> {
        let mut c = self.conn.lock();
        let tx = c.transaction()?;
        tx.execute("DELETE FROM jobs WHERE id=?1", params![id])?;
        tx.execute("DELETE FROM chunks WHERE job_id=?1", params![id])?;
        tx.commit()?;
        Ok(())
    }

    pub fn verified_chunks(&self, job_id: &str) -> Result<Vec<(u64, [u8; 32])>> {
        let c = self.conn.lock();
        let mut st = c.prepare("SELECT idx,hash FROM chunks WHERE job_id=?1 ORDER BY idx")?;
        let rows = st
            .query_map(params![job_id], |r| {
                let idx: i64 = r.get(0)?;
                let h: Vec<u8> = r.get(1)?;
                let mut hash = [0u8; 32];
                if h.len() == 32 {
                    hash.copy_from_slice(&h);
                }
                Ok((idx as u64, hash))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }

    /// Atomically records verified chunks and the new verified byte count.
    pub fn commit_chunks(&self, job_id: &str, chunks: &[(u64, [u8; 32])], verified_bytes: u64) -> Result<()> {
        let mut c = self.conn.lock();
        let tx = c.transaction()?;
        {
            let mut st = tx.prepare(
                "INSERT INTO chunks(job_id,idx,hash) VALUES(?1,?2,?3)
                 ON CONFLICT(job_id,idx) DO UPDATE SET hash=excluded.hash",
            )?;
            for (idx, hash) in chunks {
                st.execute(params![job_id, *idx as i64, &hash[..]])?;
            }
        }
        tx.execute(
            "UPDATE jobs SET verified_bytes=?2, updated_at=?3 WHERE id=?1",
            params![job_id, verified_bytes as i64, now_ms()],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn remove_chunks(&self, job_id: &str, idxs: &[u64], verified_bytes: u64) -> Result<()> {
        let mut c = self.conn.lock();
        let tx = c.transaction()?;
        for idx in idxs {
            tx.execute(
                "DELETE FROM chunks WHERE job_id=?1 AND idx=?2",
                params![job_id, *idx as i64],
            )?;
        }
        tx.execute(
            "UPDATE jobs SET verified_bytes=?2 WHERE id=?1",
            params![job_id, verified_bytes as i64],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn clear_chunks(&self, job_id: &str) -> Result<()> {
        self.remove_all_chunks(job_id)
    }

    fn remove_all_chunks(&self, job_id: &str) -> Result<()> {
        let mut c = self.conn.lock();
        let tx = c.transaction()?;
        tx.execute("DELETE FROM chunks WHERE job_id=?1", params![job_id])?;
        tx.execute("UPDATE jobs SET verified_bytes=0 WHERE id=?1", params![job_id])?;
        tx.commit()?;
        Ok(())
    }

    // ---------------------------------------------------------------- activity

    pub fn add_activity(&self, level: &str, message: &str, job_id: Option<&str>) -> Result<ActivityDto> {
        let c = self.conn.lock();
        let ts = now_ms();
        c.execute(
            "INSERT INTO activity(ts,level,message,job_id) VALUES(?1,?2,?3,?4)",
            params![ts, level, message, job_id],
        )?;
        let id = c.last_insert_rowid();
        // Keep the log bounded.
        if id % 200 == 0 {
            c.execute("DELETE FROM activity WHERE id <= ?1", params![id - 2000])?;
        }
        Ok(ActivityDto {
            id,
            ts,
            level: level.into(),
            message: message.into(),
            job_id: job_id.map(|s| s.to_string()),
        })
    }

    pub fn list_activity(&self, limit: u32) -> Result<Vec<ActivityDto>> {
        let c = self.conn.lock();
        let mut st = c.prepare(
            "SELECT id,ts,level,message,job_id FROM activity ORDER BY id DESC LIMIT ?1",
        )?;
        let rows = st
            .query_map(params![limit], |r| {
                Ok(ActivityDto {
                    id: r.get(0)?,
                    ts: r.get(1)?,
                    level: r.get(2)?,
                    message: r.get(3)?,
                    job_id: r.get(4)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(rows)
    }
}
