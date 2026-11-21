//! Drawing: turns the [`App`] state into ratatui widgets. No logic lives here.

use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, List, ListItem, ListState, Paragraph, Wrap},
    Frame,
};

use crate::edit::{diff, Change};
use crate::state::{App, Screen};

const DIM: Style = Style::new().fg(Color::DarkGray);

pub fn draw(f: &mut Frame, app: &App) {
    let [tabs, body, foot] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(3),
        Constraint::Length(1),
    ])
    .areas(f.area());
    let titles: Vec<Span> = Screen::ALL
        .iter()
        .flat_map(|s| {
            let st = if *s == app.screen {
                Style::new().add_modifier(Modifier::REVERSED | Modifier::BOLD)
            } else {
                DIM
            };
            [Span::styled(format!(" {} ", s.title()), st), Span::raw(" ")]
        })
        .collect();
    f.render_widget(Paragraph::new(Line::from(titles)), tabs);
    match app.screen {
        Screen::Welcome => welcome(f, app, body),
        Screen::Features => features(f, app, body),
        Screen::Plugins => plugins(f, app, body),
        Screen::Theme => theme(f, app, body),
        Screen::Review => review(f, app, body),
    }
    let help = match app.screen {
        Screen::Plugins if app.searching => "type to search   Enter/Esc done",
        Screen::Plugins => "↑↓ move   space mark   / search   ←→ screen   q quit",
        Screen::Review => "Enter apply   ←→ screen   q quit",
        _ => "↑↓ move   space select   ←→ screen   q quit",
    };
    f.render_widget(Paragraph::new(help).style(DIM), foot);
}

fn welcome(f: &mut Frame, app: &App, area: Rect) {
    let lines = vec![
        Line::from("shellcraft setup").style(Style::new().add_modifier(Modifier::BOLD)),
        Line::from(""),
        Line::from(format!("shell:   {}", app.snapshot.shell)),
        Line::from(format!("config:  {}", app.snapshot.config_path)),
        Line::from(""),
        Line::from("Choose features, plugins and a theme, then review and apply."),
        Line::from("Nothing is written until you apply."),
        Line::from(""),
        Line::from("Enter to start.").style(DIM),
    ];
    f.render_widget(
        Paragraph::new(lines).block(Block::bordered().title(" Welcome ")),
        area,
    );
}

fn list(f: &mut Frame, area: Rect, title: &str, items: Vec<ListItem>, cursor: usize) {
    let mut st = ListState::default().with_selected(Some(cursor));
    let l = List::new(items)
        .block(Block::bordered().title(format!(" {title} ")))
        .highlight_style(Style::new().add_modifier(Modifier::REVERSED));
    f.render_stateful_widget(l, area, &mut st);
}

fn features(f: &mut Frame, app: &App, area: Rect) {
    let [l, r] =
        Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)]).areas(area);
    let items = (0..app.toggles.len())
        .map(|i| {
            let (name, _) = app.toggle_text(i);
            ListItem::new(format!(
                "[{}] {name}",
                if app.toggles[i].on { "x" } else { " " }
            ))
        })
        .collect();
    list(f, l, "Features", items, app.cursor);
    let (_, help) = app.toggle_text(app.cursor.min(app.toggles.len() - 1));
    f.render_widget(
        Paragraph::new(help)
            .wrap(Wrap { trim: true })
            .block(Block::bordered().title(" About ")),
        r,
    );
}

