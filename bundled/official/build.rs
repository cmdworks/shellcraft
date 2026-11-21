//! Embeds every file of `vendored-plugins/` (a copy of the shellcraft-plugins repository made by
//! `scripts/sync-plugins.sh`) so `shellcraft install <name>` works offline with no
//! registry to configure.

use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

fn walk(dir: &Path, root: &Path, out: &mut Vec<(String, PathBuf, bool)>) {
    let mut entries: Vec<_> = fs::read_dir(dir).expect("read_dir").flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        let path = e.path();
        let name = e.file_name().to_string_lossy().into_owned();
        if name == ".git" || name == ".DS_Store" {
            continue;
        }
        if path.is_dir() {
            walk(&path, root, out);
        } else {
            #[cfg(unix)]
            let exec = {
                use std::os::unix::fs::PermissionsExt;
                fs::metadata(&path).is_ok_and(|m| m.permissions().mode() & 0o111 != 0)
            };
            #[cfg(not(unix))]
            let exec = false;
            let rel = path
                .strip_prefix(root)
                .expect("prefix")
                .to_string_lossy()
                .replace('\\', "/");
            out.push((rel, path, exec));
        }
    }
}

fn main() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("manifest dir"));
    let root = manifest.join("../../vendored-plugins");
    println!("cargo:rerun-if-changed={}", root.display());
    let mut files = Vec::new();
    walk(&root, &root, &mut files);
    // FNV-1a over names and contents: changes whenever the embedded plugins change
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    let mut code = String::from("pub static FILES: &[(&str, &[u8], bool)] = &[\n");
    for (rel, path, exec) in &files {
        let bytes = fs::read(path).expect("read");
        for b in rel.bytes().chain(bytes.iter().copied()) {
            h = (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3);
        }
        let abs = path.canonicalize().expect("canonical");
        writeln!(
            code,
            "    ({rel:?}, include_bytes!({:?}), {exec}),",
            abs.display().to_string()
        )
        .expect("write");
    }
    code.push_str("];\n");
    writeln!(code, "pub const STAMP: &str = \"{h:016x}\";").expect("write");
    let out = PathBuf::from(std::env::var("OUT_DIR").expect("out dir")).join("official.rs");
    fs::write(out, code).expect("write generated");
}
