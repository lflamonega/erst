use std::time::Duration;

use anyhow::Result;
use bollard::Docker;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::DefaultTerminal;
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap};

use crate::app::{self, InstalledApp};
use crate::catalog;
use crate::commands;

/// How often the app list is refreshed on its own.
const REFRESH: Duration = Duration::from_secs(5);

/// How many log lines the log view keeps.
const LOG_LINES: usize = 200;

/// Open the dashboard.
pub async fn run(docker: Docker) -> Result<()> {
    let mut terminal = setup()?;

    let mut state = State {
        docker,
        screen: Screen::Apps,
        apps: Vec::new(),
        selected: 0,
        catalog_selected: 0,
        logs: Logs::default(),
        notice: None,
        busy: None,
        root_only: false,
    };

    let outcome = event_loop(&mut terminal, &mut state).await;

    restore(&mut terminal)?;
    outcome
}

/// Which screen has the focus.
#[derive(Debug, PartialEq)]
enum Screen {
    Apps,
    Catalog,
    Logs,
    Help,
}

/// Log lines of the app being inspected.
#[derive(Default)]
struct Logs {
    app: String,
    lines: Vec<String>,
    scroll: u16,
}

/// Everything the UI needs to draw and act.
struct State {
    docker: Docker,
    screen: Screen,
    apps: Vec<InstalledApp>,
    selected: usize,
    catalog_selected: usize,
    logs: Logs,
    notice: Option<String>,
    busy: Option<String>,
    root_only: bool,
}

impl State {
    fn selected_app(&self) -> Option<&InstalledApp> {
        self.apps.get(self.selected)
    }

    /// Reload the app list from the runtime.
    async fn refresh(&mut self) {
        let Ok(mut apps) = app::list(&self.docker).await else {
            return;
        };
        apps.sort_by(|a, b| {
            b.running
                .cmp(&a.running)
                .then_with(|| a.settings.name.cmp(&b.settings.name))
        });
        if self.root_only {
            apps.retain(|app| app.settings.runs_as_root());
        }
        self.selected = self.selected.min(apps.len().saturating_sub(1));
        self.apps = apps;
    }
}

async fn event_loop(terminal: &mut DefaultTerminal, state: &mut State) -> Result<()> {
    state.refresh().await;
    let mut last_refresh = std::time::Instant::now();

    loop {
        terminal.draw(|frame| draw(frame, state))?;

        if event::poll(Duration::from_millis(200))?
            && let Event::Key(key) = event::read()?
            && key.kind == KeyEventKind::Press
            && handle_key(terminal, state, key).await?
        {
            return Ok(());
        }

        // Refresh on a timer rather than after every action.
        if last_refresh.elapsed() >= REFRESH && state.busy.is_none() {
            state.refresh().await;
            last_refresh = std::time::Instant::now();
        }
    }
}

