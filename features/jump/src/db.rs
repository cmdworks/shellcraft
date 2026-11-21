//! Frecency directory database behind `shellcraft jump`.
//!
//! Plain text file, one entry per line: `path<TAB>rank<TAB>last_used_epoch_secs`.
//! No SQL, no dependencies; a few hundred entries load in well under a millisecond.

use std::fs;
use std::io;
use std::path::Path;

/// Total rank above which all ranks are scaled down so old entries fade out.
const MAX_TOTAL_RANK: f64 = 10_000.0;
const HOUR: u64 = 3600;
const DAY: u64 = 24 * HOUR;
const WEEK: u64 = 7 * DAY;

/// One remembered directory.
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub path: String,
    pub rank: f64,
    pub last: u64,
}

impl Entry {
    /// Frecency: rank weighted by how recently the directory was used.
    pub fn score(&self, now: u64) -> f64 {
        let age = now.saturating_sub(self.last);
        let weight = if age < HOUR {
            4.0
        } else if age < DAY {
            2.0
        } else if age < WEEK {
            0.5
        } else {
            0.25
        };
        self.rank * weight
    }
}

/// In-memory directory database.
#[derive(Debug, Default, Clone)]
pub struct JumpDb {
    entries: Vec<Entry>,
}

impl JumpDb {
    /// Parse the on-disk text. Malformed lines are skipped.
    pub fn parse(text: &str) -> Self {
        let entries = text
            .lines()
            .filter_map(|line| {
                let mut f = line.split('\t');
                let path = f.next().filter(|p| !p.is_empty())?;
                let rank: f64 = f.next()?.parse().ok()?;
                let last: u64 = f.next()?.parse().ok()?;
                Some(Entry {
                    path: path.to_string(),
                    rank,
                    last,
                })
            })
            .collect();
        Self { entries }
    }

    /// Serialise to the on-disk text.
    pub fn to_text(&self) -> String {
        let mut out = String::new();
        for e in &self.entries {
            out.push_str(&format!("{}\t{}\t{}\n", e.path, e.rank, e.last));
        }
        out
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    /// Record a visit. Paths containing tabs or newlines are ignored (they would corrupt the file).
    pub fn add(&mut self, path: &str, now: u64) {
        if path.is_empty() || path.contains(['\t', '\n']) {
            return;
        }
        match self.entries.iter_mut().find(|e| e.path == path) {
            Some(e) => {
                e.rank += 1.0;
                e.last = now;
            }
            None => self.entries.push(Entry {
                path: path.to_string(),
                rank: 1.0,
                last: now,
            }),
        }
        self.age();
    }

    /// Forget a directory. Returns whether it was present.
    pub fn remove(&mut self, path: &str) -> bool {
        let before = self.entries.len();
        self.entries.retain(|e| e.path != path);
        self.entries.len() != before
    }

    /// Drop entries for which `exists` is false. Returns how many were removed.
    pub fn prune(&mut self, exists: impl Fn(&str) -> bool) -> usize {
        let before = self.entries.len();
        self.entries.retain(|e| exists(&e.path));
        before - self.entries.len()
    }

    fn age(&mut self) {
        let total: f64 = self.entries.iter().map(|e| e.rank).sum();
        if total > MAX_TOTAL_RANK {
            let k = 0.9;
            for e in &mut self.entries {
                e.rank *= k;
            }
            self.entries.retain(|e| e.rank >= 1.0);
        }
    }

    /// Entries matching `terms`, best first.
    ///
    /// Every term must appear (case-insensitive) in the path, in order, and the last term must
    /// match inside the final path component. No terms matches everything.
    pub fn query(&self, terms: &[String], now: u64) -> Vec<(&Entry, f64)> {
        let terms: Vec<String> = terms.iter().map(|t| t.to_lowercase()).collect();
        let mut hits: Vec<(&Entry, f64)> = self
            .entries
            .iter()
            .filter(|e| matches(&e.path.to_lowercase(), &terms))
            .map(|e| (e, e.score(now)))
            .collect();
        hits.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.path.cmp(&b.0.path)));
        hits
    }
}

