//! The setup manager as a state machine: keys in, state out. No terminal, no I/O, so every screen
//! and transition is unit tested.

use std::collections::BTreeSet;

use shellcraft_core::Config;

use crate::edit;

/// A plugin as the manager shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginInfo {
    pub name: String,
    pub version: String,
    pub description: String,
    /// Ships inside the binary: can be switched on or off, not installed or removed.
    pub bundled: bool,
    pub installed: bool,
    /// Why an installed plugin is not loaded, if it is not.
    pub note: String,
}

/// Everything the manager needs to know, gathered by the caller.
#[derive(Debug, Clone)]
pub struct Snapshot {
    pub config_path: String,
    pub config_text: String,
    pub shell: String,
    pub themes: Vec<String>,
    pub plugins: Vec<PluginInfo>,
}

/// Keys the manager understands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Up,
    Down,
    Left,
    Right,
    Enter,
    Esc,
    Backspace,
    Char(char),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Welcome,
    Features,
    Plugins,
    Theme,
    Review,
}

impl Screen {
    pub const ALL: [Screen; 5] = [
        Screen::Welcome,
        Screen::Features,
        Screen::Plugins,
        Screen::Theme,
        Screen::Review,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Screen::Welcome => "Welcome",
            Screen::Features => "Features",
            Screen::Plugins => "Plugins",
            Screen::Theme => "Theme",
            Screen::Review => "Review",
        }
    }
}

/// What the user decided; the caller carries it out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Cancelled,
    Apply(Plan),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    /// The new text of `shellcraft.toml`.
    pub config_text: String,
    pub install: Vec<String>,
    pub remove: Vec<String>,
}

/// A feature switch backed by one config key.
#[derive(Debug, Clone)]
pub struct Toggle {
    pub label: &'static str,
    pub help: &'static str,
    pub section: String,
    pub key: &'static str,
    pub on: bool,
    /// The value at the start, to tell whether it changed.
    pub original: bool,
}

pub struct App {
    pub snapshot: Snapshot,
    pub screen: Screen,
    pub toggles: Vec<Toggle>,
    pub cursor: usize,
    pub theme: usize,
    original_theme: usize,
    pub search: String,
    pub searching: bool,
    marks_install: BTreeSet<String>,
    marks_remove: BTreeSet<String>,
    pub outcome: Option<Outcome>,
}

impl App {
    pub fn new(snapshot: Snapshot) -> Self {
        let cfg = Config::from_toml(&snapshot.config_text).unwrap_or_default();
        let mut toggles = vec![
            Toggle { label: "Built-in prompt", help: "shellcraft draws the prompt (git, versions, duration); off keeps starship", section: "prompt".into(), key: "builtin", on: cfg.prompt.builtin, original: cfg.prompt.builtin },
            Toggle { label: "Inline suggestions", help: "dim completion of what you type, from your history (zsh)", section: "suggest".into(), key: "inline", on: cfg.suggest.inline, original: cfg.suggest.inline },
            Toggle { label: "Completion from --help", help: "flags and subcommands for commands without completion", section: "completion".into(), key: "help", on: cfg.completion.help, original: cfg.completion.help },
            Toggle { label: "Syntax highlighting", help: "colour the command line while you type: commands, options, strings, variables (zsh)", section: "suggest".into(), key: "highlight", on: cfg.suggest.highlight, original: cfg.suggest.highlight },
        ];
        for p in snapshot.plugins.iter().filter(|p| p.bundled) {
            let on = cfg.plugin.get(&p.name).map_or(true, |s| s.enabled);
            toggles.push(Toggle {
                label: "",
                help: "",
                section: format!("plugin.{}", p.name),
                key: "enabled",
                on,
                original: on,
            });
        }
        let theme = snapshot
            .themes
            .iter()
            .position(|t| *t == cfg.theme.name)
            .unwrap_or(0);
        Self {
            snapshot,
            screen: Screen::Welcome,
            toggles,
            cursor: 0,
            theme,
            original_theme: theme,
            search: String::new(),
            searching: false,
            marks_install: BTreeSet::new(),
            marks_remove: BTreeSet::new(),
            outcome: None,
        }
    }