/// Returns `true` when the user asked to quit.
async fn handle_key(
    terminal: &mut DefaultTerminal,
    state: &mut State,
    key: KeyEvent,
) -> Result<bool> {
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
        return Ok(true);
    }

    match state.screen {
        Screen::Help => {
            state.screen = Screen::Apps;
            return Ok(false);
        }
        Screen::Logs => {
            match key.code {
                KeyCode::Esc | KeyCode::Char('q') | KeyCode::Enter => state.screen = Screen::Apps,
                KeyCode::Down | KeyCode::Char('j') => {
                    state.logs.scroll = state.logs.scroll.saturating_add(1)
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    state.logs.scroll = state.logs.scroll.saturating_sub(1)
                }
                _ => {}
            }
            return Ok(false);
        }
        Screen::Catalog => {
            match key.code {
                KeyCode::Esc | KeyCode::Char('q') => state.screen = Screen::Apps,
                KeyCode::Down | KeyCode::Char('j') => {
                    state.catalog_selected = (state.catalog_selected + 1)
                        .min(catalog::entries().len().saturating_sub(1));
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    state.catalog_selected = state.catalog_selected.saturating_sub(1);
                }
                KeyCode::Enter => install_selected(terminal, state).await?,
                _ => {}
            }
            return Ok(false);
        }
        Screen::Apps => {}
    }

    let Some(app) = state.selected_app() else {
        return Ok(matches!(key.code, KeyCode::Char('q') | KeyCode::Esc));
    };
    let name = app.settings.name.clone();

    match key.code {
        KeyCode::Char('q') | KeyCode::Esc => return Ok(true),
        KeyCode::Char('?') => state.screen = Screen::Help,
        KeyCode::Char('r') => state.screen = Screen::Catalog,
        KeyCode::Char('R') => {
            state.root_only = !state.root_only;
            state.selected = 0;
            state.refresh().await;
        }
        KeyCode::Down | KeyCode::Char('j') => move_selection(state, 1),
        KeyCode::Up | KeyCode::Char('k') => move_selection(state, -1),
        KeyCode::Enter | KeyCode::Char('l') => load_logs(terminal, state, &name).await?,
        KeyCode::Char('s') => {
            let docker = state.docker.clone();
            act(
                terminal,
                state,
                format!("Starting {name}"),
                commands::start(&docker, &name),
            )
            .await?;
        }
        KeyCode::Char('x') => {
            let docker = state.docker.clone();
            act(
                terminal,
                state,
                format!("Stopping {name}"),
                commands::stop(&docker, &name),
            )
            .await?;
        }
        KeyCode::Char('u') => {
            let docker = state.docker.clone();
            act(
                terminal,
                state,
                format!("Updating {name}"),
                commands::update(&docker, &name, &mut |_| {}),
            )
            .await?;
        }
        KeyCode::Char('b') => {
            let docker = state.docker.clone();
            let destination = backup_path(&name);
            let message = act(
                terminal,
                state,
                format!("Backing up {name}"),
                commands::backup(&docker, &name, &destination, &mut |_| {}),
            )
            .await?;
            state.notice = Some(message.replace('\n', " "));
        }
        KeyCode::Char('d') => {
            // Keep a copy of the data before removing the app, otherwise the
            // volume goes away with it.
            let docker = state.docker.clone();
            let destination = backup_path(&name);
            let backed_up = act(
                terminal,
                state,
                format!("Backing up {name} before removing it"),
                commands::backup(&docker, &name, &destination, &mut |_| {}),
            )
            .await;

            if backed_up.is_ok() {
                act(
                    terminal,
                    state,
                    format!("Removing {name}"),
                    commands::remove(&docker, &name, false),
                )
                .await?;
                state.notice = Some(format!(
                    "{name} removed, backup at {}",
                    destination.display()
                ));
            }
        }
        _ => {}
    }
    Ok(false)
}

fn move_selection(state: &mut State, delta: isize) {
    if state.apps.is_empty() {
        return;
    }
    let last = state.apps.len() - 1;
    state.selected = state.selected.saturating_add_signed(delta).min(last);
}

/// Run an operation, redrawing with a busy message while it works, and show
/// whatever the command reported in the status line.
async fn act<F>(
    terminal: &mut DefaultTerminal,
    state: &mut State,
    label: String,
    run: F,
) -> Result<String>
where
    F: std::future::Future<Output = anyhow::Result<String>>,
{
    state.busy = Some(label);
    draw_now(terminal, state)?;

    let message = match run.await {
        Ok(message) => message.replace('\n', " "),
        Err(error) => format!("{error}"),
    };
    state.busy = None;
    state.notice = Some(message.clone());
    state.refresh().await;
    Ok(message)
}

