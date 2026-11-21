//! Command history behind `shellcraft hist`, stored in SQLite (`history.db`, WAL mode so several
//! shells can write at once). An older `history.tsv` is imported the first time the database is
//! created. Lookups use indexes, so a suggestion costs the same with 200k commands as with 2k.

use std::fs;
use std::path::Path;
use std::time::Duration;

use anyhow::Result;
use rusqlite::{params, params_from_iter, types::Value, Connection};

/// One executed command.
#[derive(Debug, Clone, PartialEq)]
pub struct Record {
    pub when: u64,
    pub exit: i32,
    pub duration_ms: u64,
    pub cwd: String,
    pub command: String,
}

fn unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut it = s.chars();
    while let Some(c) = it.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match it.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some(o) => out.push(o),
            None => out.push('\\'),
        }
    }
    out
}

/// Parse one line of the old `history.tsv` (`epoch<TAB>exit<TAB>ms<TAB>cwd<TAB>command`).
fn parse_legacy_line(line: &str) -> Option<Record> {
    let mut f = line.splitn(5, '\t');
    Some(Record {
        when: f.next()?.parse().ok()?,
        exit: f.next()?.parse().ok()?,
        duration_ms: f.next()?.parse().ok()?,
        cwd: unescape(f.next()?),
        command: unescape(f.next().filter(|c| !c.is_empty())?),
    })
}

/// Filters for [`Db::search`].
#[derive(Debug, Default)]
pub struct Query<'a> {
    /// Every term must appear in the command (case-insensitive for ASCII).
    pub terms: &'a [String],
    /// Only commands run in this directory.
    pub cwd: Option<&'a str>,
    /// Only commands that exited with 0.
    pub ok_only: bool,
    /// Collapse repeated commands, keeping the newest.
    pub unique: bool,
    /// Maximum results (0 = no limit).
    pub limit: usize,
}

/// `(total, failed, top commands by count)`.
pub type Stats = (u64, u64, Vec<(String, u64)>);

pub struct Db {
    conn: Connection,
}

