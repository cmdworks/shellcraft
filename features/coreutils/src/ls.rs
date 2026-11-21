//! `shellcraft ls`: directory listing (eza replacement): colours, columns, `-l`, `-a`, tree.

use std::fs::{self, Metadata};
use std::io::{self, IsTerminal, Write};
use std::os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::Result;

const RESET: &str = "\x1b[0m";

#[derive(Clone, Copy, PartialEq, Debug)]
enum Kind {
    Dir,
    Link,
    Exec,
    Pipe,
    Special,
    File,
}

struct Item {
    name: String,
    path: PathBuf,
    meta: Metadata,
    kind: Kind,
}

fn kind_of(meta: &Metadata) -> Kind {
    let ft = meta.file_type();
    if ft.is_symlink() {
        Kind::Link
    } else if ft.is_dir() {
        Kind::Dir
    } else if ft.is_fifo() {
        Kind::Pipe
    } else if ft.is_socket() || ft.is_block_device() || ft.is_char_device() {
        Kind::Special
    } else if meta.permissions().mode() & 0o111 != 0 {
        Kind::Exec
    } else {
        Kind::File
    }
}

fn color(kind: Kind) -> &'static str {
    match kind {
        Kind::Dir => "\x1b[1;34m",
        Kind::Link => "\x1b[36m",
        Kind::Exec => "\x1b[32m",
        Kind::Pipe => "\x1b[33m",
        Kind::Special => "\x1b[1;33m",
        Kind::File => "",
    }
}

fn suffix(kind: Kind) -> &'static str {
    match kind {
        Kind::Dir => "/",
        Kind::Exec => "*",
        Kind::Link => "@",
        _ => "",
    }
}

/// `drwxr-xr-x`-style permission text.
fn mode_string(mode: u32, kind: Kind) -> String {
    let t = match kind {
        Kind::Dir => 'd',
        Kind::Link => 'l',
        Kind::Pipe => 'p',
        Kind::Special => 'c',
        _ => '-',
    };
    let mut s = String::from(t);
    for (i, c) in "rwxrwxrwx".chars().enumerate() {
        s.push(if mode & (1 << (8 - i)) != 0 { c } else { '-' });
    }
    s
}

/// `1.5K`, `12M`, ... (1024 based).
fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["", "K", "M", "G", "T"];
    let mut v = bytes as f64;
    let mut u = 0;
    while v >= 1024.0 && u < UNITS.len() - 1 {
        v /= 1024.0;
        u += 1;
    }
    if u == 0 {
        format!("{bytes}")
    } else if v < 10.0 {
        format!("{v:.1}{}", UNITS[u])
    } else {
        format!("{v:.0}{}", UNITS[u])
    }
}

/// Age such as `5m`, `3h`, `12d` (timezone free).
fn age(now: u64, then: i64) -> String {
    let d = now.saturating_sub(then.max(0) as u64);
    match d {
        0..=59 => format!("{d}s"),
        60..=3599 => format!("{}m", d / 60),
        3600..=86_399 => format!("{}h", d / 3600),
        86_400..=2_591_999 => format!("{}d", d / 86_400),
        2_592_000..=31_535_999 => format!("{}mo", d / 2_592_000),
        _ => format!("{}y", d / 31_536_000),
    }
}

#[derive(Default, Clone, Copy)]
pub struct Opts {
    pub long: bool,
    pub all: bool,
    pub tree: bool,
    pub level: usize,
    pub color: bool,
    pub width: usize,
    /// One entry per line.
    pub one: bool,
    pub by_time: bool,
    pub by_size: bool,
    pub reverse: bool,
}

