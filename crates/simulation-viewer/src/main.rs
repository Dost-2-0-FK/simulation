use std::{
    cmp::min,
    fs, io,
    path::Path,
    time::{Duration, Instant},
};

use anyhow::{Context, Result};
use crossterm::{
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Frame, Terminal,
    backend::CrosstermBackend,
    buffer::Buffer,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph, Widget},
};
use serde::Deserialize;
use serde_json::Value;
use tokio::time;

const DEFAULT_BASE_URL: &str = "http://127.0.0.1:8080";
const DEFAULT_CONFIG_PATH: &str = "simulation-viewer.toml";
const POLL_INTERVAL: Duration = Duration::from_secs(1);
const MAP_MARGIN_X: u16 = 3;
const MAP_MARGIN_Y: u16 = 1;

#[derive(Debug, Clone, Copy, Deserialize)]
struct Point {
    x: f64,
    y: f64,
}

#[derive(Debug, Clone, Deserialize)]
struct Placement {
    position: Point,
}

#[derive(Debug, Clone, Deserialize)]
struct Trust {
    position: Point,
}

#[derive(Debug, Clone, Deserialize)]
struct Base {
    position: Point,
}

#[derive(Debug, Clone, Deserialize)]
struct Unit {
    position: Point,
    #[serde(default)]
    target: Value,
}

impl Unit {
    fn target_position(&self) -> Option<Point> {
        serde_json::from_value(self.target.get("position")?.clone()).ok()
    }
}

#[derive(Debug, Clone, Deserialize)]
struct Combat {
    position: Point,
}

#[derive(Debug, Clone, Copy, Deserialize)]
struct WorldBounds {
    min_x: f64,
    max_x: f64,
    min_y: f64,
    max_y: f64,
}

#[derive(Debug, Clone, Deserialize)]
struct ViewerConfig {
    #[serde(default = "default_base_url")]
    base_url: String,
    world: WorldBounds,
}

impl ViewerConfig {
    fn load(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let config = fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        let config: Self = toml::from_str(&config).with_context(|| format!("parsing {}", path.display()))?;
        config
            .world
            .validate()
            .with_context(|| format!("validating {}", path.display()))?;
        Ok(config)
    }
}

fn default_base_url() -> String {
    DEFAULT_BASE_URL.to_string()
}

impl WorldBounds {
    fn validate(self) -> Result<()> {
        if self.min_x >= self.max_x {
            anyhow::bail!("world min_x must be less than max_x");
        }
        if self.min_y >= self.max_y {
            anyhow::bail!("world min_y must be less than max_y");
        }
        Ok(())
    }

    fn width(self) -> f64 {
        self.max_x - self.min_x
    }

    fn height(self) -> f64 {
        self.max_y - self.min_y
    }

    fn wrap(self, point: Point) -> Point {
        Point {
            x: wrap_coordinate(point.x, self.min_x, self.max_x),
            y: wrap_coordinate(point.y, self.min_y, self.max_y),
        }
    }

    fn shortest_delta(self, from: Point, to: Point) -> Point {
        let from = self.wrap(from);
        let to = self.wrap(to);

        Point {
            x: shortest_axis_delta(from.x, to.x, self.width()),
            y: shortest_axis_delta(from.y, to.y, self.height()),
        }
    }
}

fn wrap_coordinate(value: f64, min: f64, max: f64) -> f64 {
    (value - min).rem_euclid(max - min) + min
}

fn shortest_axis_delta(from: f64, to: f64, span: f64) -> f64 {
    let forward = (to - from).rem_euclid(span);
    if forward > span / 2.0 { forward - span } else { forward }
}

#[derive(Debug, Default, Clone)]
struct WorldState {
    placements: Vec<Placement>,
    trusts: Vec<Trust>,
    bases: Vec<Base>,
    units: Vec<Unit>,
    combats: Vec<Combat>,
}

struct App {
    client: reqwest::Client,
    base_url: String,
    bounds: WorldBounds,
    world: WorldState,
    last_refresh: Option<Instant>,
    last_error: Option<String>,
}