fn matches(path: &str, terms: &[String]) -> bool {
    let mut from = 0;
    for t in terms {
        match path[from..].find(t.as_str()) {
            Some(i) => from += i + t.len(),
            None => return false,
        }
    }
    match terms.last() {
        Some(last) => {
            let base_start = path.trim_end_matches('/').rfind('/').map_or(0, |i| i + 1);
            path[base_start..].contains(last.as_str())
        }
        None => true,
    }
}

/// Load the database; a missing file is an empty database.
pub fn load(path: &Path) -> io::Result<JumpDb> {
    match fs::read_to_string(path) {
        Ok(text) => Ok(JumpDb::parse(&text)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(JumpDb::default()),
        Err(e) => Err(e),
    }
}

/// Save atomically (write a sibling temp file, then rename).
pub fn save(path: &Path, db: &JumpDb) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension(format!("tmp{}", std::process::id()));
    fs::write(&tmp, db.to_text())?;
    fs::rename(&tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn add_increments_and_roundtrips() {
        let mut db = JumpDb::default();
        db.add("/a/b", 100);
        db.add("/a/b", 200);
        db.add("/c", 300);
        let again = JumpDb::parse(&db.to_text());
        assert_eq!(again.entries(), db.entries());
        assert_eq!(again.entries()[0].rank, 2.0);
    }

    #[test]
    fn query_orders_by_frecency() {
        let mut db = JumpDb::default();
        db.add("/w/proj-old", 0);
        for _ in 0..3 {
            db.add("/w/proj-new", 1_000_000);
        }
        let hits = db.query(&s(&["proj"]), 1_000_000);
        assert_eq!(hits[0].0.path, "/w/proj-new");
        assert_eq!(hits.len(), 2);
    }

    #[test]
    fn last_term_must_hit_last_component() {
        let mut db = JumpDb::default();
        db.add("/src/app/docs", 1);
        db.add("/src/docs/app", 1);
        let hits = db.query(&s(&["docs", "app"]), 1);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].0.path, "/src/docs/app");
        assert!(db.query(&s(&["app", "docs"]), 1)[0].0.path == "/src/app/docs");
    }

    #[test]
    fn case_insensitive_and_empty_terms() {
        let mut db = JumpDb::default();
        db.add("/Work/Shell", 1);
        assert_eq!(db.query(&s(&["shell"]), 1).len(), 1);
        assert_eq!(db.query(&[], 1).len(), 1);
        assert!(db.query(&s(&["nope"]), 1).is_empty());
    }

    #[test]
    fn rejects_bad_paths_and_prunes() {
        let mut db = JumpDb::default();
        db.add("/a\tb", 1);
        db.add("", 1);
        assert!(db.entries().is_empty());
        db.add("/gone", 1);
        db.add("/here", 1);
        assert_eq!(db.prune(|p| p == "/here"), 1);
        assert!(db.remove("/here"));
        assert!(!db.remove("/here"));
    }

    #[test]
    fn aging_scales_ranks_down() {
        let mut db = JumpDb::default();
        db.add("/big", 1);
        db.entries[0].rank = MAX_TOTAL_RANK;
        db.add("/small", 2);
        assert!(db.entries()[0].rank < MAX_TOTAL_RANK);
    }

    #[test]
    fn save_and_load_file() {
        let dir = std::env::temp_dir().join(format!("sc-store-{}", std::process::id()));
        let file = dir.join("jump.tsv");
        let mut db = JumpDb::default();
        db.add("/x", 5);
        save(&file, &db).expect("save");
        assert_eq!(load(&file).expect("load").entries(), db.entries());
        assert!(load(&dir.join("missing"))
            .expect("missing ok")
            .entries()
            .is_empty());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn malformed_lines_skipped() {
        let db = JumpDb::parse("garbage\n/ok\t2\t9\n\t1\t1\n/bad\tx\t1\n");
        assert_eq!(db.entries().len(), 1);
    }
}