impl Db {
    /// Open (and on first use create) the database at `path`.
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        let conn = Connection::open(path)?;
        conn.busy_timeout(Duration::from_secs(2))?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "NORMAL")?;
        let db = Self { conn };
        let version: i64 = db.conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if version == 0 {
            db.conn.execute_batch(
                "CREATE TABLE IF NOT EXISTS history(
                     id INTEGER PRIMARY KEY, ts INTEGER NOT NULL, exit INTEGER NOT NULL,
                     dur INTEGER NOT NULL, cwd TEXT NOT NULL, cmd TEXT NOT NULL);
                 CREATE INDEX IF NOT EXISTS history_cmd ON history(cmd);
                 CREATE INDEX IF NOT EXISTS history_cwd ON history(cwd);
                 PRAGMA user_version = 1;",
            )?;
            db.import_legacy(&path.with_extension("tsv"))?;
        }
        Ok(db)
    }

    fn import_legacy(&self, tsv: &Path) -> Result<()> {
        let Ok(bytes) = fs::read(tsv) else {
            return Ok(());
        };
        let text = String::from_utf8_lossy(&bytes);
        self.conn.execute_batch("BEGIN")?;
        for r in text.lines().filter_map(parse_legacy_line) {
            self.add(&r)?;
        }
        self.conn.execute_batch("COMMIT")?;
        let _ = fs::rename(tsv, tsv.with_extension("tsv.imported"));
        Ok(())
    }

    pub fn add(&self, r: &Record) -> Result<()> {
        self.conn.execute(
            "INSERT INTO history(ts, exit, dur, cwd, cmd) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                r.when as i64,
                r.exit,
                r.duration_ms as i64,
                r.cwd,
                r.command
            ],
        )?;
        Ok(())
    }

    /// Newest-first records matching `q`.
    pub fn search(&self, q: &Query) -> Result<Vec<Record>> {
        let mut conds = vec!["1".to_string()];
        let mut args: Vec<Value> = Vec::new();
        if q.ok_only {
            conds.push("exit = 0".into());
        }
        if let Some(c) = q.cwd {
            conds.push("cwd = ?".into());
            args.push(c.to_string().into());
        }
        for t in q.terms {
            conds.push("instr(lower(cmd), ?) > 0".into());
            args.push(t.to_lowercase().into());
        }
        let filter = conds.join(" AND ");
        let limit = if q.limit == 0 {
            String::new()
        } else {
            format!(" LIMIT {}", q.limit)
        };
        let sql = if q.unique {
            format!(
                "SELECT ts, exit, dur, cwd, cmd FROM history WHERE id IN \
                 (SELECT MAX(id) FROM history WHERE {filter} GROUP BY cmd) ORDER BY id DESC{limit}"
            )
        } else {
            format!("SELECT ts, exit, dur, cwd, cmd FROM history WHERE {filter} ORDER BY id DESC{limit}")
        };
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params_from_iter(args), |r| {
            Ok(Record {
                when: r.get::<_, i64>(0)? as u64,
                exit: r.get(1)?,
                duration_ms: r.get::<_, i64>(2)? as u64,
                cwd: r.get(3)?,
                command: r.get(4)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// Best completion of `prefix`: the newest successful single-line command that starts with it
    /// and is longer. Commands run in `cwd` win over commands from elsewhere.
    pub fn suggest(&self, prefix: &str, cwd: Option<&str>) -> Result<Option<String>> {
        if prefix.is_empty() || prefix.contains('\n') {
            return Ok(None);
        }
        // index range scan: prefix <= cmd < prefix + the largest code point
        let upper = format!("{prefix}\u{10FFFF}");
        let mut stmt = self.conn.prepare_cached(
            "SELECT cmd FROM history
             WHERE cmd >= ?1 AND cmd < ?2 AND exit = 0 AND length(cmd) > length(?1)
               AND instr(cmd, char(10)) = 0
             ORDER BY (cwd = ?3) DESC, id DESC LIMIT 1",
        )?;
        let mut rows = stmt.query(params![prefix, upper, cwd.unwrap_or("")])?;
        Ok(rows.next()?.map(|r| r.get(0)).transpose()?)
    }

    /// Successful single-line commands as `(cwd, command)`, newest first, each pair once: what the
    /// zsh line editor keeps in memory to suggest without starting a process.
    pub fn recent(&self, limit: usize) -> Result<Vec<(String, String)>> {
        let mut stmt = self.conn.prepare(
            "SELECT cwd, cmd FROM history
             WHERE exit = 0 AND instr(cmd, char(10)) = 0 AND instr(cmd, char(9)) = 0 AND instr(cwd, char(9)) = 0
             GROUP BY cwd, cmd ORDER BY MAX(id) DESC LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit as i64], |r| Ok((r.get(0)?, r.get(1)?)))?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    /// `(total, failed, top 10 commands by count)`.
    pub fn stats(&self) -> Result<Stats> {
        let (total, failed): (i64, i64) = self.conn.query_row(
            "SELECT COUNT(*), COALESCE(SUM(exit != 0), 0) FROM history",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        let mut stmt = self.conn.prepare(
            "SELECT cmd, COUNT(*) AS n FROM history GROUP BY cmd ORDER BY n DESC, cmd ASC LIMIT 10",
        )?;
        let top = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get::<_, i64>(1)? as u64)))?
            .collect::<std::result::Result<_, _>>()?;
        Ok((total as u64, failed as u64, top))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("sc-hist-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        d
    }

    fn rec(when: u64, exit: i32, cwd: &str, cmd: &str) -> Record {
        Record {
            when,
            exit,
            duration_ms: 5,
            cwd: cwd.into(),
            command: cmd.into(),
        }
    }

    fn db_with(name: &str, all: &[Record]) -> Db {
        let db = Db::open(&temp(name).join("h.db")).expect("open");
        for r in all {
            db.add(r).expect("add");
        }
        db
    }

    #[test]
    fn search_filters_and_orders_newest_first() {
        let db = db_with(
            "search",
            &[
                rec(1, 0, "/a", "git status"),
                rec(2, 1, "/a", "git push"),
                rec(3, 0, "/b", "git status"),
                rec(4, 0, "/a", "ls"),
            ],
        );
        let terms = vec!["GIT".to_string()];
        let q = Query {
            terms: &terms,
            unique: true,
            ..Query::default()
        };
        let hits = db.search(&q).expect("search");
        assert_eq!(hits.iter().map(|r| r.when).collect::<Vec<_>>(), vec![3, 2]);
        let q = Query {
            terms: &terms,
            ok_only: true,
            cwd: Some("/a"),
            ..Query::default()
        };
        assert_eq!(db.search(&q).expect("search").len(), 1);
        let q = Query {
            limit: 2,
            ..Query::default()
        };
        assert_eq!(db.search(&q).expect("search").len(), 2);
    }

    #[test]
    fn suggest_prefers_cwd_success_and_longer() {
        let db = db_with(
            "suggest",
            &[
                rec(1, 0, "/a", "git status --short"),
                rec(2, 0, "/b", "git stash"),
                rec(3, 1, "/b", "git switch main"),
                rec(4, 0, "/a", "git log\nmore"),
            ],
        );
        let s = |p: &str, c: Option<&str>| db.suggest(p, c).expect("suggest");
        assert_eq!(
            s("git s", Some("/a")).as_deref(),
            Some("git status --short")
        );
        assert_eq!(
            s("git s", Some("/b")).as_deref(),
            Some("git stash"),
            "failed commands are skipped"
        );
        assert_eq!(
            s("git s", None).as_deref(),
            Some("git stash"),
            "newest otherwise"
        );
        assert_eq!(s("git stash", None), None, "must be longer than the prefix");
        assert_eq!(s("", None), None);
        assert_eq!(
            s("git l", None),
            None,
            "multi-line commands are not offered"
        );
        assert_eq!(s("git %", None), None, "no wildcard behaviour");
    }

    #[test]
    fn special_characters_roundtrip_and_stats() {
        let db = db_with(
            "special",
            &[
                rec(1, 0, "/a\tb", "echo 'a\\b'\nls\there"),
                rec(2, 1, "/", "x"),
                rec(3, 0, "/", "x"),
            ],
        );
        let all = db.search(&Query::default()).expect("search");
        assert_eq!(all[2], rec(1, 0, "/a\tb", "echo 'a\\b'\nls\there"));
        let recent = db.recent(10).expect("recent");
        assert_eq!(
            recent,
            vec![("/".to_string(), "x".to_string())],
            "failures, multi-line and tab commands are left out"
        );
        let (total, failed, top) = db.stats().expect("stats");
        assert_eq!((total, failed), (3, 1));
        assert_eq!(top[0], ("x".to_string(), 2));
    }

    #[test]
    fn imports_the_old_tsv_once() {
        let dir = temp("import");
        fs::create_dir_all(&dir).expect("mkdir");
        fs::write(
            dir.join("h.tsv"),
            "1\t0\t5\t/a\tgit status\njunk\n2\t1\t5\t/a\tls\n",
        )
        .expect("write");
        let db = Db::open(&dir.join("h.db")).expect("open");
        assert_eq!(db.stats().expect("stats").0, 2);
        assert!(!dir.join("h.tsv").exists() && dir.join("h.tsv.imported").exists());
        drop(db);
        let db = Db::open(&dir.join("h.db")).expect("reopen");
        assert_eq!(db.stats().expect("stats").0, 2, "not imported twice");
    }
}
