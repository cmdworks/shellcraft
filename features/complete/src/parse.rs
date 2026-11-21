//! Parsing the text printed by `--help` into flags and subcommands.

/// Flags and subcommands of one command (or subcommand).
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Spec {
    pub subcommands: Vec<(String, String)>,
    pub flags: Vec<(String, String)>,
}

fn is_name(s: &str) -> bool {
    s.starts_with(|c: char| c.is_ascii_alphanumeric())
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || "_.:-".contains(c))
        && s.len() <= 40
}

fn short(desc: &str) -> String {
    let d: String = desc
        .trim()
        .chars()
        .filter(|c| !c.is_control())
        .take(70)
        .collect();
    d.trim_end().to_string()
}

/// Split `  <left>  <right>` at the first run of two or more spaces (after the indentation).
fn two_columns(line: &str) -> (&str, &str) {
    let body = line.trim_start();
    match body.find("  ") {
        Some(i) => (&body[..i], body[i..].trim()),
        None => (body, ""),
    }
}

/// Parse the text printed by `--help` (clap, cobra, argparse, GNU and git style).
pub fn parse_help(text: &str) -> Spec {
    let mut spec = Spec::default();
    let mut in_commands = false;
    for line in text.lines() {
        let line = line.trim_end();
        if line.trim().is_empty() {
            continue;
        }
        let indented = line.starts_with([' ', '\t']);
        if !indented {
            // A heading (`Commands:`, `OPTIONS`) switches section; other unindented lines are group
            // labels inside a section (git prints "start a working area") and change nothing.
            let t = line.trim();
            let upper = t.chars().any(|c| c.is_alphabetic()) && t == t.to_uppercase();
            if t.ends_with(':') || upper {
                in_commands = t.to_lowercase().contains("command");
            }
            continue;
        }
        let (left, right) = two_columns(line);
        if left.starts_with('-') {
            // `-v, --verbose`, `--output=FILE`, `-o <FILE>`, `--[no-]color`
            for tok in left
                .split([',', ' '])
                .map(str::trim)
                .filter(|t| t.starts_with('-'))
            {
                let name: String = tok.chars().take_while(|c| !"=[<,".contains(*c)).collect();
                let name = name.trim_end_matches(['.', ';']);
                let valid = name.len() >= 2
                    && name != "--"
                    && name
                        .chars()
                        .skip(1)
                        .all(|c| c.is_ascii_alphanumeric() || c == '-')
                    && !spec.flags.iter().any(|(f, _)| f == name);
                if valid {
                    spec.flags.push((name.to_string(), short(right)));
                }
            }
        } else if in_commands {
            // `  name  description`, `  ls, list  description`
            let names: Vec<&str> = left
                .split(',')
                .map(|n| n.trim().trim_end_matches(':'))
                .collect();
            if names.iter().all(|n| is_name(n)) {
                for n in names {
                    if !spec.subcommands.iter().any(|(s, _)| s == n) {
                        spec.subcommands.push((n.to_string(), short(right)));
                    }
                }
            }
        }
    }
    spec
}

#[cfg(test)]
mod tests {
    use super::*;

    const CLAP: &str = "Fast tool\n\nUsage: tool [OPTIONS] <COMMAND>\n\nCommands:\n  init     Write a default config\n  apply    Generate the init script\n  ls, list  List things\n  help     Print this message\n\nOptions:\n  -v, --verbose        Be loud\n      --config <FILE>  Config path\n  -h, --help           Print help\n  -V, --version        Print version\n";
    const COBRA: &str = "A tool for containers\n\nUsage:\n  ctl [command]\n\nAvailable Commands:\n  build       Build an image\n  completion  Generate the autocompletion script\n  run         Run a container\n\nFlags:\n  -h, --help      help for ctl\n      --debug     Enable debug mode\n  -c, --context string   Context name\n\nUse \"ctl [command] --help\" for more information.\n";
    const GNU: &str = "Usage: grepish [OPTION]... PATTERN [FILE]...\nSearch for PATTERN.\n\nPattern selection:\n  -E, --extended-regexp     PATTERN is an extended regular expression\n  -i, --ignore-case         ignore case distinctions\n      --[no-]color          use markers\n  -m, --max-count=NUM       stop after NUM selected lines\n";
    const GH: &str = "Work with pull requests.\n\nUSAGE\n  gh pr <command> [flags]\n\nGENERAL COMMANDS\n  create:  Create a pull request\n  list:    List pull requests\n\nFLAGS\n  -R, --repo [HOST/]OWNER/REPO   Select another repository\n";
    const GIT: &str = "usage: gitish [-v | --version] [--help] <command> [<args>]\n\nThese are common Gitish commands used in various situations:\n\nstart a working area\n   clone     Clone a repository into a new directory\n   init      Create an empty repository\n\nwork on the current change\n   add       Add file contents to the index\n";

    fn names(v: &[(String, String)]) -> Vec<&str> {
        v.iter().map(|(n, _)| n.as_str()).collect()
    }

    #[test]
    fn parses_clap_cobra_gnu_and_git_styles() {
        let s = parse_help(CLAP);
        assert_eq!(
            names(&s.subcommands),
            ["init", "apply", "ls", "list", "help"]
        );
        assert_eq!(
            names(&s.flags),
            [
                "-v",
                "--verbose",
                "--config",
                "-h",
                "--help",
                "-V",
                "--version"
            ]
        );
        assert_eq!(s.flags[1].1, "Be loud");
        let s = parse_help(COBRA);
        assert_eq!(names(&s.subcommands), ["build", "completion", "run"]);
        assert!(
            names(&s.flags).contains(&"--debug")
                && names(&s.flags).contains(&"-c")
                && names(&s.flags).contains(&"--context")
        );
        let s = parse_help(GNU);
        assert!(s.subcommands.is_empty(), "option groups are not commands");
        assert_eq!(
            names(&s.flags),
            [
                "-E",
                "--extended-regexp",
                "-i",
                "--ignore-case",
                "-m",
                "--max-count"
            ]
        );
        let s = parse_help(GH);
        assert_eq!(names(&s.subcommands), ["create", "list"]);
        assert!(names(&s.flags).contains(&"--repo"));
        let s = parse_help(GIT);
        assert_eq!(names(&s.subcommands), ["clone", "init", "add"]);
    }

    #[test]
    fn name_rules() {
        assert!(is_name("ls") && is_name("remote-add") && is_name("a.b"));
        assert!(!is_name("-x") && !is_name("A thing") && !is_name(""));
    }
}
