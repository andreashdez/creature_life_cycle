use creature_life_cycle::{
    AphidParams, Board, BoardSummary, CellSnapshot, Coordinates, CreatureSnapshot,
    CreatureSnapshotKind, LadybugParams, Random, load_configured_board, save_simulation_config,
};
use macroquad::prelude::*;
use std::collections::HashMap;

const DEFAULT_SEED: u64 = 42;
const HISTORY_LIMIT: usize = 240;

#[derive(Clone, Copy)]
struct HistoryPoint {
    turn: usize,
    aphids: usize,
    ladybugs: usize,
}

impl HistoryPoint {
    fn new(turn: usize, summary: BoardSummary) -> Self {
        Self {
            turn,
            aphids: summary.aphids,
            ladybugs: summary.ladybugs,
        }
    }
}

#[derive(Clone, Copy)]
struct AnimatedCreature {
    id: usize,
    kind: CreatureSnapshotKind,
    from: Coordinates,
    to: Coordinates,
    born: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum EditTool {
    Aphid,
    Ladybug,
}

struct SaveStatus {
    message: String,
    success: bool,
    ttl: f32,
}

struct SimulationApp {
    board: Board,
    random: Random,
    seed: u64,
    aphid_params: AphidParams,
    ladybug_params: LadybugParams,
    turn: usize,
    summary: BoardSummary,
    last_births: usize,
    last_deaths: usize,
    playing: bool,
    extinct: bool,
    speed: f32,
    accumulator: f32,
    history: Vec<HistoryPoint>,
    animations: Vec<AnimatedCreature>,
    animation_elapsed: f32,
    animation_duration: f32,
    editing_enabled: bool,
    edit_tool: EditTool,
    save_status: Option<SaveStatus>,
}

impl SimulationApp {
    fn new(seed: u64) -> Self {
        let mut random = Random::with_seed(seed);
        let board = load_configured_board(&mut random);
        let summary = board.summary();
        let aphid_params = board.aphid_params();
        let ladybug_params = board.ladybug_params();
        let history = vec![HistoryPoint::new(0, summary)];
        Self {
            board,
            random,
            seed,
            aphid_params,
            ladybug_params,
            turn: 0,
            summary,
            last_births: 0,
            last_deaths: 0,
            playing: true,
            extinct: summary.is_extinct(),
            speed: 1.0,
            accumulator: 0.0,
            history,
            animations: Vec::new(),
            animation_elapsed: 0.0,
            animation_duration: 0.0,
            editing_enabled: false,
            edit_tool: EditTool::Aphid,
            save_status: None,
        }
    }

    fn reset(&mut self) {
        let mut random = Random::with_seed(self.seed);
        let mut board = load_configured_board(&mut random);
        board.set_aphid_params(self.aphid_params);
        board.set_ladybug_params(self.ladybug_params);

        self.summary = board.summary();
        self.board = board;
        self.random = random;
        self.turn = 0;
        self.last_births = 0;
        self.last_deaths = 0;
        self.extinct = self.summary.is_extinct();
        self.accumulator = 0.0;
        self.history.clear();
        self.push_history();
        self.finish_animation();
    }

    fn reload_config_files(&mut self) {
        let speed = self.speed;
        let playing = self.playing;
        *self = Self::new(self.seed);
        self.speed = speed;
        self.playing = playing;
        self.editing_enabled = false;
    }

    fn set_seed(&mut self, seed: u64) {
        self.seed = seed;
        self.reset();
    }

    fn adjust_seed(&mut self, delta: i64) {
        let seed = if delta < 0 {
            self.seed.saturating_sub(delta.unsigned_abs())
        } else {
            self.seed.saturating_add(delta as u64)
        };
        self.set_seed(seed);
    }

    fn apply_params(&mut self) {
        self.board.set_aphid_params(self.aphid_params);
        self.board.set_ladybug_params(self.ladybug_params);
    }

    fn update_save_status(&mut self, frame_time: f32) {
        if let Some(status) = &mut self.save_status {
            status.ttl -= frame_time;
            if status.ttl <= 0.0 {
                self.save_status = None;
            }
        }
    }

    fn set_save_status(&mut self, message: impl Into<String>, success: bool) {
        self.save_status = Some(SaveStatus {
            message: message.into(),
            success,
            ttl: 4.0,
        });
    }

    fn save_config(&mut self) {
        self.apply_params();
        match save_simulation_config(&self.board, "simulation.toml") {
            Ok(()) => self.set_save_status("Saved simulation.toml", true),
            Err(error) => self.set_save_status(format!("Save failed: {error}"), false),
        }
    }

    fn add_edit_creature(&mut self, row: usize, col: usize) {
        let added = match self.edit_tool {
            EditTool::Aphid => self.board.add_aphid(row, col, 10).is_some(),
            EditTool::Ladybug => self
                .board
                .add_ladybug(row, col, 15, &mut self.random)
                .is_some(),
        };

        if added {
            self.after_board_edit();
        }
    }

    fn remove_edit_creature(&mut self, row: usize, col: usize) {
        let removed = match self.edit_tool {
            EditTool::Aphid => self.board.remove_aphid_at(row, col),
            EditTool::Ladybug => self.board.remove_ladybug_at(row, col),
        };

        if removed {
            self.after_board_edit();
        }
    }

    fn after_board_edit(&mut self) {
        self.finish_animation();
        self.summary = self.board.summary();
        self.turn = 0;
        self.last_births = 0;
        self.last_deaths = 0;
        self.extinct = self.summary.is_extinct();
        self.playing = false;
        self.accumulator = 0.0;
        self.history.clear();
        self.push_history();
    }

    fn is_animating(&self) -> bool {
        !self.animations.is_empty()
    }

    fn finish_animation(&mut self) {
        self.animations.clear();
        self.animation_elapsed = 0.0;
        self.animation_duration = 0.0;
    }

    fn animation_progress(&self) -> f32 {
        if self.animation_duration <= 0.0 {
            1.0
        } else {
            (self.animation_elapsed / self.animation_duration).clamp(0.0, 1.0)
        }
    }