    /// Label of toggle `i` (bundled plugin toggles use the plugin's name and description).
    pub fn toggle_text(&self, i: usize) -> (String, String) {
        let t = &self.toggles[i];
        if t.label.is_empty() {
            let name = t.section.trim_start_matches("plugin.");
            let desc = self
                .snapshot
                .plugins
                .iter()
                .find(|p| p.name == name)
                .map(|p| p.description.clone())
                .unwrap_or_default();
            (format!("Plugin: {name}"), desc)
        } else {
            (t.label.to_string(), t.help.to_string())
        }
    }

    /// Plugins matching the search, in display order.
    pub fn visible_plugins(&self) -> Vec<&PluginInfo> {
        let q = self.search.to_lowercase();
        self.snapshot
            .plugins
            .iter()
            .filter(|p| !p.bundled)
            .filter(|p| {
                q.is_empty()
                    || p.name.to_lowercase().contains(&q)
                    || p.description.to_lowercase().contains(&q)
            })
            .collect()
    }

    /// `+` (will install), `-` (will remove) or ` ` for plugin `name`.
    pub fn mark(&self, name: &str) -> char {
        if self.marks_install.contains(name) {
            '+'
        } else if self.marks_remove.contains(name) {
            '-'
        } else {
            ' '
        }
    }

    fn list_len(&self) -> usize {
        match self.screen {
            Screen::Features => self.toggles.len(),
            Screen::Plugins => self.visible_plugins().len(),
            Screen::Theme => self.snapshot.themes.len(),
            _ => 0,
        }
    }

    fn go(&mut self, to: Screen) {
        self.screen = to;
        self.cursor = if to == Screen::Theme { self.theme } else { 0 };
        self.searching = false;
    }

    /// Config text with every choice applied.
    pub fn new_config_text(&self) -> String {
        let mut text = self.snapshot.config_text.clone();
        for t in self.toggles.iter().filter(|t| t.on != t.original) {
            text = edit::set_value(
                &text,
                &t.section,
                t.key,
                if t.on { "true" } else { "false" },
            );
        }
        if self.theme != self.original_theme {
            if let Some(name) = self.snapshot.themes.get(self.theme) {
                text = edit::set_value(&text, "theme", "name", &format!("\"{name}\""));
            }
        }
        text
    }

    pub fn plan(&self) -> Plan {
        Plan {
            config_text: self.new_config_text(),
            install: self.marks_install.iter().cloned().collect(),
            remove: self.marks_remove.iter().cloned().collect(),
        }
    }

    /// Does applying change anything?
    pub fn has_changes(&self) -> bool {
        let p = self.plan();
        p.config_text != self.snapshot.config_text || !p.install.is_empty() || !p.remove.is_empty()
    }

    fn toggle_plugin_mark(&mut self) {
        let Some(p) = self
            .visible_plugins()
            .get(self.cursor)
            .map(|p| (p.name.clone(), p.installed))
        else {
            return;
        };
        let (name, installed) = p;
        let (on, off) = if installed {
            (&mut self.marks_remove, &mut self.marks_install)
        } else {
            (&mut self.marks_install, &mut self.marks_remove)
        };
        off.remove(&name);
        if !on.remove(&name) {
            on.insert(name);
        }
    }

