//! `shellcraft cat`: file viewer (bat replacement). On a terminal it adds a header, line
//! numbers and syntax colours; when piped it is a plain byte-for-byte `cat`.

use std::fs;
use std::io::{self, IsTerminal, Read, Write};
use std::path::{Path, PathBuf};

use anyhow::Result;
use syntect::easy::HighlightLines;
use syntect::highlighting::ThemeSet;
use syntect::parsing::SyntaxSet;
use syntect::util::{as_24_bit_terminal_escaped, LinesWithEndings};

const DIM: &str = "\x1b[2m";
const RESET: &str = "\x1b[0m";
/// Larger files are numbered but not highlighted (highlighting costs about 3 ms per KB).
const MAX_HIGHLIGHT: usize = 512 * 1024;

fn read_input(path: &Path) -> io::Result<Vec<u8>> {
    if path == Path::new("-") {
        let mut v = Vec::new();
        io::stdin().lock().read_to_end(&mut v)?;
        Ok(v)
    } else {
        fs::read(path)
    }
}

fn looks_binary(data: &[u8]) -> bool {
    data.iter().take(8192).any(|&b| b == 0)
}

/// Print `text` with a gutter and syntax colours.
fn pretty(out: &mut impl Write, name: &str, text: &str, numbers: bool) -> io::Result<()> {
    let ss = SyntaxSet::load_defaults_newlines();
    let ts = ThemeSet::load_defaults();
    let ext = Path::new(name)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");
    let syntax = ss
        .find_syntax_by_extension(ext)
        .or_else(|| {
            text.lines()
                .next()
                .and_then(|l| ss.find_syntax_by_first_line(l))
        })
        .unwrap_or_else(|| ss.find_syntax_plain_text());
    let mut hl = HighlightLines::new(syntax, &ts.themes["base16-ocean.dark"]);
    let total = text.lines().count().max(1);
    let gutter = total.to_string().len();
    writeln!(out, "{DIM}{name}{RESET}")?;
    for (i, line) in LinesWithEndings::from(text).enumerate() {
        if numbers {
            write!(out, "{DIM}{:>gutter$} │{RESET} ", i + 1)?;
        }
        if text.len() > MAX_HIGHLIGHT {
            write!(out, "{line}")?;
            continue;
        }
        match hl.highlight_line(line, &ss) {
            Ok(ranges) => write!(out, "{}{RESET}", as_24_bit_terminal_escaped(&ranges, false))?,
            Err(_) => write!(out, "{line}")?,
        }
    }
    Ok(())
}

/// Arguments of `shellcraft cat`.
#[derive(clap::Args)]
pub struct CatArgs {
    pub files: Vec<PathBuf>,
    /// No header, numbers or colours
    #[arg(short, long)]
    pub plain: bool,
    /// Hide line numbers
    #[arg(long)]
    pub no_numbers: bool,
}

pub fn run(args: CatArgs) -> Result<()> {
    let (files, plain, numbers) = (&args.files[..], args.plain, !args.no_numbers);
    let tty = io::stdout().is_terminal();
    let fancy = tty && !plain && std::env::var_os("NO_COLOR").is_none();
    let mut out = io::BufWriter::new(io::stdout().lock());
    let inputs: Vec<PathBuf> = if files.is_empty() {
        vec![PathBuf::from("-")]
    } else {
        files.to_vec()
    };
    let mut failed = false;
    for p in &inputs {
        let data = match read_input(p) {
            Ok(d) => d,
            Err(e) => {
                eprintln!("cat: {}: {e}", p.display());
                failed = true;
                continue;
            }
        };
        if !fancy {
            out.write_all(&data)?;
        } else if looks_binary(&data) {
            writeln!(
                out,
                "{DIM}{}: binary file ({} bytes){RESET}",
                p.display(),
                data.len()
            )?;
        } else {
            let name = if p == Path::new("-") {
                "stdin".to_string()
            } else {
                p.display().to_string()
            };
            pretty(&mut out, &name, &String::from_utf8_lossy(&data), numbers)?;
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
    fn pretty_prints_header_numbers_and_colours() {
        let mut buf = Vec::new();
        pretty(&mut buf, "a.rs", "fn main() {}\nlet x = 1;\n", true).expect("pretty");
        let s = String::from_utf8(buf).expect("utf8");
        assert!(
            s.contains("a.rs") && s.contains("1 │") && s.contains("2 │"),
            "{s:?}"
        );
        assert!(
            s.contains("\x1b[38;2;"),
            "24-bit colour for Rust code: {s:?}"
        );
        let mut plain = Vec::new();
        pretty(&mut plain, "notes.unknownext", "hello\n", false).expect("pretty");
        assert!(String::from_utf8(plain).expect("utf8").contains("hello"));
    }

    #[test]
    fn binary_detection() {
        assert!(looks_binary(b"ab\0cd"));
        assert!(!looks_binary("héllo".as_bytes()));
    }
}