    fn push_history(&mut self) {
        if self.history.len() == HISTORY_LIMIT {
            self.history.remove(0);
        }
        self.history
            .push(HistoryPoint::new(self.turn, self.summary));
    }

    fn step(&mut self) {
        if self.extinct {
            return;
        }

        self.finish_animation();
        let before = self.board.creature_snapshots();
        let stats = self.board.refresh(&mut self.random);
        let after = self.board.creature_snapshots();
        self.turn += 1;
        self.summary = stats.summary;
        self.last_births = stats.births;
        self.last_deaths = stats.deaths;
        self.extinct = stats.summary.is_extinct();
        self.push_history();

        let (animations, changed) = build_movement_animations(&before, &after);
        if changed {
            self.animations = animations;
            self.animation_elapsed = 0.0;
            self.animation_duration = (0.42 / self.speed.sqrt()).clamp(0.12, 0.42);
        }
    }
}

fn build_movement_animations(
    before: &[CreatureSnapshot],
    after: &[CreatureSnapshot],
) -> (Vec<AnimatedCreature>, bool) {
    let before_by_id: HashMap<usize, CreatureSnapshot> = before
        .iter()
        .map(|creature| (creature.id, *creature))
        .collect();
    let mut changed = false;
    let animations = after
        .iter()
        .map(|creature| {
            let previous = before_by_id.get(&creature.id);
            let from = previous.map_or(creature.location, |snapshot| snapshot.location);
            let born = previous.is_none();
            changed |= born || from != creature.location;
            AnimatedCreature {
                id: creature.id,
                kind: creature.kind,
                from,
                to: creature.location,
                born,
            }
        })
        .collect();

    (animations, changed)
}

#[derive(Clone, Copy)]
struct BoardLayout {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    cell: f32,
    rows: usize,
    cols: usize,
    panel_x: f32,
    panel_y: f32,
    panel_w: f32,
    panel_h: f32,
    graph_x: f32,
    graph_y: f32,
    graph_w: f32,
    graph_h: f32,
}

fn window_conf() -> Conf {
    Conf {
        window_title: "Aphids and Ladybugs".to_string(),
        window_width: 1120,
        window_height: 860,
        window_resizable: true,
        high_dpi: true,
        sample_count: 4,
        ..Default::default()
    }
}

#[macroquad::main(window_conf)]
async fn main() {
    let mut app = SimulationApp::new(DEFAULT_SEED);

    loop {
        if is_key_pressed(KeyCode::Escape) {
            break;
        }

        handle_input(&mut app);
        tick_simulation(&mut app);

        draw_background();
        draw_title(&app);

        let layout = calculate_layout(&app.board);
        let hovered = hovered_cell(layout);
        handle_board_edit(&mut app, hovered);
        draw_board(&app, layout, hovered);
        draw_history_graph(&app, layout);
        draw_panel(&mut app, layout);
        draw_hover_tooltip(&app, hovered);

        next_frame().await;
    }
}

fn handle_input(app: &mut SimulationApp) {
    if is_key_pressed(KeyCode::Space) {
        app.playing = !app.playing;
        app.accumulator = 0.0;
    }

    if is_key_pressed(KeyCode::N) {
        app.finish_animation();
        app.step();
        app.accumulator = 0.0;
    }

    if is_key_pressed(KeyCode::R) {
        app.reset();
    }

    if is_key_pressed(KeyCode::E) {
        app.editing_enabled = !app.editing_enabled;
        app.playing = false;
        app.finish_animation();
    }

    if is_key_pressed(KeyCode::A) {
        app.edit_tool = EditTool::Aphid;
        app.editing_enabled = true;
        app.playing = false;
    }

    if is_key_pressed(KeyCode::L) {
        app.edit_tool = EditTool::Ladybug;
        app.editing_enabled = true;
        app.playing = false;
    }

    if is_key_pressed(KeyCode::S) {
        app.save_config();
    }

    if is_key_pressed(KeyCode::Up) || is_key_pressed(KeyCode::Equal) {
        app.speed = (app.speed + 0.5).min(8.0);
    }

    if is_key_pressed(KeyCode::Down) || is_key_pressed(KeyCode::Minus) {
        app.speed = (app.speed - 0.5).max(0.5);
    }
}

fn handle_board_edit(app: &mut SimulationApp, hovered: Option<(usize, usize)>) {
    if !app.editing_enabled || app.is_animating() {
        return;
    }

    let Some((row, col)) = hovered else {
        return;
    };

    if is_mouse_button_pressed(MouseButton::Left) {
        app.add_edit_creature(row, col);
    }

    if is_mouse_button_pressed(MouseButton::Right) {
        app.remove_edit_creature(row, col);
    }
}

fn tick_simulation(app: &mut SimulationApp) {
    let frame_time = get_frame_time();
    app.update_save_status(frame_time);

    if app.is_animating() {
        app.animation_elapsed += frame_time;
        if app.animation_elapsed < app.animation_duration {
            return;
        }

        app.finish_animation();
    }

    if !app.playing || app.extinct {
        return;
    }

    app.accumulator += frame_time;
    let step_interval = 1.0 / app.speed;
    let mut steps = 0;

    while app.accumulator >= step_interval && steps < 8 && !app.extinct && !app.is_animating() {
        app.step();
        app.accumulator -= step_interval;
        steps += 1;
    }
}

fn calculate_layout(board: &Board) -> BoardLayout {
    let rows = board.rows().max(1);
    let cols = board.cols().max(1);
    let screen_w = screen_width();
    let screen_h = screen_height();
    let wide = screen_w >= 900.0;
    let margin = if wide { 32.0 } else { 18.0 };
    let top = if wide { 96.0 } else { 112.0 };
    let panel_w = if wide { 360.0 } else { screen_w - margin * 2.0 };
    let graph_h = if wide { 118.0 } else { 102.0 };
    let graph_gap = if wide { 22.0 } else { 14.0 };
    let panel_h = if wide {
        screen_h - top - margin
    } else {
        (screen_h * 0.42).max(260.0)
    };
    let board_area_w = if wide {
        screen_w - panel_w - margin * 3.0
    } else {
        screen_w - margin * 2.0
    };
    let board_area_h = if wide {
        screen_h - top - margin - graph_h - graph_gap
    } else {
        screen_h - top - panel_h - graph_h - graph_gap - margin * 3.0
    }
    .max(180.0);
    let cell = (board_area_w / cols as f32)
        .min(board_area_h / rows as f32)
        .floor()
        .max(16.0);
    let width = cell * cols as f32;
    let height = cell * rows as f32;
    let x = if wide {
        margin + (board_area_w - width).max(0.0) * 0.5
    } else {
        (screen_w - width) * 0.5
    };
    let y = top + (board_area_h - height).max(0.0) * 0.5;
    let graph_x = margin;
    let graph_y = y + height + graph_gap;
    let graph_w = board_area_w;
    let panel_x = if wide {
        screen_w - panel_w - margin
    } else {
        margin
    };
    let panel_y = if wide {
        top
    } else {
        graph_y + graph_h + margin
    };

    BoardLayout {
        x,
        y,
        width,
        height,
        cell,
        rows,
        cols,
        panel_x,
        panel_y,
        panel_w,
        panel_h,
        graph_x,
        graph_y,
        graph_w,
        graph_h,
    }
}

fn hovered_cell(layout: BoardLayout) -> Option<(usize, usize)> {
    let (mouse_x, mouse_y) = mouse_position();
    if mouse_x < layout.x
        || mouse_y < layout.y
        || mouse_x >= layout.x + layout.width
        || mouse_y >= layout.y + layout.height
    {
        return None;
    }

    let row = ((mouse_y - layout.y) / layout.cell) as usize;
    let col = ((mouse_x - layout.x) / layout.cell) as usize;
    (row < layout.rows && col < layout.cols).then_some((row, col))
}

fn draw_background() {
    let top = color(11, 17, 27, 255);
    let bottom = color(27, 35, 42, 255);
    let bands = 36;
    let band_h = screen_height() / bands as f32;

    for band in 0..bands {
        let t = band as f32 / (bands - 1) as f32;
        draw_rectangle(
            0.0,
            band as f32 * band_h,
            screen_width(),
            band_h + 1.0,
            mix(top, bottom, t),
        );
    }

    draw_circle(
        screen_width() * 0.12,
        screen_height() * 0.12,
        260.0,
        color(61, 111, 78, 35),
    );
    draw_circle(
        screen_width() * 0.88,
        screen_height() * 0.78,
        300.0,
        color(131, 59, 48, 28),
    );
}

fn draw_title(app: &SimulationApp) {
    draw_text_ex(
        "Aphids & Ladybugs",
        32.0,
        44.0,
        TextParams {
            font_size: 34,
            color: color(238, 232, 210, 255),
            ..Default::default()
        },
    );
    draw_text_ex(
        &format!(
            "Seed {}  |  Turn {}  |  {}",
            app.seed,
            app.turn,
            if app.extinct {
                "extinct"
            } else if app.playing {
                "running"
            } else {
                "paused"
            }
        ),
        34.0,
        70.0,
        TextParams {
            font_size: 18,
            color: color(162, 176, 163, 255),
            ..Default::default()
        },
    );
}

fn draw_board(app: &SimulationApp, layout: BoardLayout, hovered: Option<(usize, usize)>) {
    draw_round_rect(
        layout.x - 14.0,
        layout.y - 14.0,
        layout.width + 28.0,
        layout.height + 28.0,
        24.0,
        color(0, 0, 0, 78),
    );
    draw_round_rect(
        layout.x - 8.0,
        layout.y - 8.0,
        layout.width + 16.0,
        layout.height + 16.0,
        20.0,
        color(31, 42, 38, 225),
    );

    let gap = (layout.cell * 0.08).clamp(2.0, 7.0);
    let inner = layout.cell - gap;

    for row in 0..layout.rows {
        for col in 0..layout.cols {
            let Some(snapshot) = app.board.cell_snapshot(row, col) else {
                continue;
            };
            let x = layout.x + col as f32 * layout.cell + gap * 0.5;
            let y = layout.y + row as f32 * layout.cell + gap * 0.5;
            if app.is_animating() {
                draw_cell_background(x, y, inner, row, col, snapshot, hovered == Some((row, col)));
            } else {
                draw_cell(x, y, inner, row, col, snapshot, hovered == Some((row, col)));
            }
        }
    }

    if app.is_animating() {
        draw_animated_creatures(app, layout);
    }

    if app.editing_enabled {
        draw_edit_overlay(app, layout, hovered);
    }
}

fn draw_history_graph(app: &SimulationApp, layout: BoardLayout) {
    let rect = Rect::new(
        layout.graph_x,
        layout.graph_y,
        layout.graph_w,
        layout.graph_h,
    );
    draw_round_rect(rect.x, rect.y, rect.w, rect.h, 18.0, color(15, 22, 25, 218));
    draw_rectangle_lines(
        rect.x + 1.0,
        rect.y + 1.0,
        rect.w - 2.0,
        rect.h - 2.0,
        1.0,
        color(112, 131, 117, 96),
    );

    draw_text_ex(
        "Population History",
        rect.x + 18.0,
        rect.y + 25.0,
        TextParams {
            font_size: 18,
            color: color(238, 232, 210, 255),
            ..Default::default()
        },
    );

    let Some(first) = app.history.first() else {
        return;
    };
    let Some(last) = app.history.last() else {
        return;
    };
    let max_population = app
        .history
        .iter()
        .map(|point| point.aphids.max(point.ladybugs))
        .max()
        .unwrap_or(1)
        .max(1) as f32;
    let plot = Rect::new(rect.x + 18.0, rect.y + 39.0, rect.w - 36.0, rect.h - 60.0);

    draw_history_grid(plot, max_population);
    draw_history_series(
        &app.history,
        plot,
        max_population,
        |point| point.aphids,
        color(111, 219, 91, 255),
    );
    draw_history_series(
        &app.history,
        plot,
        max_population,
        |point| point.ladybugs,
        color(231, 78, 61, 255),
    );

    draw_history_legend(rect, app, first.turn, last.turn, max_population as usize);
}

fn draw_history_grid(plot: Rect, max_population: f32) {
    for step in 0..=3 {
        let t = step as f32 / 3.0;
        let y = plot.y + plot.h * t;
        draw_line(
            plot.x,
            y,
            plot.x + plot.w,
            y,
            1.0,
            color(255, 255, 255, if step == 3 { 68 } else { 28 }),
        );
    }

    draw_text_ex(
        &format!("{}", max_population as usize),
        plot.x + plot.w - 24.0,
        plot.y - 5.0,
        TextParams {
            font_size: 12,
            color: color(144, 158, 150, 255),
            ..Default::default()
        },
    );
    draw_text_ex(
        "0",
        plot.x + plot.w - 10.0,
        plot.y + plot.h + 13.0,
        TextParams {
            font_size: 12,
            color: color(144, 158, 150, 255),
            ..Default::default()
        },
    );
}

fn draw_history_series(
    history: &[HistoryPoint],
    plot: Rect,
    max_population: f32,
    value: fn(&HistoryPoint) -> usize,
    series_color: Color,
) {
    if history.is_empty() {
        return;
    }

    if history.len() == 1 {
        let y = history_y(value(&history[0]), plot, max_population);
        draw_circle(plot.x, y, 3.0, series_color);
        return;
    }

    for index in 1..history.len() {
        let previous = &history[index - 1];
        let current = &history[index];
        draw_line(
            history_x(index - 1, history.len(), plot),
            history_y(value(previous), plot, max_population),
            history_x(index, history.len(), plot),
            history_y(value(current), plot, max_population),
            2.2,
            series_color,
        );
    }

    let latest = history.last().expect("history is not empty");
    draw_circle(
        history_x(history.len() - 1, history.len(), plot),
        history_y(value(latest), plot, max_population),
        3.2,
        series_color,
    );
}

fn history_x(index: usize, length: usize, plot: Rect) -> f32 {
    if length <= 1 {
        plot.x
    } else {
        plot.x + plot.w * index as f32 / (length - 1) as f32
    }
}

fn history_y(value: usize, plot: Rect, max_population: f32) -> f32 {
    plot.y + plot.h - plot.h * value as f32 / max_population
}

fn draw_history_legend(
    rect: Rect,
    app: &SimulationApp,
    first_turn: usize,
    last_turn: usize,
    max_population: usize,
) {
    let y = rect.y + rect.h - 14.0;
    draw_legend_item(
        rect.x + 18.0,
        y,
        color(111, 219, 91, 255),
        &format!("Aphids {}", app.summary.aphids),
    );
    draw_legend_item(
        rect.x + 122.0,
        y,
        color(231, 78, 61, 255),
        &format!("Ladybugs {}", app.summary.ladybugs),
    );
    draw_text_ex(
        &format!("Turns {first_turn}-{last_turn} | max {max_population}"),
        rect.x + rect.w - 190.0,
        y + 4.0,
        TextParams {
            font_size: 12,
            color: color(144, 158, 150, 255),
            ..Default::default()
        },
    );
}

fn draw_legend_item(x: f32, y: f32, item_color: Color, label: &str) {
    draw_circle(x, y, 4.0, item_color);
    draw_text_ex(
        label,
        x + 10.0,
        y + 4.0,
        TextParams {
            font_size: 12,
            color: color(211, 219, 198, 255),
            ..Default::default()
        },
    );
}

fn draw_cell(
    x: f32,
    y: f32,
    size: f32,
    row: usize,
    col: usize,
    snapshot: CellSnapshot,
    hovered: bool,
) {
    draw_cell_background(x, y, size, row, col, snapshot, hovered);
    draw_creatures(x, y, size, snapshot.aphids, snapshot.ladybugs);
}

fn draw_cell_background(
    x: f32,
    y: f32,
    size: f32,
    row: usize,
    col: usize,
    snapshot: CellSnapshot,
    hovered: bool,
) {
    let food = ((snapshot.food + 2) as f32 / 12.0).clamp(0.0, 1.0);
    let base = mix(color(31, 33, 34, 255), color(76, 104, 58, 255), food);
    let rim = mix(color(48, 53, 50, 255), color(135, 153, 89, 255), food);

    draw_round_rect(x, y, size, size, size * 0.12, base);
    draw_rectangle_lines(x + 1.0, y + 1.0, size - 2.0, size - 2.0, 1.0, rim);

    if snapshot.food > 0 {
        let bar_w = size * (snapshot.food.min(9) as f32 / 9.0).clamp(0.0, 1.0);
        draw_round_rect(
            x + size * 0.14,
            y + size * 0.82,
            bar_w * 0.72,
            (size * 0.055).max(2.0),
            4.0,
            color(194, 184, 83, 130),
        );
    }

    draw_food_speckles(x, y, size, row, col, snapshot.food);

    if hovered {
        draw_rectangle_lines(
            x - 1.5,
            y - 1.5,
            size + 3.0,
            size + 3.0,
            3.0,
            color(245, 228, 168, 230),
        );
    }
}

fn draw_animated_creatures(app: &SimulationApp, layout: BoardLayout) {
    let progress = smooth_progress(app.animation_progress());
    let base_radius = (layout.cell * 0.125).clamp(4.0, 12.0);

    for creature in &app.animations {
        let from = animated_creature_position(layout, creature.from, creature.id, creature.kind);
        let to = animated_creature_position(layout, creature.to, creature.id, creature.kind);
        let moving = creature.from != creature.to;
        let position = if creature.born {
            to
        } else {
            from + (to - from) * progress
        };
        let scale = if creature.born {
            progress
        } else if moving {
            1.0 + (std::f32::consts::PI * progress).sin() * 0.08
        } else {
            1.0
        };

        if moving {
            draw_line(
                from.x,
                from.y,
                position.x,
                position.y,
                1.1,
                color(245, 229, 177, 80),
            );
        }

        match creature.kind {
            CreatureSnapshotKind::Aphid => draw_aphid(
                position.x,
                position.y,
                (base_radius * scale).max(1.0),
                creature.id,
            ),
            CreatureSnapshotKind::Ladybug => draw_ladybug(
                position.x,
                position.y,
                (base_radius * scale).max(1.0),
                creature.id,
            ),
        }
    }
}

fn animated_creature_position(
    layout: BoardLayout,
    location: Coordinates,
    id: usize,
    kind: CreatureSnapshotKind,
) -> Vec2 {
    let gap = (layout.cell * 0.08).clamp(2.0, 7.0);
    let inner = layout.cell - gap;
    let origin_x = layout.x + location.y as f32 * layout.cell + gap * 0.5;
    let origin_y = layout.y + location.x as f32 * layout.cell + gap * 0.5;
    let offset = animated_creature_offset(id, kind);

    vec2(origin_x + inner * offset.x, origin_y + inner * offset.y)
}

fn animated_creature_offset(id: usize, kind: CreatureSnapshotKind) -> Vec2 {
    let kind_offset = match kind {
        CreatureSnapshotKind::Aphid => 17,
        CreatureSnapshotKind::Ladybug => 53,
    };
    let hash = id.wrapping_mul(97).wrapping_add(kind_offset);
    let dx = (hash % 19) as f32 / 18.0 - 0.5;
    let dy = ((hash / 19) % 19) as f32 / 18.0 - 0.5;

    vec2(0.5 + dx * 0.42, 0.5 + dy * 0.42)
}

fn smooth_progress(progress: f32) -> f32 {
    let t = progress.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn draw_edit_overlay(app: &SimulationApp, layout: BoardLayout, hovered: Option<(usize, usize)>) {
    let Some((row, col)) = hovered else {
        return;
    };

    let gap = (layout.cell * 0.08).clamp(2.0, 7.0);
    let inner = layout.cell - gap;
    let x = layout.x + col as f32 * layout.cell + gap * 0.5;
    let y = layout.y + row as f32 * layout.cell + gap * 0.5;
    let preview_color = match app.edit_tool {
        EditTool::Aphid => color(111, 219, 91, 175),
        EditTool::Ladybug => color(231, 78, 61, 175),
    };

    draw_rectangle_lines(
        x - 3.0,
        y - 3.0,
        inner + 6.0,
        inner + 6.0,
        3.0,
        preview_color,
    );
    draw_circle_lines(
        x + inner * 0.5,
        y + inner * 0.5,
        inner * 0.22,
        2.0,
        preview_color,
    );
}

fn draw_food_speckles(x: f32, y: f32, size: f32, row: usize, col: usize, food: i32) {
    let dots = food.clamp(0, 6) as usize;
    for dot in 0..dots {
        let seed = row * 73 + col * 41 + dot * 19;
        let px = x + size * (0.18 + (seed % 59) as f32 / 92.0);
        let py = y + size * (0.18 + (seed % 43) as f32 / 78.0);
        draw_circle(
            px,
            py,
            (size * 0.018).clamp(1.0, 2.6),
            color(219, 205, 116, 95),
        );
    }
}

fn draw_creatures(x: f32, y: f32, size: f32, aphids: usize, ladybugs: usize) {
    let radius = (size * 0.125).clamp(4.0, 12.0);
    let aphid_slots = creature_slots(aphids, ladybugs, true);
    let ladybug_slots = creature_slots(ladybugs, aphids, false);

    for (index, &(dx, dy)) in aphid_slots.iter().enumerate().take(aphids.min(4)) {
        let scale = if aphids + ladybugs > 5 { 0.82 } else { 1.0 };
        draw_aphid(x + size * dx, y + size * dy, radius * scale, index);
    }

    for (index, &(dx, dy)) in ladybug_slots.iter().enumerate().take(ladybugs.min(4)) {
        let scale = if aphids + ladybugs > 5 { 0.82 } else { 1.0 };
        draw_ladybug(x + size * dx, y + size * dy, radius * scale, index);
    }

    if aphids > 1 {
        draw_count_badge(
            x + size * 0.22,
            y + size * 0.22,
            aphids,
            color(93, 188, 85, 255),
        );
    }

    if ladybugs > 1 {
        draw_count_badge(
            x + size * 0.78,
            y + size * 0.78,
            ladybugs,
            color(217, 66, 52, 255),
        );
    }
}

fn creature_slots(primary: usize, secondary: usize, aphid: bool) -> &'static [(f32, f32); 5] {
    const CENTER: [(f32, f32); 5] = [
        (0.50, 0.50),
        (0.36, 0.38),
        (0.64, 0.38),
        (0.38, 0.64),
        (0.64, 0.64),
    ];
    const APHIDS: [(f32, f32); 5] = [
        (0.34, 0.36),
        (0.48, 0.28),
        (0.25, 0.54),
        (0.50, 0.53),
        (0.36, 0.44),
    ];
    const LADYBUGS: [(f32, f32); 5] = [
        (0.68, 0.66),
        (0.54, 0.74),
        (0.76, 0.50),
        (0.57, 0.53),
        (0.68, 0.58),
    ];

    if primary == 0 || secondary == 0 {
        &CENTER
    } else if aphid {
        &APHIDS
    } else {
        &LADYBUGS
    }
}

fn draw_aphid(cx: f32, cy: f32, radius: f32, index: usize) {
    let tilt = if index.is_multiple_of(2) { -0.8 } else { 0.8 };
    let body = color(105, 211, 83, 255);
    let shell = color(66, 155, 62, 255);
    let dark = color(27, 58, 35, 230);

    draw_circle(cx, cy, radius, color(12, 20, 13, 90));
    draw_circle(cx, cy, radius * 0.86, body);
    draw_circle(cx - radius * 0.38, cy - radius * 0.22, radius * 0.42, shell);
    draw_circle(cx + radius * 0.52, cy + radius * 0.12, radius * 0.28, shell);

    for leg in [-0.5_f32, 0.0, 0.5] {
        draw_line(
            cx - radius * 0.12,
            cy + radius * leg,
            cx - radius * (0.92 + leg.abs() * 0.18),
            cy + radius * (leg + 0.22 * tilt),
            1.3,
            dark,
        );
        draw_line(
            cx + radius * 0.12,
            cy + radius * leg,
            cx + radius * (0.92 + leg.abs() * 0.18),
            cy + radius * (leg - 0.22 * tilt),
            1.3,
            dark,
        );
    }

    draw_circle(cx + radius * 0.46, cy - radius * 0.05, radius * 0.09, dark);
}

fn draw_ladybug(cx: f32, cy: f32, radius: f32, index: usize) {
    let shift = if index.is_multiple_of(2) { -0.08 } else { 0.08 };
    let red = color(221, 64, 50, 255);
    let red_hi = color(246, 99, 72, 255);
    let dark = color(26, 23, 24, 255);

    draw_circle(cx, cy, radius, color(12, 12, 12, 105));
    draw_circle(cx, cy, radius * 0.9, red);
    draw_circle(
        cx - radius * 0.18,
        cy - radius * 0.24,
        radius * 0.42,
        red_hi,
    );
    draw_circle(cx, cy - radius * 0.7, radius * 0.38, dark);
    draw_line(cx, cy - radius * 0.72, cx, cy + radius * 0.72, 1.5, dark);

    for (dx, dy) in [(-0.35, -0.12), (0.34, -0.04), (-0.22, 0.34), (0.28, 0.32)] {
        draw_circle(
            cx + radius * (dx + shift),
            cy + radius * dy,
            radius * 0.13,
            dark,
        );
    }
}

fn draw_count_badge(cx: f32, cy: f32, count: usize, accent: Color) {
    let label = count.to_string();
    let radius = 11.0;
    draw_circle(cx + 1.0, cy + 1.0, radius + 1.0, color(0, 0, 0, 115));
    draw_circle(cx, cy, radius, accent);

    let font_size = 16;
    let dimensions = measure_text(&label, None, font_size, 1.0);
    draw_text_ex(
        &label,
        cx - dimensions.width * 0.5,
        cy + dimensions.height * 0.36,
        TextParams {
            font_size,
            color: color(18, 25, 20, 255),
            ..Default::default()
        },
    );
}

fn draw_panel(app: &mut SimulationApp, layout: BoardLayout) {
    draw_round_rect(
        layout.panel_x,
        layout.panel_y,
        layout.panel_w,
        layout.panel_h,
        22.0,
        color(18, 24, 29, 218),
    );
    draw_rectangle_lines(
        layout.panel_x + 1.0,
        layout.panel_y + 1.0,
        layout.panel_w - 2.0,
        layout.panel_h - 2.0,
        1.0,
        color(116, 137, 119, 115),
    );

    let content_x = layout.panel_x + 24.0;
    let content_w = layout.panel_w - 48.0;
    let mut y = layout.panel_y + 34.0;
    draw_text_ex(
        "Simulation",
        content_x,
        y,
        TextParams {
            font_size: 24,
            color: color(238, 232, 210, 255),
            ..Default::default()
        },
    );
    y += 28.0;

    draw_stat(layout.panel_x, y, "Status", status_label(app));
    y += 19.0;
    draw_stat(layout.panel_x, y, "Turn", &app.turn.to_string());
    y += 19.0;
    draw_stat(layout.panel_x, y, "Aphids", &app.summary.aphids.to_string());
    y += 19.0;
    draw_stat(
        layout.panel_x,
        y,
        "Ladybugs",
        &app.summary.ladybugs.to_string(),
    );
    y += 19.0;
    draw_stat(layout.panel_x, y, "Food", &app.summary.food.to_string());
    y += 19.0;
    draw_stat(
        layout.panel_x,
        y,
        "Births/Deaths",
        &format!("{} / {}", app.last_births, app.last_deaths),
    );
    y += 19.0;
    draw_stat(
        layout.panel_x,
        y,
        "Speed",
        &format!("{:.1} turns/s", app.speed),
    );
    y += 26.0;

    draw_section_title(content_x, y, "Run");
    y += 20.0;

    let gap = 8.0;
    let button_h = 26.0;
    let third_w = (content_w - gap * 2.0) / 3.0;
    let play_label = if app.playing { "Pause" } else { "Play" };
    if draw_control_button(
        Rect::new(content_x, y, third_w, button_h),
        play_label,
        color(82, 116, 80, 255),
    ) {
        app.playing = !app.playing;
        app.accumulator = 0.0;
    }
    if draw_control_button(
        Rect::new(content_x + third_w + gap, y, third_w, button_h),
        "Step",
        color(78, 96, 121, 255),
    ) {
        app.step();
        app.accumulator = 0.0;
    }
    if draw_control_button(
        Rect::new(content_x + (third_w + gap) * 2.0, y, third_w, button_h),
        "Reset",
        color(127, 86, 61, 255),
    ) {
        app.reset();
    }
    y += 30.0;

    let half_w = (content_w - gap) / 2.0;
    if draw_control_button(
        Rect::new(content_x, y, half_w, button_h),
        "Slower",
        color(62, 76, 92, 255),
    ) {
        app.speed = (app.speed - 0.5).max(0.5);
    }
    if draw_control_button(
        Rect::new(content_x + half_w + gap, y, half_w, button_h),
        "Faster",
        color(62, 76, 92, 255),
    ) {
        app.speed = (app.speed + 0.5).min(8.0);
    }
    y += 32.0;

    draw_section_title(content_x, y, "Seed");
    y += 20.0;
    let seed_button_w = 44.0;
    let seed_display_w = content_w - seed_button_w * 4.0 - gap * 4.0;
    if draw_control_button(
        Rect::new(content_x, y, seed_button_w, button_h),
        "-10",
        color(74, 83, 93, 255),
    ) {
        app.adjust_seed(-10);
    }
    if draw_control_button(
        Rect::new(content_x + seed_button_w + gap, y, seed_button_w, button_h),
        "-1",
        color(74, 83, 93, 255),
    ) {
        app.adjust_seed(-1);
    }
    draw_round_rect(
        content_x + (seed_button_w + gap) * 2.0,
        y,
        seed_display_w,
        button_h,
        8.0,
        color(32, 42, 45, 230),
    );
    draw_centered_text(
        &app.seed.to_string(),
        Rect::new(
            content_x + (seed_button_w + gap) * 2.0,
            y,
            seed_display_w,
            button_h,
        ),
        16,
        color(232, 223, 188, 255),
    );
    if draw_control_button(
        Rect::new(
            content_x + (seed_button_w + gap) * 2.0 + seed_display_w + gap,
            y,
            seed_button_w,
            button_h,
        ),
        "+1",
        color(74, 83, 93, 255),
    ) {
        app.adjust_seed(1);
    }
    if draw_control_button(
        Rect::new(
            content_x + (seed_button_w + gap) * 3.0 + seed_display_w + gap,
            y,
            seed_button_w,
            button_h,
        ),
        "+10",
        color(74, 83, 93, 255),
    ) {
        app.adjust_seed(10);
    }
    y += 32.0;

    draw_section_title(content_x, y, "Board Edit");
    y += 20.0;
    let edit_button_w = (content_w - gap * 2.0) / 3.0;
    let edit_label = if app.editing_enabled {
        "Editing"
    } else {
        "Edit Off"
    };
    if draw_control_button(
        Rect::new(content_x, y, edit_button_w, button_h),
        edit_label,
        if app.editing_enabled {
            color(82, 116, 80, 255)
        } else {
            color(74, 83, 93, 255)
        },
    ) {
        app.editing_enabled = !app.editing_enabled;
        app.playing = false;
        app.finish_animation();
    }
    if draw_control_button(
        Rect::new(content_x + edit_button_w + gap, y, edit_button_w, button_h),
        "Aphid",
        if app.edit_tool == EditTool::Aphid {
            color(85, 145, 73, 255)
        } else {
            color(57, 76, 59, 255)
        },
    ) {
        app.edit_tool = EditTool::Aphid;
        app.editing_enabled = true;
        app.playing = false;
    }
    if draw_control_button(
        Rect::new(
            content_x + (edit_button_w + gap) * 2.0,
            y,
            edit_button_w,
            button_h,
        ),
        "Ladybug",
        if app.edit_tool == EditTool::Ladybug {
            color(145, 73, 62, 255)
        } else {
            color(76, 59, 57, 255)
        },
    ) {
        app.edit_tool = EditTool::Ladybug;
        app.editing_enabled = true;
        app.playing = false;
    }
    y += 35.0;
    draw_text_ex(
        "Left click adds, right click removes.",
        content_x,
        y,
        TextParams {
            font_size: 13,
            color: color(154, 170, 158, 255),
            ..Default::default()
        },
    );
    y += 22.0;

    draw_section_title(content_x, y, "Config Probabilities");
    y += 22.0;
    let mut params_changed = false;
    params_changed |= draw_probability_slider(
        content_x,
        y,
        content_w,
        "Aphid move",
        &mut app.aphid_params.prob_move,
        color(98, 204, 83, 255),
    );
    y += 24.0;
    params_changed |= draw_probability_slider(
        content_x,
        y,
        content_w,
        "Aphid kill",
        &mut app.aphid_params.prob_kill,
        color(98, 204, 83, 255),
    );
    y += 24.0;
    params_changed |= draw_probability_slider(
        content_x,
        y,
        content_w,
        "Aphid help",
        &mut app.aphid_params.prob_accomplice,
        color(98, 204, 83, 255),
    );
    y += 24.0;
    params_changed |= draw_probability_slider(
        content_x,
        y,
        content_w,
        "Aphid breed",
        &mut app.aphid_params.prob_procreate,
        color(98, 204, 83, 255),
    );
    y += 26.0;
    params_changed |= draw_probability_slider(
        content_x,
        y,
        content_w,
        "Lady move",
        &mut app.ladybug_params.prob_move,
        color(221, 73, 58, 255),
    );
    y += 24.0;
    params_changed |= draw_probability_slider(
        content_x,
        y,
        content_w,
        "Lady kill",
        &mut app.ladybug_params.prob_kill,
        color(221, 73, 58, 255),
    );
    y += 24.0;
    params_changed |= draw_probability_slider(
        content_x,
        y,
        content_w,
        "Lady turn",
        &mut app.ladybug_params.prob_direction,
        color(221, 73, 58, 255),
    );
    y += 24.0;
    params_changed |= draw_probability_slider(
        content_x,
        y,
        content_w,
        "Lady breed",
        &mut app.ladybug_params.prob_procreate,
        color(221, 73, 58, 255),
    );
    y += 29.0;

    if params_changed {
        app.apply_params();
    }

    let half_w = (content_w - gap) / 2.0;
    if draw_control_button(
        Rect::new(content_x, y, half_w, button_h),
        "Save TOML",
        color(91, 105, 73, 255),
    ) {
        app.save_config();
    }
    if draw_control_button(
        Rect::new(content_x + half_w + gap, y, half_w, button_h),
        "Reload TOML",
        color(77, 91, 75, 255),
    ) {
        app.reload_config_files();
    }
    y += 28.0;

    if let Some(status) = &app.save_status {
        draw_text_ex(
            &status.message,
            content_x,
            y,
            TextParams {
                font_size: 13,
                color: if status.success {
                    color(167, 218, 137, 255)
                } else {
                    color(235, 126, 102, 255)
                },
                ..Default::default()
            },
        );
    }
}

fn draw_section_title(x: f32, y: f32, label: &str) {
    draw_text_ex(
        label,
        x,
        y,
        TextParams {
            font_size: 17,
            color: color(205, 211, 184, 255),
            ..Default::default()
        },
    );
}

fn draw_control_button(rect: Rect, label: &str, fill: Color) -> bool {
    let mouse = mouse_vec();
    let hovered = rect.contains(mouse);
    let pressed = hovered && is_mouse_button_down(MouseButton::Left);
    let clicked = hovered && is_mouse_button_pressed(MouseButton::Left);
    let button_fill = if pressed {
        mix(fill, color(255, 255, 255, 255), 0.18)
    } else if hovered {
        mix(fill, color(255, 255, 255, 255), 0.10)
    } else {
        fill
    };

    draw_round_rect(rect.x, rect.y, rect.w, rect.h, 8.0, button_fill);
    draw_rectangle_lines(
        rect.x + 1.0,
        rect.y + 1.0,
        rect.w - 2.0,
        rect.h - 2.0,
        1.0,
        color(255, 255, 255, if hovered { 110 } else { 55 }),
    );
    draw_centered_text(label, rect, 15, color(240, 233, 207, 255));

    clicked
}

fn draw_probability_slider(
    x: f32,
    y: f32,
    width: f32,
    label: &str,
    value: &mut f64,
    accent: Color,
) -> bool {
    let track_x = x + 112.0;
    let track_y = y + 10.0;
    let track_w = (width - 162.0).max(40.0);
    let track_h = 7.0;
    let mouse = mouse_vec();
    let hitbox = Rect::new(track_x - 8.0, y - 4.0, track_w + 16.0, 28.0);
    let changed = hitbox.contains(mouse) && is_mouse_button_down(MouseButton::Left);

    if changed {
        *value = ((mouse.x - track_x) / track_w).clamp(0.0, 1.0) as f64;
    }

    draw_text_ex(
        label,
        x,
        y + 16.0,
        TextParams {
            font_size: 14,
            color: color(154, 170, 158, 255),
            ..Default::default()
        },
    );

    draw_round_rect(
        track_x,
        track_y,
        track_w,
        track_h,
        4.0,
        color(42, 52, 55, 255),
    );
    draw_round_rect(
        track_x,
        track_y,
        track_w * (*value as f32),
        track_h,
        4.0,
        accent,
    );

    let knob_x = track_x + track_w * (*value as f32);
    draw_circle(knob_x, track_y + track_h * 0.5, 8.0, color(16, 21, 23, 220));
    draw_circle(
        knob_x,
        track_y + track_h * 0.5,
        5.8,
        color(239, 232, 203, 255),
    );

    draw_text_ex(
        &format!("{:.2}", *value),
        x + width - 38.0,
        y + 16.0,
        TextParams {
            font_size: 14,
            color: color(232, 223, 188, 255),
            ..Default::default()
        },
    );

    changed
}

fn draw_hover_tooltip(app: &SimulationApp, hovered: Option<(usize, usize)>) {
    let Some((row, col)) = hovered else {
        return;
    };
    let Some(snapshot) = app.board.cell_snapshot(row, col) else {
        return;
    };

    let (mouse_x, mouse_y) = mouse_position();
    let width = 260.0;
    let height = 66.0;
    let x = (mouse_x + 18.0)
        .min(screen_width() - width - 12.0)
        .max(12.0);
    let y = (mouse_y + 18.0)
        .min(screen_height() - height - 12.0)
        .max(12.0);

    draw_round_rect(x, y, width, height, 14.0, color(22, 31, 31, 232));
    draw_rectangle_lines(
        x + 1.0,
        y + 1.0,
        width - 2.0,
        height - 2.0,
        1.0,
        color(209, 194, 142, 125),
    );
    draw_text_ex(
        &format!("Cell ({row}, {col})"),
        x + 16.0,
        y + 26.0,
        TextParams {
            font_size: 17,
            color: color(244, 229, 177, 255),
            ..Default::default()
        },
    );
    draw_text_ex(
        &format!(
            "Food {}  |  Aphids {}  |  Ladybugs {}",
            snapshot.food, snapshot.aphids, snapshot.ladybugs
        ),
        x + 16.0,
        y + 50.0,
        TextParams {
            font_size: 14,
            color: color(178, 191, 177, 255),
            ..Default::default()
        },
    );
}

fn draw_centered_text(label: &str, rect: Rect, font_size: u16, text_color: Color) {
    let dimensions = measure_text(label, None, font_size, 1.0);
    draw_text_ex(
        label,
        rect.x + (rect.w - dimensions.width) * 0.5,
        rect.y + (rect.h + dimensions.height) * 0.5 - 2.0,
        TextParams {
            font_size,
            color: text_color,
            ..Default::default()
        },
    );
}

fn mouse_vec() -> Vec2 {
    let (x, y) = mouse_position();
    vec2(x, y)
}

fn draw_stat(panel_x: f32, y: f32, label: &str, value: &str) {
    draw_text_ex(
        label,
        panel_x + 24.0,
        y,
        TextParams {
            font_size: 14,
            color: color(126, 142, 134, 255),
            ..Default::default()
        },
    );
    draw_text_ex(
        value,
        panel_x + 154.0,
        y,
        TextParams {
            font_size: 15,
            color: color(232, 223, 188, 255),
            ..Default::default()
        },
    );
}

fn status_label(app: &SimulationApp) -> &'static str {
    if app.extinct {
        "Extinct"
    } else if app.playing {
        "Running"
    } else {
        "Paused"
    }
}

fn draw_round_rect(x: f32, y: f32, w: f32, h: f32, radius: f32, fill: Color) {
    let r = radius.min(w * 0.5).min(h * 0.5);
    draw_rectangle(x + r, y, w - r * 2.0, h, fill);
    draw_rectangle(x, y + r, w, h - r * 2.0, fill);
    draw_circle(x + r, y + r, r, fill);
    draw_circle(x + w - r, y + r, r, fill);
    draw_circle(x + r, y + h - r, r, fill);
    draw_circle(x + w - r, y + h - r, r, fill);
}

fn color(red: u8, green: u8, blue: u8, alpha: u8) -> Color {
    Color::from_rgba(red, green, blue, alpha)
}

fn mix(from: Color, to: Color, amount: f32) -> Color {
    let t = amount.clamp(0.0, 1.0);
    Color::new(
        from.r + (to.r - from.r) * t,
        from.g + (to.g - from.g) * t,
        from.b + (to.b - from.b) * t,
        from.a + (to.a - from.a) * t,
    )
}
