use creature_life_cycle::{Board, Random, parse_simulation_config};
use criterion::{BatchSize, Criterion, black_box, criterion_group, criterion_main};
use std::fmt::Write as _;
use std::sync::LazyLock;

const ROWS: usize = 25;
const COLS: usize = 25;
const CREATURES_PER_KIND: usize = 800;
const SEED: u64 = 42;

static DENSE_CONFIG: LazyLock<String> = LazyLock::new(build_dense_config);

fn board_with_random() -> (Board, Random) {
    let mut random = Random::with_seed(SEED);
    let mut board = Board::new();
    parse_simulation_config(DENSE_CONFIG.as_str(), &mut board, &mut random)
        .expect("benchmark config is valid");
    (board, random)
}

fn bench_board_refresh(c: &mut Criterion) {
    let mut group = c.benchmark_group("board_refresh");

    group.bench_function("dense_25x25_1600_creatures", |bench| {
        bench.iter_batched(
            board_with_random,
            |(mut board, mut random)| {
                black_box(board.refresh(&mut random));
            },
            BatchSize::SmallInput,
        );
    });

    group.finish();
}

fn build_dense_config() -> String {
    let mut contents = String::new();

    writeln!(&mut contents, "[board]").expect("write to string");
    writeln!(&mut contents, "rows = {ROWS}").expect("write to string");
    writeln!(&mut contents, "columns = {COLS}").expect("write to string");
    writeln!(&mut contents).expect("write to string");
    write_creature_positions(&mut contents, "aphids", 0);
    writeln!(&mut contents).expect("write to string");
    write_creature_positions(&mut contents, "ladybugs", 1);
    writeln!(&mut contents).expect("write to string");

    writeln!(&mut contents, "[aphid]").expect("write to string");
    writeln!(&mut contents, "move_probability = 0.7").expect("write to string");
    writeln!(&mut contents, "kill_probability = 0.2").expect("write to string");
    writeln!(&mut contents, "accomplice_probability = 0.1").expect("write to string");
    writeln!(&mut contents, "procreation_probability = 0.4").expect("write to string");
    writeln!(&mut contents).expect("write to string");

    writeln!(&mut contents, "[ladybug]").expect("write to string");
    writeln!(&mut contents, "move_probability = 0.7").expect("write to string");
    writeln!(&mut contents, "kill_probability = 0.2").expect("write to string");
    writeln!(&mut contents, "direction_change_probability = 0.4").expect("write to string");
    writeln!(&mut contents, "procreation_probability = 0.2").expect("write to string");
    writeln!(&mut contents).expect("write to string");

    writeln!(&mut contents, "[food]").expect("write to string");
    writeln!(&mut contents, "regeneration_probability = 0.1").expect("write to string");

    contents
}

fn write_creature_positions(contents: &mut String, label: &str, offset: usize) {
    writeln!(contents, "{label} = [").expect("write to string");
    for index in 0..CREATURES_PER_KIND {
        let x = (index * 37 + offset * 11) % ROWS;
        let y = (index * 17 + index / ROWS + offset * 29) % COLS;
        writeln!(contents, "  {{ x = {x}, y = {y} }},").expect("write to string");
    }
    writeln!(contents, "]").expect("write to string");
}

criterion_group!(benches, bench_board_refresh);
criterion_main!(benches);
