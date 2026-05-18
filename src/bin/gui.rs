use creature_life_cycle::{
    Board, BoardSummary, CellSnapshot, Random, load_standard_data, parse_aphid_params, parse_board,
    parse_ladybug_params,
};
use macroquad::prelude::*;
use std::fs;

const DEFAULT_SEED: u64 = 42;

struct SimulationApp {
    board: Board,
    random: Random,
    seed: u64,
    turn: usize,
    summary: BoardSummary,
    last_births: usize,
    last_deaths: usize,
    playing: bool,
    extinct: bool,
    speed: f32,
    accumulator: f32,
}

impl SimulationApp {
    fn new(seed: u64) -> Self {
        let (board, random) = load_simulation(seed);
        let summary = board.summary();
        Self {
            board,
            random,
            seed,
            turn: 0,
            summary,
            last_births: 0,
            last_deaths: 0,
            playing: true,
            extinct: summary.is_extinct(),
            speed: 1.0,
            accumulator: 0.0,
        }
    }

    fn reset(&mut self) {
        *self = Self::new(self.seed);
    }

    fn step(&mut self) {
        if self.extinct {
            return;
        }

        let stats = self.board.refresh(&mut self.random);
        self.turn += 1;
        self.summary = stats.summary;
        self.last_births = stats.births;
        self.last_deaths = stats.deaths;
        self.extinct = stats.summary.is_extinct();
    }
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
}

fn window_conf() -> Conf {
    Conf {
        window_title: "Aphids and Ladybugs".to_string(),
        window_width: 1120,
        window_height: 760,
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
        draw_board(&app.board, layout, hovered);
        draw_panel(&app, layout, hovered);

        next_frame().await;
    }
}

fn load_simulation(seed: u64) -> (Board, Random) {
    let mut random = Random::with_seed(seed);
    let mut board = Board::new();

    match fs::read_to_string("board.conf") {
        Ok(contents) => {
            if let Err(error) = parse_board(&contents, &mut board, &mut random) {
                eprintln!("Invalid board.conf ({error}), using standard data.");
                load_standard_data(&mut board, &mut random);
            }
        }
        Err(_) => {
            eprintln!("File \"board.conf\" not found, using standard data.");
            load_standard_data(&mut board, &mut random);
        }
    }

    match fs::read_to_string("aphid.conf") {
        Ok(contents) => match parse_aphid_params(&contents) {
            Ok(params) => board.set_aphid_params(params),
            Err(error) => eprintln!("Invalid aphid.conf ({error}), using standard data."),
        },
        Err(_) => eprintln!("File \"aphid.conf\" not found, using standard data."),
    }

    match fs::read_to_string("ladybug.conf") {
        Ok(contents) => match parse_ladybug_params(&contents) {
            Ok(params) => board.set_ladybug_params(params),
            Err(error) => eprintln!("Invalid ladybug.conf ({error}), using standard data."),
        },
        Err(_) => eprintln!("File \"ladybug.conf\" not found, using standard data."),
    }

    (board, random)
}

fn handle_input(app: &mut SimulationApp) {
    if is_key_pressed(KeyCode::Space) {
        app.playing = !app.playing;
        app.accumulator = 0.0;
    }

    if is_key_pressed(KeyCode::N) {
        app.step();
        app.accumulator = 0.0;
    }

    if is_key_pressed(KeyCode::R) {
        app.reset();
    }

    if is_key_pressed(KeyCode::Up) || is_key_pressed(KeyCode::Equal) {
        app.speed = (app.speed + 0.5).min(8.0);
    }

    if is_key_pressed(KeyCode::Down) || is_key_pressed(KeyCode::Minus) {
        app.speed = (app.speed - 0.5).max(0.5);
    }
}