async fn install_selected(terminal: &mut DefaultTerminal, state: &mut State) -> Result<()> {
    let Some(entry) = catalog::entries().get(state.catalog_selected) else {
        return Ok(());
    };
    let (name, image) = (entry.name.to_string(), entry.image.to_string());

    state.screen = Screen::Apps;
    let docker = state.docker.clone();
    act(
        terminal,
        state,
        format!("Installing {name} ({image})"),
        commands::install_entry(&docker, entry, &mut |_| {}),
    )
    .await?;
    Ok(())
}

/// Read the tail of an app's logs into the log view.
async fn load_logs(terminal: &mut DefaultTerminal, state: &mut State, name: &str) -> Result<()> {
    state.busy = Some(format!("Loading logs of {name}"));
    draw_now(terminal, state)?;

    let lines = commands::tail_logs(&state.docker, name, LOG_LINES as u64).await?;
    state.logs = Logs {
        app: name.to_string(),
        lines,
        scroll: 0,
    };
    state.busy = None;
    state.screen = Screen::Logs;
    Ok(())
}

fn backup_path(name: &str) -> std::path::PathBuf {
    let seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("{name}-{seconds}.tar.gz").into()
}

fn draw_now(terminal: &mut DefaultTerminal, state: &State) -> Result<()> {
    terminal.draw(|frame| draw(frame, state))?;
    Ok(())
}

fn draw(frame: &mut Frame, state: &State) {
    let areas = Layout::vertical([
        Constraint::Length(3),
        Constraint::Min(0),
        Constraint::Length(3),
    ])
    .split(frame.area());

    header(frame, areas[0], state);

    match state.screen {
        Screen::Apps => apps(frame, areas[1], state),
        Screen::Catalog => catalog(frame, areas[1], state),
        Screen::Logs => logs(frame, areas[1], state),
        Screen::Help => help(frame, areas[1]),
    }

    footer(frame, areas[2], state);
}

fn header(frame: &mut Frame, area: Rect, state: &State) {
    let running = state.apps.iter().filter(|app| app.running).count();
    let root = state
        .apps
        .iter()
        .filter(|app| app.settings.runs_as_root())
        .count();

    let line = Line::from(vec![
        Span::styled(" erst ", Style::default().add_modifier(Modifier::REVERSED)),
        Span::raw(format!(" {} apps  ({running} running, ", state.apps.len())),
        if root > 0 {
            Span::styled(
                format!("{root} as root"),
                Style::default().fg(Color::Yellow),
            )
        } else {
            Span::raw("none as root".to_string())
        },
        Span::raw(")"),
    ]);

    frame.render_widget(
        Paragraph::new(line).block(Block::default().borders(Borders::ALL)),
        area,
    );
}

fn apps(frame: &mut Frame, area: Rect, state: &State) {
    let title = if state.root_only {
        "apps running as root"
    } else {
        "apps"
    };

    if state.apps.is_empty() {
        let message = if state.root_only {
            "No apps running as root.\nPress R to show every app."
        } else {
            "No apps installed.\nPress r to install one from the catalog."
        };
        frame.render_widget(
            Paragraph::new(message)
                .wrap(Wrap { trim: true })
                .block(Block::default().borders(Borders::ALL).title(title)),
            area,
        );
        return;
    }

    let items: Vec<ListItem> = state
        .apps
        .iter()
        .map(|app| {
            let status = if app.running { "running" } else { "stopped" };
            let ports = app
                .settings
                .ports
                .iter()
                .map(|port| port.to_string())
                .collect::<Vec<_>>()
                .join(", ");
            let user = if app.settings.runs_as_root() {
                Span::styled("root!", Style::default().fg(Color::Yellow))
            } else {
                Span::raw(app.settings.user.trim().to_string())
            };

            ListItem::new(Line::from(vec![
                Span::styled(
                    format!(" {:<18}", app.settings.name),
                    Style::default().add_modifier(Modifier::BOLD),
                ),
                Span::raw(format!("{status:<9} ")),
                user,
                Span::raw(format!("  {ports}")),
            ]))
        })
        .collect();

    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .title(title.to_string()),
    );
    let mut list_state = ListState::default();
    list_state.select(Some(state.selected));
    frame.render_stateful_widget(list, area, &mut list_state);
}