fn plugins(f: &mut Frame, app: &App, area: Rect) {
    let [top, rest] = Layout::vertical([Constraint::Length(3), Constraint::Min(3)]).areas(area);
    let q = if app.searching {
        format!("{}▏", app.search)
    } else {
        app.search.clone()
    };
    f.render_widget(
        Paragraph::new(q).block(Block::bordered().title(" Search ( / ) ")),
        top,
    );
    let [l, r] =
        Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)]).areas(rest);
    let vis = app.visible_plugins();
    let items = vis
        .iter()
        .map(|p| {
            let state = if p.installed {
                "installed"
            } else {
                "available"
            };
            ListItem::new(format!("{} {:<14} {state}", app.mark(&p.name), p.name))
        })
        .collect();
    list(f, l, "Plugins  (+ install, - remove)", items, app.cursor);
    let detail = vis.get(app.cursor).map_or_else(
        || "No plugins match.".to_string(),
        |p| {
            let note = if p.note.is_empty() {
                String::new()
            } else {
                format!("\n\n{}", p.note)
            };
            format!("{} {}\n\n{}{note}", p.name, p.version, p.description)
        },
    );
    f.render_widget(
        Paragraph::new(detail)
            .wrap(Wrap { trim: true })
            .block(Block::bordered().title(" Details ")),
        r,
    );
}

fn theme(f: &mut Frame, app: &App, area: Rect) {
    let [l, r] =
        Layout::horizontal([Constraint::Percentage(40), Constraint::Percentage(60)]).areas(area);
    let items = app
        .theme_choices()
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let more = if app.theme_needs_install(i) {
                "  (install)"
            } else {
                ""
            };
            ListItem::new(format!(
                "({}) {t}{more}",
                if i == app.theme { "•" } else { " " }
            ))
        })
        .collect();
    list(f, l, "Theme", items, app.cursor);
    let c = swatch(&app.previewed_theme());
    let prompt = Line::from(vec![
        Span::styled("~/code/app", Style::new().fg(c.0)),
        Span::raw(" "),
        Span::styled("main*", Style::new().fg(c.1)),
        Span::raw(" "),
        Span::styled("node 20", Style::new().fg(c.2)),
        Span::raw(" "),
        Span::styled("❯", Style::new().fg(c.3)),
    ]);
    f.render_widget(
        Paragraph::new(vec![
            Line::from(""),
            prompt,
            Line::from(""),
            Line::from("preview of the prompt").style(DIM),
        ])
        .block(Block::bordered().title(format!(" {} ", app.previewed_theme()))),
        r,
    );
}

/// Preview colours (dir, git, version, ok) for a theme name; matches the themes in core closely enough to judge.
fn swatch(name: &str) -> (Color, Color, Color, Color) {
    match name {
        "nord" => (
            Color::Rgb(136, 192, 208),
            Color::Rgb(180, 142, 173),
            Color::Rgb(163, 190, 140),
            Color::Rgb(163, 190, 140),
        ),
        "dracula" => (
            Color::Rgb(189, 147, 249),
            Color::Rgb(255, 121, 198),
            Color::Rgb(241, 250, 140),
            Color::Rgb(80, 250, 123),
        ),
        "gruvbox" => (
            Color::Rgb(131, 165, 152),
            Color::Rgb(211, 134, 155),
            Color::Rgb(250, 189, 47),
            Color::Rgb(184, 187, 38),
        ),
        "passionp" => (Color::Cyan, Color::Cyan, Color::Green, Color::Green),
        _ => (Color::Blue, Color::Magenta, Color::Green, Color::Green),
    }
}

fn review(f: &mut Frame, app: &App, area: Rect) {
    let plan = app.plan();
    let mut lines: Vec<Line> = Vec::new();
    for n in &plan.install {
        lines
            .push(Line::from(format!("+ install plugin {n}")).style(Style::new().fg(Color::Green)));
    }
    for n in &plan.remove {
        lines.push(Line::from(format!("- remove plugin {n}")).style(Style::new().fg(Color::Red)));
    }
    for c in diff(&app.snapshot.config_text, &plan.config_text) {
        match c {
            Change::Added(l) => {
                lines.push(Line::from(format!("+ {l}")).style(Style::new().fg(Color::Green)))
            }
            Change::Removed(l) => {
                lines.push(Line::from(format!("- {l}")).style(Style::new().fg(Color::Red)))
            }
            Change::Same(_) => {}
        }
    }
    if lines.is_empty() {
        lines.push(Line::from("No changes."));
    }
    f.render_widget(
        Paragraph::new(lines).block(Block::bordered().title(" Review ")),
        area,
    );
}
