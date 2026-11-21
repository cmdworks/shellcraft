//! The official plugin registry, embedded in the binary: `index.toml` and one folder per plugin
//! (`vendored-plugins/`, synced from the shellcraft-plugins repository). [`extract`] writes it to a folder the registry code can read like
//! any other local registry.

use std::fs;
use std::io;
use std::path::Path;

include!(concat!(env!("OUT_DIR"), "/official.rs"));

/// Write the embedded registry to `dest` unless it is already there in this exact version.
pub fn extract(dest: &Path) -> io::Result<()> {
    let stamp = dest.join(".stamp");
    if fs::read_to_string(&stamp).is_ok_and(|s| s.trim() == STAMP) {
        return Ok(());
    }
    let _ = fs::remove_dir_all(dest);
    for (rel, bytes, exec) in FILES {
        let path = dest.join(rel);
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        fs::write(&path, bytes)?;
        #[cfg(unix)]
        if *exec {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o755))?;
        }
        #[cfg(not(unix))]
        let _ = exec;
    }
    fs::write(stamp, STAMP)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_an_index_and_executable_scripts_once() {
        let dir = std::env::temp_dir().join(format!("sc-official-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        extract(&dir).expect("extract");
        assert!(
            dir.join("index.toml").is_file()
                && dir.join("plugins/base/shellcraft-plugin.toml").is_file()
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = fs::metadata(dir.join("plugins/docker/bin/segment"))
                .expect("segment")
                .permissions()
                .mode();
            assert_eq!(mode & 0o111, 0o111, "scripts stay executable");
        }
        fs::write(dir.join("marker"), "x").expect("marker");
        extract(&dir).expect("again");
        assert!(
            dir.join("marker").exists(),
            "an up-to-date copy is left alone"
        );
        fs::write(dir.join(".stamp"), "old").expect("stamp");
        extract(&dir).expect("refresh");
        assert!(
            !dir.join("marker").exists(),
            "a different version is replaced"
        );
        let _ = fs::remove_dir_all(&dir);
    }
}