    pub fn handle(&mut self, key: Key) {
        if self.outcome.is_some() {
            return;
        }
        // typing in the plugin search box
        if self.searching {
            match key {
                Key::Esc | Key::Enter => self.searching = false,
                Key::Backspace => {
                    self.search.pop();
                    self.cursor = 0;
                }
                Key::Char(c) => {
                    self.search.push(c);
                    self.cursor = 0;
                }
                _ => {}
            }
            return;
        }
        let n = self.list_len();
        match key {
            Key::Char('q') => self.outcome = Some(Outcome::Cancelled),
            Key::Esc => {
                if self.screen == Screen::Welcome {
                    self.outcome = Some(Outcome::Cancelled);
                } else {
                    self.go(Screen::Welcome);
                }
            }
            Key::Right | Key::Char('\t') | Key::Char('l') => {
                let i = Screen::ALL
                    .iter()
                    .position(|s| *s == self.screen)
                    .unwrap_or(0);
                self.go(Screen::ALL[(i + 1).min(Screen::ALL.len() - 1)]);
            }
            Key::Left | Key::Char('h') => {
                let i = Screen::ALL
                    .iter()
                    .position(|s| *s == self.screen)
                    .unwrap_or(0);
                self.go(Screen::ALL[i.saturating_sub(1)]);
            }
            Key::Down | Key::Char('j') if n > 0 => self.cursor = (self.cursor + 1).min(n - 1),
            Key::Up | Key::Char('k') => self.cursor = self.cursor.saturating_sub(1),
            Key::Char('/') if self.screen == Screen::Plugins => {
                self.searching = true;
            }
            Key::Char(' ') | Key::Enter => match self.screen {
                Screen::Welcome => self.go(Screen::Features),
                Screen::Features => {
                    if let Some(t) = self.toggles.get_mut(self.cursor) {
                        t.on = !t.on;
                    }
                }
                Screen::Plugins => self.toggle_plugin_mark(),
                Screen::Theme => self.theme = self.cursor,
                Screen::Review => {
                    if key == Key::Enter {
                        self.outcome = Some(if self.has_changes() {
                            Outcome::Apply(self.plan())
                        } else {
                            Outcome::Cancelled
                        });
                    }
                }
            },
            Key::Char('a') if self.screen == Screen::Review => {
                self.outcome = Some(if self.has_changes() {
                    Outcome::Apply(self.plan())
                } else {
                    Outcome::Cancelled
                });
            }
            _ => {}
        }
        // moving over the theme list previews it; the choice is made with Enter or Space
        let _ = self.snapshot.themes.len();
    }

    pub fn keys(&mut self, keys: &str) {
        for c in keys.chars() {
            self.handle(match c {
                '\n' => Key::Enter,
                '\u{1b}' => Key::Esc,
                c => Key::Char(c),
            });
        }
    }

    /// Theme being previewed: under the cursor on the theme screen, else the chosen one.
    pub fn previewed_theme(&self) -> &str {
        let i = if self.screen == Screen::Theme {
            self.cursor
        } else {
            self.theme
        };
        self.snapshot
            .themes
            .get(i)
            .map_or("default", String::as_str)
    }
}

