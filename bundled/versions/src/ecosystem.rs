//! The ecosystems the versions plugin knows, and how to read what a project declares.

use std::fs;
use std::path::{Path, PathBuf};

const MAX_DEPTH: usize = 8;

pub(super) struct Eco {
    pub(super) name: &'static str,
    pub(super) label: &'static str,
    pub(super) markers: &'static [&'static str],
    pub(super) program: &'static str,
    pub(super) args: &'static [&'static str],
    /// 1-based whitespace token of the first output line that holds the version (0 = whole line).
    pub(super) token: usize,
    /// `.tool-versions` key.
    pub(super) asdf: &'static str,
}

pub(super) const ECOS: &[Eco] = &[
    Eco {
        name: "node",
        label: "node",
        markers: &["package.json", ".nvmrc", ".node-version"],
        program: "node",
        args: &["--version"],
        token: 0,
        asdf: "nodejs",
    },
    Eco {
        name: "bun",
        label: "bun",
        markers: &["bun.lockb", "bun.lock", "bunfig.toml", ".bun-version"],
        program: "bun",
        args: &["--version"],
        token: 0,
        asdf: "bun",
    },
    Eco {
        name: "deno",
        label: "deno",
        markers: &["deno.json", "deno.jsonc", "deno.lock"],
        program: "deno",
        args: &["--version"],
        token: 2,
        asdf: "deno",
    },
    Eco {
        name: "python",
        label: "py",
        markers: &[
            "pyproject.toml",
            "requirements.txt",
            "setup.py",
            "Pipfile",
            ".python-version",
        ],
        program: "python3",
        args: &["--version"],
        token: 2,
        asdf: "python",
    },
    Eco {
        name: "go",
        label: "go",
        markers: &["go.mod", "go.work"],
        program: "go",
        args: &["version"],
        token: 3,
        asdf: "golang",
    },
    Eco {
        name: "rust",
        label: "rs",
        markers: &["Cargo.toml", "rust-toolchain", "rust-toolchain.toml"],
        program: "rustc",
        args: &["--version"],
        token: 2,
        asdf: "rust",
    },
    Eco {
        name: "ruby",
        label: "rb",
        markers: &["Gemfile", ".ruby-version"],
        program: "ruby",
        args: &["--version"],
        token: 2,
        asdf: "ruby",
    },
];

pub(super) fn eco(name: &str) -> Option<&'static Eco> {
    ECOS.iter().find(|e| e.name == name)
}

/// Nearest ancestor of `cwd` (up to 8 levels) that contains one of the markers.
pub(super) fn project_dir(cwd: &Path, markers: &[&str]) -> Option<PathBuf> {
    cwd.ancestors()
        .take(MAX_DEPTH)
        .find(|d| markers.iter().any(|m| d.join(m).exists()))
        .map(Path::to_path_buf)
}

pub(super) fn first_line(path: &Path) -> Option<String> {
    let text = fs::read_to_string(path).ok()?;
    let line = text.lines().next()?.trim();
    (!line.is_empty()).then(|| line.to_string())
}

/// Value of `key = "value"` (or `key: "value"`) on any line of `text`.
pub(super) fn quoted_value(text: &str, key: &str) -> Option<String> {
    text.lines().find_map(|l| {
        let l = l.trim();
        let rest = l.strip_prefix(key)?.trim_start();
        let rest = rest.strip_prefix(['=', ':'])?.trim();
        let v = rest.trim_matches(|c| matches!(c, '"' | '\'' | ',' | '}' | ' '));
        (!v.is_empty()).then(|| v.to_string())
    })
}

/// Version the project asks for, if it says so.
pub(super) fn declared(e: &Eco, dir: &Path) -> Option<String> {
    if let Ok(tv) = fs::read_to_string(dir.join(".tool-versions")) {
        for l in tv.lines() {
            let mut w = l.split_whitespace();
            if w.next() == Some(e.asdf) {
                if let Some(v) = w.next() {
                    return Some(v.to_string());
                }
            }
        }
    }
    let read = |f: &str| fs::read_to_string(dir.join(f)).ok();
    let v = match e.name {
        "node" => first_line(&dir.join(".nvmrc"))
            .or_else(|| first_line(&dir.join(".node-version")))
            .or_else(|| {
                let pj = read("package.json")?;
                let at = pj.find("\"engines\"")?;
                quoted_value(&pj[at..].replace('{', "\n"), "\"node\"")
            }),
        "bun" => first_line(&dir.join(".bun-version")).or_else(|| {
            let pj = read("package.json")?;
            let at = pj.find("\"bun@")? + 5;
            Some(pj[at..].split('"').next()?.to_string())
        }),
        "python" => first_line(&dir.join(".python-version"))
            .or_else(|| quoted_value(&read("pyproject.toml")?, "requires-python")),
        "go" => read("go.mod")?
            .lines()
            .find_map(|l| l.strip_prefix("go ").map(|v| v.trim().to_string())),
        "rust" => first_line(&dir.join("rust-toolchain"))
            .filter(|l| !l.starts_with('['))
            .or_else(|| quoted_value(&read("rust-toolchain.toml")?, "channel"))
            .or_else(|| quoted_value(&read("Cargo.toml")?, "rust-version")),
        "ruby" => first_line(&dir.join(".ruby-version")),
        _ => None,
    }?;
    Some(v.trim_start_matches('v').to_string())
}