fn catalog(frame: &mut Frame, area: Rect, state: &State) {
    let items: Vec<ListItem> = catalog::entries()
        .iter()
        .map(|entry| {
            ListItem::new(Line::from(vec![
                Span::styled(
                    format!(" {:<18}", entry.name),
                    Style::default().add_modifier(Modifier::BOLD),
                ),
                Span::raw(format!(" {:<28}", entry.image)),
                Span::raw(entry.description),
            ]))
        })
        .collect();

    let list = List::new(items).block(
        Block::default()
            .borders(Borders::ALL)
            .title("catalog  (enter install · esc back)"),
    );
    let mut list_state = ListState::default();
    list_state.select(Some(state.catalog_selected));
    frame.render_stateful_widget(list, area, &mut list_state);
}

fn logs(frame: &mut Frame, area: Rect, state: &State) {
    if state.logs.lines.is_empty() {
        frame.render_widget(
            Paragraph::new("No logs yet.").block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(state.logs.app.as_str()),
            ),
            area,
        );
        return;
    }

    let height = area.height.saturating_sub(2) as usize;
    let last = state.logs.lines.len();
    let start = last.saturating_sub(height + state.logs.scroll as usize);

    let lines: Vec<Line> = state.logs.lines[start..last]
        .iter()
        .map(|line| Line::from(line.clone()))
        .collect();

    frame.render_widget(
        Paragraph::new(lines).block(
            Block::default()
                .borders(Borders::ALL)
                .title(format!("{}  (j/k scroll · esc back)", state.logs.app)),
        ),
        area,
    );
}

fn help(frame: &mut Frame, area: Rect) {
    let lines: Vec<Line> = [
        "j / k    move between apps",
        "l        logs of the selected app",
        "s        start",
        "x        stop",
        "u        update to the latest image",
        "b        back up the data",
        "d        back up the data, then remove the app",
        "r        install from the catalog",
        "R        show only apps running as root",
        "q / esc  quit",
    ]
    .into_iter()
    .map(Line::from)
    .collect();

    let area = centered(area, 60, lines.len() as u16 + 2);
    frame.render_widget(Clear, area);
    frame.render_widget(
        Paragraph::new(lines).block(Block::default().borders(Borders::ALL).title("keys")),
        area,
    );
}

fn footer(frame: &mut Frame, area: Rect, state: &State) {
    let text = match (&state.busy, &state.notice) {
        (Some(busy), _) => busy.clone(),
        (None, Some(notice)) => notice.clone(),
        (None, None) => match state.screen {
            Screen::Apps => {
                "j/k move · l logs · s start · x stop · u update · b backup · r install · q quit"
            }
            Screen::Catalog => "j/k move · enter install · esc back",
            Screen::Logs => "j/k scroll · esc back",
            Screen::Help => "any key to go back",
        }
        .to_string(),
    };

    frame.render_widget(
        Paragraph::new(text).block(Block::default().borders(Borders::ALL)),
        area,
    );
}

/// A box centered in `area`, for popups.
fn centered(area: Rect, percent_x: u16, height: u16) -> Rect {
    let height = height.min(area.height);
    let vertical = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(height),
        Constraint::Fill(1),
    ])
    .split(area);

    Layout::horizontal([
        Constraint::Fill(1),
        Constraint::Percentage(percent_x),
        Constraint::Fill(1),
    ])
    .split(vertical[1])[1]
}

fn setup() -> Result<DefaultTerminal> {
    crossterm::terminal::enable_raw_mode()?;
    Ok(ratatui::init())
}

fn restore(terminal: &mut DefaultTerminal) -> Result<()> {
    ratatui::restore();
    terminal.clear()?;
    Ok(())
}
