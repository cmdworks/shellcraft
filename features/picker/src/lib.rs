//! `shellcraft pick`: fuzzy picker (fzf replacement). Reads lines from stdin, draws a small
//! inline list on the terminal (`/dev/tty`), prints the chosen line to stdout.

use std::fs::OpenOptions;
use std::io::{self, BufRead, Write};

use anyhow::{bail, Context, Result};
use crossterm::cursor::{Hide, MoveToColumn, MoveUp, Show};
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crossterm::style::{Attribute, Print, SetAttribute};
use crossterm::terminal::{self, Clear, ClearType};
use crossterm::{queue, QueueableCommand};
use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32Str};

/// Indices of `items` matching `query`, best first (ties keep input order).
pub fn rank(items: &[String], query: &str) -> Vec<usize> {
    if query.trim().is_empty() {
        return (0..items.len()).collect();
    }
    let mut matcher = Matcher::new(Config::DEFAULT);
    let pat = Pattern::parse(query, CaseMatching::Smart, Normalization::Smart);
    let mut buf = Vec::new();
    let mut scored: Vec<(usize, u32)> = items
        .iter()
        .enumerate()
        .filter_map(|(i, s)| {
            pat.score(Utf32Str::new(s, &mut buf), &mut matcher)
                .map(|sc| (i, sc))
        })
        .collect();
    scored.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    scored.into_iter().map(|(i, _)| i).collect()
}

/// Char positions of `text` matched by `query` (for highlighting).
fn match_positions(text: &str, query: &str) -> Vec<u32> {
    if query.trim().is_empty() {
        return Vec::new();
    }
    let mut matcher = Matcher::new(Config::DEFAULT);
    let pat = Pattern::parse(query, CaseMatching::Smart, Normalization::Smart);
    let mut buf = Vec::new();
    let mut idx = Vec::new();
    pat.indices(Utf32Str::new(text, &mut buf), &mut matcher, &mut idx);
    idx
}

/// Restores the terminal even if drawing fails.
struct RawGuard;
impl RawGuard {
    fn new() -> Result<Self> {
        terminal::enable_raw_mode().context("cannot put the terminal in raw mode")?;
        Ok(Self)
    }
}
impl Drop for RawGuard {
    fn drop(&mut self) {
        let _ = terminal::disable_raw_mode();
    }
}

/// What a key press does to the picker.
#[derive(Debug, PartialEq)]
enum Action {
    Type(char),
    Backspace,
    ClearQuery,
    Up,
    Down,
    Accept,
    Cancel,
    None,
}

fn action(k: KeyEvent) -> Action {
    if k.kind == KeyEventKind::Release {
        return Action::None;
    }
    let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
    match k.code {
        KeyCode::Char('c' | 'g' | 'd') if ctrl => Action::Cancel,
        KeyCode::Char('p' | 'k') if ctrl => Action::Up,
        KeyCode::Char('n' | 'j') if ctrl => Action::Down,
        KeyCode::Char('u') if ctrl => Action::ClearQuery,
        KeyCode::Char(c) if !ctrl => Action::Type(c),
        KeyCode::Backspace => Action::Backspace,
        KeyCode::Up => Action::Up,
        KeyCode::Down | KeyCode::Tab => Action::Down,
        KeyCode::Enter => Action::Accept,
        KeyCode::Esc => Action::Cancel,
        _ => Action::None,
    }
}

fn truncate(s: &str, width: usize) -> String {
    s.chars().take(width).collect()
}

/// Draw the list inline and return the chosen item, or `None` if cancelled.
pub fn interactive(items: &[String], initial: &str, height: u16) -> Result<Option<String>> {
    let mut tty = OpenOptions::new()
        .write(true)
        .open("/dev/tty")
        .context("no terminal (/dev/tty) to draw on")?;
    // A pty without a window size reports 0x0.
    let (cols, rows) = terminal::size()
        .ok()
        .filter(|&(c, r)| c > 0 && r > 0)
        .unwrap_or((80, 24));
    let height = height.clamp(3, rows.saturating_sub(1).max(3));
    let list_rows = usize::from(height) - 1;
    let width = usize::from(cols).saturating_sub(2).max(10);

    let _guard = RawGuard::new()?;
    // Reserve the region below the cursor, then come back to its top.
    for _ in 0..height {
        tty.write_all(b"\n")?;
    }
    tty.queue(MoveUp(height))?.queue(Hide)?;

    let mut query = initial.to_string();
    let mut ranked = rank(items, &query);
    let (mut sel, mut top) = (0usize, 0usize);
    let result = loop {
        sel = sel.min(ranked.len().saturating_sub(1));
        if sel < top {
            top = sel;
        } else if sel >= top + list_rows {
            top = sel + 1 - list_rows;
        }
        // frame
        queue!(
            tty,
            MoveToColumn(0),
            Print(format!("> {query}")),
            SetAttribute(Attribute::Dim),
            Print(format!("  {}/{}", ranked.len(), items.len())),
            SetAttribute(Attribute::Reset),
            Clear(ClearType::UntilNewLine)
        )?;
        for row in 0..list_rows {
            queue!(tty, Print("\r\n"), MoveToColumn(0))?;
            if let Some(&i) = ranked.get(top + row) {
                let text = truncate(&items[i], width);
                let hits = match_positions(&text, &query);
                let selected = top + row == sel;
                queue!(tty, Print(if selected { "▌" } else { " " }))?;
                if selected {
                    queue!(tty, SetAttribute(Attribute::Bold))?;
                }
                for (n, ch) in text.chars().enumerate() {
                    if hits.contains(&(n as u32)) {
                        queue!(
                            tty,
                            SetAttribute(Attribute::Underlined),
                            Print(ch),
                            SetAttribute(Attribute::NoUnderline)
                        )?;
                    } else {
                        queue!(tty, Print(ch))?;
                    }
                }
                queue!(tty, SetAttribute(Attribute::Reset))?;
            }
            queue!(tty, Clear(ClearType::UntilNewLine))?;
        }
        queue!(tty, MoveUp(height - 1), MoveToColumn(0))?;
        tty.flush()?;

        let Event::Key(key) = event::read()? else {
            continue;
        };
        match action(key) {
            Action::Type(c) => {
                query.push(c);
                ranked = rank(items, &query);
                (sel, top) = (0, 0);
            }
            Action::Backspace => {
                query.pop();
                ranked = rank(items, &query);
                (sel, top) = (0, 0);
            }
            Action::ClearQuery => {
                query.clear();
                ranked = rank(items, &query);
                (sel, top) = (0, 0);
            }
            Action::Up => sel = sel.saturating_sub(1),
            Action::Down => sel = (sel + 1).min(ranked.len().saturating_sub(1)),
            Action::Accept => break ranked.get(sel).map(|&i| items[i].clone()),
            Action::Cancel => break None,
            Action::None => {}
        }
    };
    // Wipe the region and give the cursor back.
    for row in 0..height {
        queue!(tty, MoveToColumn(0), Clear(ClearType::CurrentLine))?;
        if row + 1 < height {
            queue!(tty, Print("\r\n"))?;
        }
    }
    queue!(tty, MoveUp(height - 1), MoveToColumn(0), Show)?;
    tty.flush()?;
    Ok(result)
}