/// Does `active` satisfy a plain declared version like `18` or `3.12`? Ranges are not judged.
pub(super) fn satisfies(declared: &str, active: &str) -> bool {
    let plain = !declared.is_empty()
        && declared
            .split('.')
            .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()));
    if !plain {
        return true;
    }
    active == declared || active.starts_with(&format!("{declared}."))
}

pub(super) fn parse_version(e: &Eco, output: &str) -> Option<String> {
    let line = output.lines().next()?.trim();
    let word = if e.token == 0 {
        line
    } else {
        line.split_whitespace().nth(e.token - 1)?
    };
    let v = word.trim_start_matches(['v', 'g', 'o']).trim();
    // `go1.22.1` loses its `go`; everything else keeps digits first.
    let v = if v.starts_with(|c: char| c.is_ascii_digit()) {
        v
    } else {
        word.trim_start_matches('v')
    };
    (v.starts_with(|c: char| c.is_ascii_digit())).then(|| v.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("sc-ver-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).expect("mkdir");
        d
    }

    #[test]
    fn declared_versions_from_project_files() {
        let d = temp("declared");
        let node = eco("node").expect("node");
        fs::write(
            d.join("package.json"),
            r#"{"name":"x","engines":{"node":">=18.0.0"}}"#,
        )
        .expect("write");
        assert_eq!(declared(node, &d).as_deref(), Some(">=18.0.0"));
        fs::write(d.join(".nvmrc"), "v20.11.0\n").expect("write");
        assert_eq!(
            declared(node, &d).as_deref(),
            Some("20.11.0"),
            ".nvmrc wins and loses the v"
        );
        fs::write(d.join("go.mod"), "module x\n\ngo 1.22\n").expect("write");
        assert_eq!(
            declared(eco("go").expect("go"), &d).as_deref(),
            Some("1.22")
        );
        fs::write(
            d.join("rust-toolchain.toml"),
            "[toolchain]\nchannel = \"1.80.0\"\n",
        )
        .expect("write");
        assert_eq!(
            declared(eco("rust").expect("rust"), &d).as_deref(),
            Some("1.80.0")
        );
        fs::write(
            d.join("pyproject.toml"),
            "[project]\nrequires-python = \">=3.10\"\n",
        )
        .expect("write");
        assert_eq!(
            declared(eco("python").expect("py"), &d).as_deref(),
            Some(">=3.10")
        );
        fs::write(d.join(".tool-versions"), "ruby 3.3.0\nnodejs 22.1.0\n").expect("write");
        assert_eq!(
            declared(eco("ruby").expect("rb"), &d).as_deref(),
            Some("3.3.0")
        );
        assert_eq!(
            declared(node, &d).as_deref(),
            Some("22.1.0"),
            ".tool-versions first"
        );
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn version_comparison_and_parsing() {
        assert!(
            satisfies("18", "18.2.0")
                && satisfies("3.12", "3.12.1")
                && satisfies("20.1.0", "20.1.0")
        );
        assert!(!satisfies("18", "20.1.0") && !satisfies("3.1", "3.12.1"));
        assert!(satisfies(">=18", "16.0.0"), "ranges are not judged");
        let p = |n: &str, out: &str| parse_version(eco(n).expect("eco"), out);
        assert_eq!(p("node", "v20.11.0\n").as_deref(), Some("20.11.0"));
        assert_eq!(p("python", "Python 3.12.1").as_deref(), Some("3.12.1"));
        assert_eq!(
            p("go", "go version go1.22.1 darwin/arm64").as_deref(),
            Some("1.22.1")
        );
        assert_eq!(
            p("rust", "rustc 1.80.0 (051478957 2024-07-21)").as_deref(),
            Some("1.80.0")
        );
        assert_eq!(
            p("deno", "deno 1.44.0 (release, aarch64)").as_deref(),
            Some("1.44.0")
        );
        assert_eq!(p("bun", "1.1.8").as_deref(), Some("1.1.8"));
        assert_eq!(p("node", "command not found"), None);
    }
}
