use rusqlite::Connection;

const VERSION: i64 = 2;

pub fn init(conn: &Connection) -> rusqlite::Result<()> {
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    conn.pragma_update(None, "temp_store", "MEMORY")?;
    conn.busy_timeout(std::time::Duration::from_secs(5))?;
    let current: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
    if current >= VERSION {
        return Ok(());
    }
    if current < 1 {
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
    }
    if current < 2 {
        // In-app SDK fields. Each ADD COLUMN is guarded so a half-applied
        // migration can be run again.
        let have: Vec<String> = {
            let mut st = conn.prepare("SELECT name FROM pragma_table_info('flow')")?;
            let rows = st.query_map([], |r| r.get::<_, String>(0))?;
            rows.collect::<Result<_, _>>()?
        };
        let cols: [(&str, &str); 13] = [
            ("source", "TEXT NOT NULL DEFAULT 'proxy'"),
            ("device_id", "TEXT NOT NULL DEFAULT ''"),
            ("app_id", "TEXT NOT NULL DEFAULT ''"),
            ("tag", "TEXT NOT NULL DEFAULT ''"),
            ("name", "TEXT NOT NULL DEFAULT ''"),
            ("owner", "TEXT NOT NULL DEFAULT ''"),
            ("screen", "TEXT NOT NULL DEFAULT ''"),
            ("route_template", "TEXT NOT NULL DEFAULT ''"),
            ("client_stack", "TEXT NOT NULL DEFAULT ''"),
            ("sdk_call_id", "TEXT NOT NULL DEFAULT ''"),
            ("merged_with", "INTEGER NOT NULL DEFAULT 0"),
            ("client_ip", "TEXT NOT NULL DEFAULT ''"),
            ("sdk_json", "TEXT NOT NULL DEFAULT ''"),
        ];
        for (name, ty) in cols {
            if !have.iter().any(|h| h == name) {
                conn.execute_batch(&format!("ALTER TABLE flow ADD COLUMN {name} {ty};"))?;
            }
        }
        conn.execute_batch(
            "CREATE INDEX IF NOT EXISTS flow_device ON flow(device_id, app_id);
            CREATE INDEX IF NOT EXISTS flow_source ON flow(source);
            CREATE INDEX IF NOT EXISTS flow_sdk_call ON flow(sdk_call_id);
            CREATE VIRTUAL TABLE IF NOT EXISTS flow_sdk_fts USING fts5(
                name, tag, owner, screen, route_template, app_id, device,
                tokenize = 'unicode61 remove_diacritics 2'
            );",
        )?;
    }
    conn.pragma_update(None, "user_version", VERSION)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrates_a_version_1_database() {
        let conn = Connection::open_in_memory().unwrap();
        // Build a v1 database by hand, with a row in it.
        conn.execute_batch(
            "CREATE TABLE flow(id INTEGER PRIMARY KEY, started_at INTEGER NOT NULL,
                duration_ms REAL NOT NULL, scheme TEXT NOT NULL, host TEXT NOT NULL,
                port INTEGER NOT NULL, method TEXT NOT NULL, path TEXT NOT NULL,
                query TEXT NOT NULL, status INTEGER NOT NULL, protocol TEXT NOT NULL,
                app TEXT NOT NULL, req_size INTEGER NOT NULL, res_size INTEGER NOT NULL,
                req_blob TEXT, res_blob TEXT, req_headers TEXT NOT NULL,
                res_headers TEXT NOT NULL, timings_json TEXT NOT NULL, flags INTEGER NOT NULL,
                state INTEGER NOT NULL, error TEXT NOT NULL, content_type TEXT NOT NULL,
                tags TEXT NOT NULL DEFAULT '');
             CREATE VIRTUAL TABLE flow_fts USING fts5(host, path, req_headers, res_headers, body_text);
             INSERT INTO flow VALUES(7,1,1.0,'https','a.test',443,'GET','/','',200,'h2','',0,0,NULL,NULL,'[]','[]','{}',0,2,'','json','');
             PRAGMA user_version = 1;",
        )
        .unwrap();
        init(&conn).unwrap();
        init(&conn).unwrap(); // idempotent
        let (source, merged): (String, i64) = conn
            .query_row(
                "SELECT source, merged_with FROM flow WHERE id = 7",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(source, "proxy");
        assert_eq!(merged, 0);
        let v: i64 = conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(v, 2);
    }
}