fn tick_simulation(app: &mut SimulationApp) {
    if !app.playing || app.extinct {
        return;
    }

    app.accumulator += get_frame_time();
    let step_interval = 1.0 / app.speed;
    let mut steps = 0;

    while app.accumulator >= step_interval && steps < 8 && !app.extinct {
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
    let panel_w = if wide { 302.0 } else { screen_w - margin * 2.0 };
    let panel_h = if wide { screen_h - top - margin } else { 132.0 };
    let board_area_w = if wide {
        screen_w - panel_w - margin * 3.0
    } else {
        screen_w - margin * 2.0
    };
    let board_area_h = if wide {
        screen_h - top - margin
    } else {
        screen_h - top - panel_h - margin * 2.0
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
    let panel_x = if wide {
        screen_w - panel_w - margin
    } else {
        margin
    };
    let panel_y = if wide { top } else { y + height + margin };

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

fn draw_board(board: &Board, layout: BoardLayout, hovered: Option<(usize, usize)>) {
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
            let Some(snapshot) = board.cell_snapshot(row, col) else {
                continue;
            };
            let x = layout.x + col as f32 * layout.cell + gap * 0.5;
            let y = layout.y + row as f32 * layout.cell + gap * 0.5;
            draw_cell(x, y, inner, row, col, snapshot, hovered == Some((row, col)));
        }
    }
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
    draw_creatures(x, y, size, snapshot.aphids, snapshot.ladybugs);

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

fn draw_panel(app: &SimulationApp, layout: BoardLayout, hovered: Option<(usize, usize)>) {
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

    let mut y = layout.panel_y + 34.0;
    draw_text_ex(
        "Simulation",
        layout.panel_x + 24.0,
        y,
        TextParams {
            font_size: 24,
            color: color(238, 232, 210, 255),
            ..Default::default()
        },
    );
    y += 34.0;

    draw_stat(layout.panel_x, y, "Status", status_label(app));
    y += 28.0;
    draw_stat(layout.panel_x, y, "Turn", &app.turn.to_string());
    y += 28.0;
    draw_stat(layout.panel_x, y, "Aphids", &app.summary.aphids.to_string());
    y += 28.0;
    draw_stat(
        layout.panel_x,
        y,
        "Ladybugs",
        &app.summary.ladybugs.to_string(),
    );
    y += 28.0;
    draw_stat(layout.panel_x, y, "Food", &app.summary.food.to_string());
    y += 28.0;
    draw_stat(
        layout.panel_x,
        y,
        "Last Births",
        &app.last_births.to_string(),
    );
    y += 28.0;
    draw_stat(
        layout.panel_x,
        y,
        "Last Deaths",
        &app.last_deaths.to_string(),
    );
    y += 28.0;
    draw_stat(
        layout.panel_x,
        y,
        "Speed",
        &format!("{:.1} turns/s", app.speed),
    );
    y += 42.0;

    draw_text_ex(
        "Controls",
        layout.panel_x + 24.0,
        y,
        TextParams {
            font_size: 20,
            color: color(205, 211, 184, 255),
            ..Default::default()
        },
    );
    y += 28.0;

    for control in [
        "Space  play / pause",
        "N      step one turn",
        "R      reset seed 42",
        "Up/Down adjust speed",
        "Esc    quit",
    ] {
        draw_text_ex(
            control,
            layout.panel_x + 24.0,
            y,
            TextParams {
                font_size: 17,
                color: color(154, 170, 158, 255),
                ..Default::default()
            },
        );
        y += 24.0;
    }

    if let Some((row, col)) = hovered
        && let Some(snapshot) = app.board.cell_snapshot(row, col)
    {
        let hover_y = (layout.panel_y + layout.panel_h - 92.0).max(y + 18.0);
        draw_round_rect(
            layout.panel_x + 18.0,
            hover_y,
            layout.panel_w - 36.0,
            70.0,
            14.0,
            color(38, 50, 47, 190),
        );
        draw_text_ex(
            &format!("Cell ({row}, {col})"),
            layout.panel_x + 36.0,
            hover_y + 26.0,
            TextParams {
                font_size: 18,
                color: color(244, 229, 177, 255),
                ..Default::default()
            },
        );
        draw_text_ex(
            &format!(
                "Food {}  |  Aphids {}  |  Ladybugs {}",
                snapshot.food, snapshot.aphids, snapshot.ladybugs
            ),
            layout.panel_x + 36.0,
            hover_y + 52.0,
            TextParams {
                font_size: 15,
                color: color(178, 191, 177, 255),
                ..Default::default()
            },
        );
    }
}

fn draw_stat(panel_x: f32, y: f32, label: &str, value: &str) {
    draw_text_ex(
        label,
        panel_x + 24.0,
        y,
        TextParams {
            font_size: 16,
            color: color(126, 142, 134, 255),
            ..Default::default()
        },
    );
    draw_text_ex(
        value,
        panel_x + 154.0,
        y,
        TextParams {
            font_size: 17,
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