fn read_items(dir: &Path, opts: &Opts) -> Vec<Item> {
    let all = opts.all;
    let Ok(rd) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut items: Vec<Item> = rd
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            if !all && name.starts_with('.') {
                return None;
            }
            let meta = fs::symlink_metadata(e.path()).ok()?;
            Some(Item {
                kind: kind_of(&meta),
                name,
                path: e.path(),
                meta,
            })
        })
        .collect();
    // directories first, then the chosen order (name by default)
    items.sort_by(|a, b| {
        let by_dir = (a.kind != Kind::Dir).cmp(&(b.kind != Kind::Dir));
        let order = if opts.by_time {
            b.meta.mtime().cmp(&a.meta.mtime())
        } else if opts.by_size {
            b.meta.len().cmp(&a.meta.len())
        } else {
            std::cmp::Ordering::Equal
        };
        by_dir
            .then(order)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    if opts.reverse {
        items.reverse();
    }
    items
}

fn paint(opts: &Opts, kind: Kind, text: &str) -> String {
    if opts.color && !color(kind).is_empty() {
        format!("{}{text}{RESET}", color(kind))
    } else {
        text.to_string()
    }
}

fn label(opts: &Opts, it: &Item) -> String {
    let mut text = it.name.clone();
    if it.kind == Kind::Link {
        if let Ok(target) = fs::read_link(&it.path) {
            text = format!("{} -> {}", it.name, target.display());
        }
    }
    paint(opts, it.kind, &text)
}

/// Marker after the name; only without colour, where nothing else tells the types apart.
fn marker(opts: &Opts, kind: Kind) -> &'static str {
    if opts.color {
        ""
    } else {
        suffix(kind)
    }
}

fn plain_width(it: &Item, opts: &Opts) -> usize {
    it.name.chars().count() + marker(opts, it.kind).len()
}

fn write_columns(out: &mut impl Write, items: &[Item], opts: &Opts) -> io::Result<()> {
    if items.is_empty() {
        return Ok(());
    }
    if opts.width == 0 || opts.one {
        for it in items {
            writeln!(out, "{}", label(opts, it))?;
        }
        return Ok(());
    }
    let cell = items
        .iter()
        .map(|i| plain_width(i, opts))
        .max()
        .unwrap_or(1)
        + 2;
    let cols = (opts.width / cell).max(1);
    let rows = items.len().div_ceil(cols);
    for r in 0..rows {
        let mut line = String::new();
        for c in 0..cols {
            let Some(it) = items.get(c * rows + r) else {
                break;
            };
            let shown = format!("{}{}", it.name, marker(opts, it.kind));
            let pad = cell - shown.chars().count();
            line.push_str(&paint(opts, it.kind, &shown));
            if c * rows + r + rows < items.len() {
                line.push_str(&" ".repeat(pad));
            }
        }
        writeln!(out, "{}", line.trim_end())?;
    }
    Ok(())
}

fn write_long(out: &mut impl Write, items: &[Item], opts: &Opts, now: u64) -> io::Result<()> {
    let sizes: Vec<String> = items
        .iter()
        .map(|i| {
            if i.kind == Kind::Dir {
                "-".into()
            } else {
                human_size(i.meta.len())
            }
        })
        .collect();
    let w = sizes.iter().map(String::len).max().unwrap_or(1);
    for (it, size) in items.iter().zip(&sizes) {
        writeln!(
            out,
            "{} {size:>w$} {:>4} {}",
            mode_string(it.meta.permissions().mode(), it.kind),
            age(now, it.meta.mtime()),
            label(opts, it),
        )?;
    }
    Ok(())
}

fn write_tree(
    out: &mut impl Write,
    dir: &Path,
    opts: &Opts,
    prefix: &str,
    depth: usize,
) -> io::Result<()> {
    let items = read_items(dir, opts);
    let n = items.len();
    for (i, it) in items.iter().enumerate() {
        let last = i + 1 == n;
        writeln!(
            out,
            "{prefix}{}{}",
            if last { "└── " } else { "├── " },
            label(opts, it)
        )?;
        if it.kind == Kind::Dir && depth < opts.level {
            let next = format!("{prefix}{}", if last { "    " } else { "│   " });
            write_tree(out, &it.path, opts, &next, depth + 1)?;
        }
    }
    Ok(())
}

/// List `paths` (default `.`).
/// Arguments of `shellcraft ls`.
#[derive(clap::Args)]
#[command(disable_help_flag = true)]
pub struct LsArgs {
    pub paths: Vec<PathBuf>,
    /// Long format: permissions, size, age
    #[arg(short, long)]
    pub long: bool,
    /// Show hidden entries
    #[arg(short, long)]
    pub all: bool,
    /// Draw a tree
    #[arg(short = 'T', long)]
    pub tree: bool,
    /// Tree depth
    #[arg(short = 'L', long, default_value_t = 2)]
    pub level: usize,
    /// One entry per line
    #[arg(short = '1')]
    pub one: bool,
    /// Accepted for compatibility: sizes are always human readable
    #[arg(short = 'h')]
    pub _human: bool,
    /// Newest first
    #[arg(short = 't')]
    pub by_time: bool,
    /// Largest first
    #[arg(short = 'S')]
    pub by_size: bool,
    /// Reverse the order
    #[arg(short = 'r')]
    pub reverse: bool,
}

