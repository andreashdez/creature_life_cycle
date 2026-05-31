use creature_life_cycle::{Board, Random, parse_simulation_config};
use criterion::measurement::WallTime;
use criterion::{BatchSize, BenchmarkGroup, Criterion, criterion_group, criterion_main};
use std::fmt::Write as _;
use std::hint::black_box;
use std::sync::LazyLock;
use std::time::Duration;

const ROWS: usize = 25;
const COLS: usize = 25;
const LARGE_ROWS: usize = 100;
const LARGE_COLS: usize = 100;
const CREATURES_PER_KIND: usize = 800;
const MULTI_TURN_COUNT: usize = 20;
const SEED: u64 = 42;

type PositionWriter = fn(&mut String, &str, usize, usize, usize, usize);

#[derive(Clone, Copy)]
struct BehaviorParams {
    aphid_move: f64,
    aphid_kill: f64,
    aphid_accomplice: f64,
    aphid_procreate: f64,
    ladybug_move: f64,
    ladybug_kill: f64,
    ladybug_direction: f64,
    ladybug_procreate: f64,
    food_regenerate: f64,
}

const DEFAULT_PARAMS: BehaviorParams = BehaviorParams {
    aphid_move: 0.7,
    aphid_kill: 0.2,
    aphid_accomplice: 0.1,
    aphid_procreate: 0.4,
    ladybug_move: 0.7,
    ladybug_kill: 0.2,
    ladybug_direction: 0.4,
    ladybug_procreate: 0.2,
    food_regenerate: 0.1,
};

const STABLE_PARAMS: BehaviorParams = BehaviorParams {
    aphid_procreate: 0.0,
    ladybug_procreate: 0.0,
    ..DEFAULT_PARAMS
};

static DENSE_CONFIG: LazyLock<String> = LazyLock::new(build_dense_config);
static CLUSTERED_CONFIG: LazyLock<String> = LazyLock::new(build_clustered_config);
static SPARSE_CONFIG: LazyLock<String> = LazyLock::new(build_sparse_config);
static STABLE_DENSE_CONFIG: LazyLock<String> = LazyLock::new(build_stable_dense_config);

fn board_with_config(config: &str) -> (Board, Random) {
    let mut random = Random::with_seed(SEED);
    let mut board = Board::new();
    parse_simulation_config(config, &mut board, &mut random).expect("benchmark config is valid");
    (board, random)
}

fn bench_board_refresh(c: &mut Criterion) {
    let mut group = c.benchmark_group("board_refresh");
    configure_group(&mut group);

    bench_single_refresh(&mut group, "dense_25x25_1600_creatures", &DENSE_CONFIG);
    bench_single_refresh(
        &mut group,
        "clustered_25x25_1600_creatures",
        &CLUSTERED_CONFIG,
    );
    bench_single_refresh(&mut group, "sparse_100x100_1600_creatures", &SPARSE_CONFIG);
    bench_multi_turn_refresh(
        &mut group,
        "stable_dense_25x25_1600_creatures_20_turns",
        &STABLE_DENSE_CONFIG,
        MULTI_TURN_COUNT,
    );

    group.finish();
}

fn bench_board_snapshots(c: &mut Criterion) {
    let mut group = c.benchmark_group("board_snapshots");
    configure_group(&mut group);

    let (board, _random) = board_with_config(DENSE_CONFIG.as_str());
    let mut snapshots = Vec::new();

    group.bench_function("write_snapshots_dense_25x25_1600_creatures", |bench| {
        bench.iter(|| {
            board.write_creature_snapshots(&mut snapshots);
            black_box(snapshots.len());
        });
    });

    group.finish();
}

fn configure_group(group: &mut BenchmarkGroup<'_, WallTime>) {
    group.warm_up_time(Duration::from_secs(1));
    group.measurement_time(Duration::from_secs(5));
    group.sample_size(50);
}

fn bench_single_refresh(
    group: &mut BenchmarkGroup<'_, WallTime>,
    name: &str,
    config: &'static LazyLock<String>,
) {
    group.bench_function(name, |bench| {
        bench.iter_batched(
            || board_with_config(config.as_str()),
            |(mut board, mut random)| {
                black_box(board.refresh(&mut random));
            },
            BatchSize::SmallInput,
        );
    });
}

fn bench_multi_turn_refresh(
    group: &mut BenchmarkGroup<'_, WallTime>,
    name: &str,
    config: &'static LazyLock<String>,
    turns: usize,
) {
    group.bench_function(name, |bench| {
        bench.iter_batched(
            || board_with_config(config.as_str()),
            |(mut board, mut random)| {
                for _ in 0..turns {
                    black_box(board.refresh(&mut random));
                }
                black_box(board.summary());
            },
            BatchSize::SmallInput,
        );
    });
}

