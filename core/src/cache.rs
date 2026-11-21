//! Compiled config cache for the hot path.
//!
//! Hot commands must not parse TOML. `apply` writes the validated [`Config`] in a compact binary
//! form; hot commands read that file and only `stat` the source config to see if it is stale.
//! The file starts with a stamp: format version, crate version and the source config's size and
//! modification time. Any mismatch, truncation or garbage reads as "no cache".

use std::fs;
use std::path::Path;
use std::time::UNIX_EPOCH;

use serde::{Deserialize, Serialize};

use crate::Config;

/// Bump when the cached layout changes.
pub const FORMAT: u32 = 12;

/// Identifies the config file a cache was built from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceId {
    /// File size in bytes.
    pub len: u64,
    /// Modification time as nanoseconds since the Unix epoch.
    pub mtime_ns: u128,
}

#[derive(Serialize, Deserialize)]
struct Stamp {
    format: u32,
    version: String,
    source: SourceId,
}

fn stamp(source: SourceId) -> Stamp {
    Stamp {
        format: FORMAT,
        version: env!("CARGO_PKG_VERSION").to_string(),
        source,
    }
}

/// Serialize `config` together with the stamp for `source`.
pub fn encode(config: &Config, source: SourceId) -> Option<Vec<u8>> {
    postcard::to_stdvec(&(stamp(source), config)).ok()
}

/// Decode a cache. `None` if it is corrupt, from another version, or built from a different source.
pub fn decode(bytes: &[u8], source: SourceId) -> Option<Config> {
    let (st, config): (Stamp, Config) = postcard::from_bytes(bytes).ok()?;
    let want = stamp(source);
    (st.format == want.format && st.version == want.version && st.source == want.source)
        .then_some(config)
}

/// Size and modification time of `path`.
pub fn source_id(path: &Path) -> std::io::Result<SourceId> {
    let meta = fs::metadata(path)?;
    let mtime_ns = meta
        .modified()?
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    Ok(SourceId {
        len: meta.len(),
        mtime_ns,
    })
}

/// Hot-path load: one `stat` of the config, one read of the cache. No TOML parsing.
pub fn load(cache_file: &Path, config_file: &Path) -> Option<Config> {
    let source = source_id(config_file).ok()?;
    decode(&fs::read(cache_file).ok()?, source)
}

/// Write the cache for `config`, built from `config_file`.
pub fn store(cache_file: &Path, config_file: &Path, config: &Config) -> std::io::Result<()> {
    let source = source_id(config_file)?;
    let bytes = encode(config, source)
        .ok_or_else(|| std::io::Error::other("cannot serialize config cache"))?;
    fs::write(cache_file, bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SRC: SourceId = SourceId {
        len: 10,
        mtime_ns: 1_000,
    };

    fn sample() -> Config {
        Config::from_toml("[env]\nA = \"1\"\n[tools]\nstarship = true\n").unwrap()
    }

    #[test]
    fn roundtrip() {
        let bytes = encode(&sample(), SRC).unwrap();
        assert_eq!(decode(&bytes, SRC), Some(sample()));
    }

    #[test]
    fn source_change_invalidates() {
        let bytes = encode(&sample(), SRC).unwrap();
        assert_eq!(decode(&bytes, SourceId { len: 11, ..SRC }), None);
        assert_eq!(
            decode(
                &bytes,
                SourceId {
                    mtime_ns: 2_000,
                    ..SRC
                }
            ),
            None
        );
    }

    #[test]
    fn corrupt_or_truncated_is_none() {
        let bytes = encode(&sample(), SRC).unwrap();
        assert_eq!(decode(&bytes[..bytes.len() / 2], SRC), None);
        assert_eq!(decode(b"garbage", SRC), None);
        assert_eq!(decode(&[], SRC), None);
    }

    #[test]
    fn file_roundtrip_and_staleness() {
        let dir = std::env::temp_dir().join(format!("shellcraft-cache-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let cfg_file = dir.join("shellcraft.toml");
        let cache_file = dir.join("config.bin");
        fs::write(&cfg_file, "[env]\nA = \"1\"\n").unwrap();
        let cfg = Config::from_toml(&fs::read_to_string(&cfg_file).unwrap()).unwrap();
        assert_eq!(load(&cache_file, &cfg_file), None);
        store(&cache_file, &cfg_file, &cfg).unwrap();
        assert_eq!(load(&cache_file, &cfg_file), Some(cfg));
        fs::write(&cfg_file, "[env]\nA = \"22\"\n").unwrap();
        assert_eq!(load(&cache_file, &cfg_file), None);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn version_mismatch_is_none() {
        let old = (
            Stamp {
                format: FORMAT,
                version: "0.0.0-old".into(),
                source: SRC,
            },
            sample(),
        );
        let bytes = postcard::to_stdvec(&old).unwrap();
        assert_eq!(decode(&bytes, SRC), None);
        let old = (
            Stamp {
                format: FORMAT + 1,
                version: env!("CARGO_PKG_VERSION").into(),
                source: SRC,
            },
            sample(),
        );
        let bytes = postcard::to_stdvec(&old).unwrap();
        assert_eq!(decode(&bytes, SRC), None);
    }
}
