use std::{
    cmp::min,
    io,
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

#[derive(Debug, Default, Clone)]
struct WorldState {
    placements: Vec<Placement>,
    trusts: Vec<Trust>,
    bases: Vec<Base>,
    units: Vec<Unit>,
    combats: Vec<Combat>,
}

impl WorldState {
    fn bounds(&self) -> Option<Bounds> {
        if !self.placements.is_empty() {
            return bounds_for(self.placements.iter().map(|placement| placement.position));
        }

        bounds_for(self.fallback_points())
    }

    fn fallback_points(&self) -> impl Iterator<Item = Point> + '_ {
        self.trusts
            .iter()
            .map(|trust| trust.position)
            .chain(self.bases.iter().map(|base| base.position))
            .chain(self.units.iter().map(|unit| unit.position))
            .chain(self.units.iter().filter_map(Unit::target_position))
            .chain(self.combats.iter().map(|combat| combat.position))
    }
}

fn bounds_for(mut points: impl Iterator<Item = Point>) -> Option<Bounds> {
    let first = points.next()?;
    let mut bounds = Bounds::from_point(first);
    for point in points {
        bounds.include(point);
    }
    Some(bounds)
}

#[derive(Debug, Clone, Copy)]
struct Bounds {
    min_x: f64,
    max_x: f64,
    min_y: f64,
    max_y: f64,
}

impl Bounds {
    fn from_point(point: Point) -> Self {
        Self {
            min_x: point.x,
            max_x: point.x,
            min_y: point.y,
            max_y: point.y,
        }
    }

    fn include(&mut self, point: Point) {
        self.min_x = self.min_x.min(point.x);
        self.max_x = self.max_x.max(point.x);
        self.min_y = self.min_y.min(point.y);
        self.max_y = self.max_y.max(point.y);
    }
}

struct App {
    client: reqwest::Client,
    base_url: String,
    world: WorldState,
    last_refresh: Option<Instant>,
    last_error: Option<String>,
}

impl App {
    fn new(base_url: String) -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url: base_url.trim_end_matches('/').to_string(),
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
    let base_url = std::env::args().nth(1).unwrap_or_else(|| DEFAULT_BASE_URL.to_string());
    let mut app = App::new(base_url);
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

    frame.render_widget(WorldMap { world: &app.world }, map_area);
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

    if let Some(bounds) = app.world.bounds() {
        lines.extend([
            Line::from(""),
            Line::from(format!("Bounds x: {:.1}..{:.1}", bounds.min_x, bounds.max_x)),
            Line::from(format!("Bounds y: {:.1}..{:.1}", bounds.min_y, bounds.max_y)),
        ]);
    }

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
    world: &'a WorldState,
}

impl Widget for WorldMap<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let block = Block::new().borders(Borders::ALL).title("World");
        let inner = block.inner(area);
        block.render(area, buf);

        if inner.width < 8 || inner.height < 4 {
            return;
        }

        let Some(bounds) = self.world.bounds() else {
            buf.set_string(
                inner.x,
                inner.y,
                "No simulation data yet",
                Style::default().fg(Color::DarkGray),
            );
            return;
        };

        let viewport = Viewport::new(inner, bounds);
        viewport.draw_world_border(buf);

        for unit in &self.world.units {
            if let Some(target) = unit.target_position() {
                viewport.draw_path(buf, unit.position, target);
            }
        }

        for placement in &self.world.placements {
            viewport.draw_point(buf, placement.position, "📍", Color::Green);
        }
        for trust in &self.world.trusts {
            viewport.draw_point(buf, trust.position, "⚙️", Color::Yellow);
        }
        for base in &self.world.bases {
            viewport.draw_point(buf, base.position, "🎪", Color::Magenta);
        }
        for unit in &self.world.units {
            viewport.draw_point(buf, unit.position, "🪖", Color::Blue);
        }
        for combat in &self.world.combats {
            viewport.draw_point(buf, combat.position, "⚔️", Color::Red);
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct Viewport {
    rect: Rect,
    bounds: Bounds,
}

impl Viewport {
    fn new(area: Rect, bounds: Bounds) -> Self {
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

        for (x, y) in line_cells(from_x, from_y, to_x, to_y) {
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
        if point.x < self.bounds.min_x
            || point.x > self.bounds.max_x
            || point.y < self.bounds.min_y
            || point.y > self.bounds.max_y
        {
            return None;
        }

        let width = self.rect.width.saturating_sub(1);
        let height = self.rect.height.saturating_sub(1);
        let x_span = (self.bounds.max_x - self.bounds.min_x).max(f64::EPSILON);
        let y_span = (self.bounds.max_y - self.bounds.min_y).max(f64::EPSILON);
        let x_ratio = (point.x - self.bounds.min_x) / x_span;
        let y_ratio = (point.y - self.bounds.min_y) / y_span;
        let x = self.rect.x + (x_ratio * f64::from(width)).round() as u16;
        let y = self.rect.y + height - (y_ratio * f64::from(height)).round() as u16;

        Some((x, y))
    }
}

fn line_cells(from_x: u16, from_y: u16, to_x: u16, to_y: u16) -> Vec<(u16, u16)> {
    let mut cells = Vec::new();
    let mut x = i32::from(from_x);
    let mut y = i32::from(from_y);
    let to_x = i32::from(to_x);
    let to_y = i32::from(to_y);
    let dx = (to_x - x).abs();
    let dy = -(to_y - y).abs();
    let sx = if x < to_x { 1 } else { -1 };
    let sy = if y < to_y { 1 } else { -1 };
    let mut error = dx + dy;

    loop {
        cells.push((x as u16, y as u16));
        if x == to_x && y == to_y {
            break;
        }
        let doubled_error = 2 * error;
        if doubled_error >= dy {
            error += dy;
            x += sx;
        }
        if doubled_error <= dx {
            error += dx;
            y += sy;
        }
    }

    cells
}