/// Run `shellcraft ls` with parsed arguments.
pub fn run_args(a: LsArgs) -> Result<()> {
    run(
        &a.paths,
        Opts {
            long: a.long,
            all: a.all,
            tree: a.tree,
            level: a.level,
            one: a.one,
            by_time: a.by_time,
            by_size: a.by_size,
            reverse: a.reverse,
            ..Opts::default()
        },
    )
}

pub fn run(paths: &[PathBuf], mut opts: Opts) -> Result<()> {
    let tty = io::stdout().is_terminal();
    opts.color = tty && std::env::var_os("NO_COLOR").is_none();
    opts.width = if tty {
        crossterm::terminal::size()
            .ok()
            .map_or(80, |(c, _)| usize::from(c).max(20))
    } else {
        0
    };
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let mut out = io::BufWriter::new(io::stdout().lock());
    let targets: Vec<PathBuf> = if paths.is_empty() {
        vec![PathBuf::from(".")]
    } else {
        paths.to_vec()
    };
    let many = targets.len() > 1;
    let mut failed = false;
    for (n, p) in targets.iter().enumerate() {
        let meta = match fs::symlink_metadata(p) {
            Ok(m) => m,
            Err(e) => {
                eprintln!("ls: {}: {e}", p.display());
                failed = true;
                continue;
            }
        };
        if n > 0 {
            writeln!(out)?;
        }
        if !meta.is_dir() {
            let it = Item {
                name: p.to_string_lossy().into_owned(),
                path: p.clone(),
                kind: kind_of(&meta),
                meta,
            };
            if opts.long {
                write_long(&mut out, &[it], &opts, now)?
            } else {
                write_columns(&mut out, &[it], &opts)?
            }
            continue;
        }
        if many {
            writeln!(out, "{}:", p.display())?;
        }
        if opts.tree {
            writeln!(out, "{}", paint(&opts, Kind::Dir, &p.to_string_lossy()))?;
            write_tree(&mut out, p, &opts, "", 1)?;
        } else if opts.long {
            write_long(&mut out, &read_items(p, &opts), &opts, now)?;
        } else {
            write_columns(&mut out, &read_items(p, &opts), &opts)?;
        }
    }
    out.flush()?;
    if failed {
        std::process::exit(1);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats() {
        assert_eq!(mode_string(0o755, Kind::Dir), "drwxr-xr-x");
        assert_eq!(mode_string(0o640, Kind::File), "-rw-r-----");
        assert_eq!(human_size(512), "512");
        assert_eq!(human_size(1536), "1.5K");
        assert_eq!(human_size(12 * 1024 * 1024), "12M");
        assert_eq!(age(1000, 990), "10s");
        assert_eq!(age(10_000, 6_400), "1h");
        assert_eq!(age(1_000_000, 0), "11d");
    }

    #[test]
    fn lists_dirs_first_hides_dotfiles_and_draws_tree() {
        let root = std::env::temp_dir().join(format!("sc-ls-{}", std::process::id()));
        fs::create_dir_all(root.join("b_dir/inner")).expect("mkdir");
        fs::write(root.join("a_file"), "x").expect("write");
        fs::write(root.join(".hidden"), "x").expect("write");
        let names: Vec<_> = read_items(&root, &Opts::default())
            .into_iter()
            .map(|i| i.name)
            .collect();
        assert_eq!(names, vec!["b_dir", "a_file"]);
        assert_eq!(
            read_items(
                &root,
                &Opts {
                    all: true,
                    ..Opts::default()
                }
            )
            .len(),
            3
        );
        let mut buf = Vec::new();
        let opts = Opts {
            level: 2,
            ..Opts::default()
        };
        write_tree(&mut buf, &root, &opts, "", 1).expect("tree");
        let text = String::from_utf8(buf).expect("utf8");
        assert!(
            text.contains("├── b_dir\n│   └── inner\n└── a_file"),
            "{text}"
        );
        let mut cols = Vec::new();
        write_columns(
            &mut cols,
            &read_items(&root, &Opts::default()),
            &Opts {
                width: 80,
                ..Opts::default()
            },
        )
        .expect("cols");
        assert_eq!(
            String::from_utf8(cols).expect("utf8").trim(),
            "b_dir/  a_file"
        );
        let _ = fs::remove_dir_all(&root);
    }
}