impl App {
    fn new(config: ViewerConfig, base_url_override: Option<String>) -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url: base_url_override
                .unwrap_or(config.base_url)
                .trim_end_matches('/')
                .to_string(),
            bounds: config.world,
            world: WorldState::default(),
            last_refresh: None,
            last_error: None,
        }
    }

    async fn refresh(&mut self) {
        match self.fetch_world().await {
            Ok(world) => {
                self.world = world;
                self.last_refresh = Some(Instant::now());
                self.last_error = None;
            }
            Err(error) => {
                self.last_error = Some(format!("{error:#}"));
            }
        }
    }

    async fn fetch_world(&self) -> Result<WorldState> {
        let placements = self.fetch("/api/placements").await?;
        let trusts = self.fetch("/api/trusts").await?;
        let bases = self.fetch("/api/bases").await?;
        let units = self.fetch("/api/units").await?;
        let combats = self.fetch("/api/combats").await?;

        Ok(WorldState {
            placements,
            trusts,
            bases,
            units,
            combats,
        })
    }

    async fn fetch<T>(&self, path: &str) -> Result<T>
    where
        T: for<'de> Deserialize<'de>,
    {
        let url = format!("{}{}", self.base_url, path);
        self.client
            .get(&url)
            .send()
            .await
            .with_context(|| format!("requesting {url}"))?
            .error_for_status()
            .with_context(|| format!("HTTP error from {url}"))?
            .json()
            .await
            .with_context(|| format!("decoding {url}"))
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let config = ViewerConfig::load(DEFAULT_CONFIG_PATH)?;
    let base_url_override = std::env::args().nth(1);
    let mut app = App::new(config, base_url_override);
    app.refresh().await;

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    let result = run(&mut terminal, &mut app).await;

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    result
}

async fn run(terminal: &mut Terminal<CrosstermBackend<io::Stdout>>, app: &mut App) -> Result<()> {
    let mut interval = time::interval(POLL_INTERVAL);

    loop {
        terminal.draw(|frame| draw(frame, app))?;

        tokio::select! {
            _ = interval.tick() => app.refresh().await,
            should_quit = read_input() => {
                if should_quit? {
                    break;
                }
            }
        }
    }

    Ok(())
}

async fn read_input() -> Result<bool> {
    if !event::poll(Duration::from_millis(80))? {
        return Ok(false);
    }

    let Event::Key(key) = event::read()? else {
        return Ok(false);
    };

    if key.kind != KeyEventKind::Press {
        return Ok(false);
    }

    Ok(matches!(key.code, KeyCode::Esc | KeyCode::Char('q')))
}

fn draw(frame: &mut Frame<'_>, app: &App) {
    let [map_area, side_area] = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Min(40), Constraint::Length(32)])
        .areas(frame.area());

    frame.render_widget(WorldMap { app }, map_area);
    frame.render_widget(status_panel(app), side_area);
}

fn status_panel(app: &App) -> Paragraph<'_> {
    let mut lines = vec![
        Line::from(vec![Span::styled(
            "Simulation Viewer",
            Style::default().fg(Color::Cyan),
        )]),
        Line::from(""),
        Line::from(format!("Endpoint: {}", app.base_url)),
        Line::from(format!("Placements: {}", app.world.placements.len())),
        Line::from(format!("Trusts: {}", app.world.trusts.len())),
        Line::from(format!("Bases: {}", app.world.bases.len())),
        Line::from(format!("Units: {}", app.world.units.len())),
        Line::from(format!("Combats: {}", app.world.combats.len())),
        Line::from(""),
        Line::from("📍 placement"),
        Line::from("⚙️ trust"),
        Line::from("🎪 base"),
        Line::from("🪖 unit"),
        Line::from("⚔️ combat"),
        Line::from("ㆍ unit path"),
        Line::from(""),
        Line::from("q / Esc quits"),
    ];

    lines.extend([
        Line::from(""),
        Line::from(format!("World x: {:.1}..{:.1}", app.bounds.min_x, app.bounds.max_x)),
        Line::from(format!("World y: {:.1}..{:.1}", app.bounds.min_y, app.bounds.max_y)),
    ]);

    if let Some(last_refresh) = app.last_refresh {
        lines.push(Line::from(format!(
            "Updated: {}s ago",
            last_refresh.elapsed().as_secs()
        )));
    }

    if let Some(error) = &app.last_error {
        lines.extend([
            Line::from(""),
            Line::from(Span::styled("Last error:", Style::default().fg(Color::Red))),
            Line::from(error.as_str()),
        ]);
    }

    Paragraph::new(lines).block(Block::new().borders(Borders::ALL).title("Status"))
}

struct WorldMap<'a> {
    app: &'a App,
}

