use rusqlite::Connection;

const VERSION: i64 = 1;

pub fn init(conn: &Connection) -> rusqlite::Result<()> {
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    conn.pragma_update(None, "temp_store", "MEMORY")?;
    conn.busy_timeout(std::time::Duration::from_secs(5))?;
    let current: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if current >= VERSION {
        return Ok(());
    }
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS flow(
            id INTEGER PRIMARY KEY,
            started_at INTEGER NOT NULL,
            duration_ms REAL NOT NULL,
            scheme TEXT NOT NULL,
            host TEXT NOT NULL,
            port INTEGER NOT NULL,
            method TEXT NOT NULL,
            path TEXT NOT NULL,
            query TEXT NOT NULL,
            status INTEGER NOT NULL,
            protocol TEXT NOT NULL,
            app TEXT NOT NULL,
            req_size INTEGER NOT NULL,
            res_size INTEGER NOT NULL,
            req_blob TEXT,
            res_blob TEXT,
            req_headers TEXT NOT NULL,
            res_headers TEXT NOT NULL,
            timings_json TEXT NOT NULL,
            flags INTEGER NOT NULL,
            state INTEGER NOT NULL,
            error TEXT NOT NULL,
            content_type TEXT NOT NULL,
            tags TEXT NOT NULL DEFAULT ''
        );
        CREATE INDEX IF NOT EXISTS flow_started ON flow(started_at);
        CREATE INDEX IF NOT EXISTS flow_host ON flow(host);
        CREATE VIRTUAL TABLE IF NOT EXISTS flow_fts USING fts5(
            host, path, req_headers, res_headers, body_text,
            tokenize = 'unicode61 remove_diacritics 2'
        );",
    )?;
    conn.pragma_update(None, "user_version", VERSION)?;
    Ok(())
}