/// A fixed snapshot for tests in this crate.
#[cfg(test)]
pub(crate) fn tests_snapshot(config: &str) -> Snapshot {
    let p = |n: &str, bundled: bool, installed: bool| PluginInfo {
        name: n.into(),
        version: "1.0".into(),
        description: format!("{n} plugin"),
        bundled,
        installed,
        note: String::new(),
    };
    Snapshot {
        config_path: "/c/shellcraft.toml".into(),
        config_text: config.into(),
        shell: "zsh".into(),
        themes: vec!["default".into(), "nord".into(), "dracula".into()],
        plugins: vec![
            p("git", true, true),
            p("docker", true, true),
            p("todos", false, false),
            p("notes", false, true),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snap(config: &str) -> Snapshot {
        tests_snapshot(config)
    }

    #[test]
    fn starts_on_welcome_with_state_from_the_config() {
        let app = App::new(snap("[theme]\nname = \"nord\"\n[suggest]\ninline = false\n[plugin.docker]\nenabled = false\n"));
        assert_eq!(app.screen, Screen::Welcome);
        assert_eq!(app.theme, 1);
        let t = |k: &str| {
            app.toggles
                .iter()
                .find(|t| t.section.ends_with(k))
                .expect("toggle")
                .on
        };
        assert!(!t("suggest") && t("prompt") && !t("docker") && t("git"));
        assert!(!app.has_changes());
    }

    #[test]
    fn navigation_between_screens_and_quit() {
        let mut app = App::new(snap(""));
        app.keys("\n");
        assert_eq!(app.screen, Screen::Features, "Enter on welcome starts");
        app.handle(Key::Right);
        app.handle(Key::Right);
        assert_eq!(app.screen, Screen::Theme);
        app.handle(Key::Left);
        assert_eq!(app.screen, Screen::Plugins);
        app.handle(Key::Esc);
        assert_eq!(app.screen, Screen::Welcome, "Esc goes home first");
        app.handle(Key::Esc);
        assert_eq!(app.outcome, Some(Outcome::Cancelled));
        let mut q = App::new(snap(""));
        q.keys("q");
        assert_eq!(q.outcome, Some(Outcome::Cancelled));
    }

    #[test]
    fn toggles_theme_and_plugin_marks_produce_a_plan() {
        let mut app = App::new(snap("[shell]\nhistory_size = 5\n"));
        app.keys("\n"); // features
        app.keys(" "); // built-in prompt off
        app.handle(Key::Down);
        app.handle(Key::Down);
        app.keys(" "); // completion off
        app.handle(Key::Right); // plugins
        assert_eq!(
            app.visible_plugins().len(),
            2,
            "bundled plugins are not in the browser"
        );
        app.keys(" "); // todos: install
        app.handle(Key::Down);
        app.keys(" "); // notes: installed, so remove
        assert_eq!((app.mark("todos"), app.mark("notes")), ('+', '-'));
        app.keys(" "); // toggling again clears the mark
        assert_eq!(app.mark("notes"), ' ');
        app.keys(" ");
        app.handle(Key::Right); // theme
        app.handle(Key::Down);
        app.handle(Key::Down);
        assert_eq!(app.previewed_theme(), "dracula", "moving previews");
        app.keys(" ");
        app.handle(Key::Right); // review
        let Plan {
            config_text,
            install,
            remove,
        } = app.plan();
        assert_eq!(
            (install, remove),
            (vec!["todos".to_string()], vec!["notes".to_string()])
        );
        assert!(
            config_text.contains("[prompt]\nbuiltin = false"),
            "{config_text}"
        );
        assert!(
            config_text.contains("[completion]\nhelp = false")
                && config_text.contains("name = \"dracula\"")
        );
        assert!(Config::from_toml(&config_text).is_ok());
        app.keys("\n");
        assert!(matches!(app.outcome, Some(Outcome::Apply(_))));
    }

    #[test]
    fn nothing_changed_means_nothing_to_apply() {
        let mut app = App::new(snap(""));
        app.handle(Key::Right);
        app.handle(Key::Right);
        app.handle(Key::Right);
        app.handle(Key::Right);
        assert_eq!(app.screen, Screen::Review);
        app.keys("\n");
        assert_eq!(app.outcome, Some(Outcome::Cancelled));
    }

    #[test]
    fn search_filters_plugins_and_esc_leaves_the_box() {
        let mut app = App::new(snap(""));
        app.handle(Key::Right);
        app.handle(Key::Right);
        app.keys("/not");
        assert!(app.searching);
        assert_eq!(
            app.visible_plugins()
                .iter()
                .map(|p| p.name.as_str())
                .collect::<Vec<_>>(),
            vec!["notes"]
        );
        app.handle(Key::Backspace);
        app.handle(Key::Backspace);
        app.handle(Key::Backspace);
        assert_eq!(app.visible_plugins().len(), 2);
        app.handle(Key::Esc);
        assert!(
            !app.searching && app.screen == Screen::Plugins,
            "Esc ends the search, it does not leave the screen"
        );
        app.keys("q");
        assert_eq!(app.outcome, Some(Outcome::Cancelled));
    }
}