impl Widget for WorldMap<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::new().borders(Borders::ALL).title("World");
        let inner = block.inner(area);
        block.render(area, buf);

        if inner.width < 8 || inner.height < 4 {
            return;
        }

        let viewport = Viewport::new(inner, self.app.bounds);
        viewport.draw_world_border(buf);

        for unit in &self.app.world.units {
            if let Some(target) = unit.target_position() {
                viewport.draw_path(buf, unit.position, target);
            }
        }

        for placement in &self.app.world.placements {
            viewport.draw_point(buf, placement.position, "📍", Color::Green);
        }
        for trust in &self.app.world.trusts {
            viewport.draw_point(buf, trust.position, "⚙️", Color::Yellow);
        }
        for base in &self.app.world.bases {
            viewport.draw_point(buf, base.position, "🎪", Color::Magenta);
        }
        for unit in &self.app.world.units {
            viewport.draw_point(buf, unit.position, "🪖", Color::Blue);
        }
        for combat in &self.app.world.combats {
            viewport.draw_point(buf, combat.position, "⚔️", Color::Red);
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct Viewport {
    rect: Rect,
    bounds: WorldBounds,
}

impl Viewport {
    fn new(area: Rect, bounds: WorldBounds) -> Self {
        let margin_x = min(MAP_MARGIN_X, area.width.saturating_sub(3) / 2);
        let margin_y = min(MAP_MARGIN_Y, area.height.saturating_sub(3) / 2);
        let rect = Rect {
            x: area.x + margin_x,
            y: area.y + margin_y,
            width: area.width.saturating_sub(margin_x * 2).max(1),
            height: area.height.saturating_sub(margin_y * 2).max(1),
        };

        Self { rect, bounds }
    }

    fn draw_world_border(self, buf: &mut Buffer) {
        if self.rect.width < 2 || self.rect.height < 2 {
            return;
        }

        let style = Style::default().fg(Color::DarkGray);
        let left = self.rect.x;
        let right = self.rect.x + self.rect.width - 1;
        let top = self.rect.y;
        let bottom = self.rect.y + self.rect.height - 1;

        for x in left..=right {
            buf[(x, top)].set_symbol("─").set_style(style);
            buf[(x, bottom)].set_symbol("─").set_style(style);
        }
        for y in top..=bottom {
            buf[(left, y)].set_symbol("│").set_style(style);
            buf[(right, y)].set_symbol("│").set_style(style);
        }
        buf[(left, top)].set_symbol("┌").set_style(style);
        buf[(right, top)].set_symbol("┐").set_style(style);
        buf[(left, bottom)].set_symbol("└").set_style(style);
        buf[(right, bottom)].set_symbol("┘").set_style(style);
    }

    fn draw_path(self, buf: &mut Buffer, from: Point, to: Point) {
        let Some((from_x, from_y)) = self.to_cell(from) else {
            return;
        };
        let Some((to_x, to_y)) = self.to_cell(to) else {
            return;
        };

        let delta = self.bounds.shortest_delta(from, to);
        let steps = usize::from(self.rect.width.max(self.rect.height)).max(1) * 2;

        for step in 1..steps {
            let scale = step as f64 / steps as f64;
            let point = Point {
                x: from.x + delta.x * scale,
                y: from.y + delta.y * scale,
            };
            let Some((x, y)) = self.to_cell(point) else {
                continue;
            };
            if (x, y) == (from_x, from_y) || (x, y) == (to_x, to_y) {
                continue;
            }
            self.set_cell(buf, x, y, "ㆍ", Color::Gray);
        }
    }

    fn draw_point(self, buf: &mut Buffer, point: Point, symbol: &str, color: Color) {
        let Some((x, y)) = self.to_cell(point) else {
            return;
        };
        self.set_cell(buf, x, y, symbol, color);
    }

    fn set_cell(self, buf: &mut Buffer, x: u16, y: u16, symbol: &str, color: Color) {
        if x >= buf.area().right() || y >= buf.area().bottom() {
            return;
        }
        buf.set_string(x, y, symbol, Style::default().fg(color));
    }

    fn to_cell(self, point: Point) -> Option<(u16, u16)> {
        if self.bounds.width() <= 0.0 || self.bounds.height() <= 0.0 {
            return None;
        }

        let point = self.bounds.wrap(point);
        let width = self.rect.width.saturating_sub(1);
        let height = self.rect.height.saturating_sub(1);
        let x_ratio = (point.x - self.bounds.min_x) / self.bounds.width();
        let y_ratio = (point.y - self.bounds.min_y) / self.bounds.height();
        let x = self.rect.x + (x_ratio * f64::from(width)).round() as u16;
        let y = self.rect.y + height - (y_ratio * f64::from(height)).round() as u16;

        Some((x, y))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bounds() -> WorldBounds {
        WorldBounds {
            min_x: 0.0,
            max_x: 30.0,
            min_y: 0.0,
            max_y: 30.0,
        }
    }

    #[test]
    fn wraps_points_into_half_open_bounds() {
        assert_eq!(bounds().wrap(Point { x: 30.0, y: -1.0 }).x, 0.0);
        assert_eq!(bounds().wrap(Point { x: 30.0, y: -1.0 }).y, 29.0);
    }

    #[test]
    fn shortest_delta_crosses_wrapped_edges() {
        assert_eq!(
            bounds()
                .shortest_delta(Point { x: 29.0, y: 15.0 }, Point { x: 1.0, y: 15.0 })
                .x,
            2.0
        );
        assert_eq!(
            bounds()
                .shortest_delta(Point { x: 15.0, y: 1.0 }, Point { x: 15.0, y: 29.0 })
                .y,
            -2.0
        );
    }
}
