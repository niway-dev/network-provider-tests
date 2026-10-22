//! Append-only JSONL store: `<data dir>/runs.jsonl`.

use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use crate::record::RunRecord;

pub const FILE_NAME: &str = "runs.jsonl";

/// Data directory: `NETTEST_DATA_DIR`, else the macOS log directory the bash tool
/// uses, else the platform's local data directory.
pub fn data_dir() -> PathBuf {
    if let Some(d) = std::env::var_os("NETTEST_DATA_DIR") {
        if !d.is_empty() {
            return PathBuf::from(d);
        }
    }
    default_dir()
}

#[cfg(target_os = "macos")]
fn default_dir() -> PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    home.join("Library/Logs/network-provider-tests")
}

#[cfg(not(target_os = "macos"))]
fn default_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("network-provider-tests")
}

#[derive(Clone, Debug)]
pub struct Store {
    dir: PathBuf,
}

/// Result of reading the file: parsed runs in file order and the number of
/// lines that were skipped because they did not parse.
#[derive(Debug, Default)]
pub struct ReadResult {
    pub runs: Vec<RunRecord>,
    pub skipped: usize,
}

impl Store {
    pub fn new(dir: impl Into<PathBuf>) -> Store {
        Store { dir: dir.into() }
    }

    pub fn from_env() -> Store {
        Store::new(data_dir())
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn file(&self) -> PathBuf {
        self.dir.join(FILE_NAME)
    }

    /// Append one record as a single buffer ending in `\n`, so concurrent writers
    /// (CLI and app) do not interleave lines.
    pub fn append(&self, record: &RunRecord) -> io::Result<()> {
        self.append_line(&record.to_json_line())
    }

    pub fn append_line(&self, line: &str) -> io::Result<()> {
        fs::create_dir_all(&self.dir)?;
        let mut buf = String::with_capacity(line.len() + 1);
        buf.push_str(line);
        buf.push('\n');
        let mut f = OpenOptions::new()
            .create(true)
            .append(true)
            .open(self.file())?;
        f.write_all(buf.as_bytes())
    }

    /// Read every parsable run. A missing file is an empty store. Lines that do
    /// not parse are skipped and counted, never fatal.
    pub fn read_all(&self) -> io::Result<ReadResult> {
        let bytes = match fs::read(self.file()) {
            Ok(b) => b,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(ReadResult::default()),
            Err(e) => return Err(e),
        };
        let text = String::from_utf8_lossy(&bytes);
        let mut out = ReadResult::default();
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            match RunRecord::from_json_line(line) {
                Ok(r) => out.runs.push(r),
                Err(_) => out.skipped += 1,
            }
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn temp_dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("nettest-store-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        d
    }

    fn sample(n: usize) -> RunRecord {
        let line = include_str!("../../../tests/fixtures/run-bash-sample.jsonl")
            .lines()
            .next()
            .unwrap();
        let mut r = RunRecord::from_json_line(line).unwrap();
        r.duration_ms = n as u64;
        r
    }

    #[test]
    fn two_threads_append_and_every_line_parses() {
        let dir = temp_dir("threads");
        let store = Arc::new(Store::new(&dir));
        let handles: Vec<_> = (0..2)
            .map(|t| {
                let s = store.clone();
                std::thread::spawn(move || {
                    for i in 0..100 {
                        s.append(&sample(t * 1000 + i)).unwrap();
                    }
                })
            })
            .collect();
        for h in handles {
            h.join().unwrap();
        }
        let res = store.read_all().unwrap();
        assert_eq!(res.runs.len(), 200);
        assert_eq!(res.skipped, 0);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn bad_lines_are_skipped_and_counted() {
        let dir = temp_dir("bad");
        let store = Store::new(&dir);
        store.append(&sample(1)).unwrap();
        store.append_line("not json").unwrap();
        store.append_line("{\"schema\":1}").unwrap(); // no ts: not a record
        store.append(&sample(2)).unwrap();
        let res = store.read_all().unwrap();
        assert_eq!(res.runs.len(), 2);
        assert_eq!(res.skipped, 2);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_file_is_an_empty_store() {
        let store = Store::new(temp_dir("missing"));
        let res = store.read_all().unwrap();
        assert!(res.runs.is_empty());
    }

    #[test]
    fn env_override_wins() {
        // Only reads the variable; other tests pass an explicit directory.
        std::env::set_var("NETTEST_DATA_DIR", "/tmp/nettest-env-test");
        assert_eq!(data_dir(), PathBuf::from("/tmp/nettest-env-test"));
        std::env::remove_var("NETTEST_DATA_DIR");
    }
}
