//! `shellcraft cheat`: short built-in cheat sheets.

use anyhow::{bail, Result};

/// The sheet in the binary; the others come from cheat plugins (`[cheat.<topic>]`, baked into
/// `cheats.tsv` as `topic<TAB>path`).
const SHEETS: &[(&str, &str)] = &[("shellcraft", include_str!("../cheats/shellcraft.txt"))];

fn plugin_sheets() -> Vec<(String, String)> {
    let Some(dir) = shellcraft_core::paths::cache_dir(|k| std::env::var(k).ok()) else {
        return Vec::new();
    };
    std::fs::read_to_string(dir.join("cheats.tsv"))
        .unwrap_or_default()
        .lines()
        .filter_map(|l| l.split_once('\t'))
        .map(|(t, p)| (t.to_string(), p.to_string()))
        .collect()
}

/// Arguments of `shellcraft cheat`.
#[derive(clap::Args)]
pub struct CheatArgs {
    pub topic: Option<String>,
}

pub fn run(args: CheatArgs) -> Result<()> {
    let extra = plugin_sheets();
    let mut topics: Vec<String> = SHEETS.iter().map(|(n, _)| (*n).to_string()).collect();
    topics.extend(extra.iter().map(|(t, _)| t.clone()));
    topics.sort();
    topics.dedup();
    let Some(t) = args.topic else {
        for n in &topics {
            println!("{n}");
        }
        return Ok(());
    };
    if let Some((_, text)) = SHEETS.iter().find(|(n, _)| *n == t) {
        print!("{text}");
        return Ok(());
    }
    match extra.iter().find(|(n, _)| *n == t) {
        Some((_, path)) => {
            print!("{}", std::fs::read_to_string(path)?);
            Ok(())
        }
        None => bail!(
            "no cheat sheet for `{t}` (available: {}; more: `shellcraft plugin add cheats`)",
            topics.join(", ")
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_sheet_starts_with_its_name() {
        for (name, text) in SHEETS {
            assert_eq!(text.lines().next(), Some(*name));
        }
        assert!(run(CheatArgs {
            topic: Some("nope".into())
        })
        .is_err());
    }
}