fn build_dense_config() -> String {
    build_config(
        ROWS,
        COLS,
        CREATURES_PER_KIND,
        write_spread_positions,
        DEFAULT_PARAMS,
    )
}

fn build_clustered_config() -> String {
    build_config(
        ROWS,
        COLS,
        CREATURES_PER_KIND,
        write_clustered_positions,
        DEFAULT_PARAMS,
    )
}

fn build_sparse_config() -> String {
    build_config(
        LARGE_ROWS,
        LARGE_COLS,
        CREATURES_PER_KIND,
        write_spread_positions,
        DEFAULT_PARAMS,
    )
}

fn build_stable_dense_config() -> String {
    build_config(
        ROWS,
        COLS,
        CREATURES_PER_KIND,
        write_spread_positions,
        STABLE_PARAMS,
    )
}

fn build_config(
    rows: usize,
    cols: usize,
    creatures_per_kind: usize,
    position_writer: PositionWriter,
    params: BehaviorParams,
) -> String {
    let mut contents = String::new();

    writeln!(&mut contents, "[board]").expect("write to string");
    writeln!(&mut contents, "rows = {rows}").expect("write to string");
    writeln!(&mut contents, "columns = {cols}").expect("write to string");
    writeln!(&mut contents).expect("write to string");
    position_writer(&mut contents, "aphids", rows, cols, creatures_per_kind, 0);
    writeln!(&mut contents).expect("write to string");
    position_writer(&mut contents, "ladybugs", rows, cols, creatures_per_kind, 1);
    writeln!(&mut contents).expect("write to string");

    writeln!(&mut contents, "[aphid]").expect("write to string");
    writeln!(&mut contents, "move_probability = {}", params.aphid_move).expect("write to string");
    writeln!(&mut contents, "kill_probability = {}", params.aphid_kill).expect("write to string");
    writeln!(
        &mut contents,
        "accomplice_probability = {}",
        params.aphid_accomplice
    )
    .expect("write to string");
    writeln!(
        &mut contents,
        "procreation_probability = {}",
        params.aphid_procreate
    )
    .expect("write to string");
    writeln!(&mut contents).expect("write to string");

    writeln!(&mut contents, "[ladybug]").expect("write to string");
    writeln!(&mut contents, "move_probability = {}", params.ladybug_move).expect("write to string");
    writeln!(&mut contents, "kill_probability = {}", params.ladybug_kill).expect("write to string");
    writeln!(
        &mut contents,
        "direction_change_probability = {}",
        params.ladybug_direction
    )
    .expect("write to string");
    writeln!(
        &mut contents,
        "procreation_probability = {}",
        params.ladybug_procreate
    )
    .expect("write to string");
    writeln!(&mut contents).expect("write to string");

    writeln!(&mut contents, "[food]").expect("write to string");
    writeln!(
        &mut contents,
        "regeneration_probability = {}",
        params.food_regenerate
    )
    .expect("write to string");

    contents
}

fn write_spread_positions(
    contents: &mut String,
    label: &str,
    rows: usize,
    cols: usize,
    creatures_per_kind: usize,
    offset: usize,
) {
    writeln!(contents, "{label} = [").expect("write to string");
    for index in 0..creatures_per_kind {
        let x = (index * 37 + offset * 11) % rows;
        let y = (index * 17 + index / rows + offset * 29) % cols;
        writeln!(contents, "  {{ x = {x}, y = {y} }},").expect("write to string");
    }
    writeln!(contents, "]").expect("write to string");
}

fn write_clustered_positions(
    contents: &mut String,
    label: &str,
    rows: usize,
    cols: usize,
    creatures_per_kind: usize,
    offset: usize,
) {
    let cluster_rows = rows.min(5);
    let cluster_cols = cols.min(5);
    let start_x = (rows - cluster_rows) / 2;
    let start_y = (cols - cluster_cols) / 2;

    writeln!(contents, "{label} = [").expect("write to string");
    for index in 0..creatures_per_kind {
        let x = start_x + (index * 7 + offset * 3) % cluster_rows;
        let y = start_y + (index * 11 + index / cluster_rows + offset * 2) % cluster_cols;
        writeln!(contents, "  {{ x = {x}, y = {y} }},").expect("write to string");
    }
    writeln!(contents, "]").expect("write to string");
}

criterion_group!(benches, bench_board_refresh, bench_board_snapshots);
criterion_main!(benches);