/// Arguments of `shellcraft pick`.
#[derive(clap::Args)]
pub struct PickArgs {
    /// Start with this query typed
    #[arg(long)]
    pub query: Option<String>,
    /// Do not open the picker: print the lines matching this query, best first
    #[arg(long)]
    pub filter: Option<String>,
    /// Rows used by the inline list
    #[arg(long, default_value_t = 12)]
    pub height: u16,
    /// With --filter: maximum lines printed (0 = all)
    #[arg(short = 'n', long, default_value_t = 0)]
    pub limit: usize,
}

pub fn run(args: PickArgs) -> Result<()> {
    let PickArgs {
        query,
        filter,
        height,
        limit,
    } = args;
    let items: Vec<String> = io::stdin()
        .lock()
        .lines()
        .collect::<io::Result<Vec<_>>>()
        .context("reading candidates from stdin")?
        .into_iter()
        .filter(|l| !l.is_empty())
        .collect();
    if let Some(q) = filter {
        let mut out = io::stdout().lock();
        for i in rank(&items, &q)
            .into_iter()
            .take(if limit == 0 { usize::MAX } else { limit })
        {
            writeln!(out, "{}", items[i])?;
        }
        return Ok(());
    }
    match interactive(&items, query.as_deref().unwrap_or(""), height)? {
        Some(choice) => {
            println!("{choice}");
            Ok(())
        }
        None => bail!("cancelled"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn items(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn ranks_fuzzy_matches_best_first() {
        let all = items(&[
            "src/main.rs",
            "README.md",
            "src/cli/main_menu.rs",
            "Cargo.toml",
        ]);
        let hits = rank(&all, "main");
        assert_eq!(hits[0], 0, "shorter, tighter match first");
        assert!(hits.contains(&2) && !hits.contains(&1));
        assert_eq!(rank(&all, "").len(), 4);
        assert_eq!(rank(&all, "zzzz"), Vec::<usize>::new());
        assert_eq!(rank(&all, "cgt")[0], 3, "fuzzy subsequence");
    }

    #[test]
    fn smart_case_and_stable_ties() {
        let all = items(&["apple", "Apple", "banana apple"]);
        assert_eq!(
            rank(&all, "Apple"),
            vec![1],
            "uppercase query is case sensitive"
        );
        assert_eq!(rank(&all, "apple")[..2], [0, 1], "ties keep input order");
    }

    #[test]
    fn highlights_matched_chars() {
        let pos = match_positions("src/main.rs", "mn");
        assert_eq!(pos.len(), 2);
        assert!(match_positions("abc", "").is_empty());
    }

    #[test]
    fn keys_map_to_actions() {
        let key = |code, m| KeyEvent::new(code, m);
        assert_eq!(
            action(key(KeyCode::Char('a'), KeyModifiers::NONE)),
            Action::Type('a')
        );
        assert_eq!(
            action(key(KeyCode::Char('c'), KeyModifiers::CONTROL)),
            Action::Cancel
        );
        assert_eq!(
            action(key(KeyCode::Char('n'), KeyModifiers::CONTROL)),
            Action::Down
        );
        assert_eq!(
            action(key(KeyCode::Esc, KeyModifiers::NONE)),
            Action::Cancel
        );
        assert_eq!(
            action(key(KeyCode::Enter, KeyModifiers::NONE)),
            Action::Accept
        );
        assert_eq!(
            action(key(KeyCode::Char('x'), KeyModifiers::CONTROL)),
            Action::None
        );
    }
}
