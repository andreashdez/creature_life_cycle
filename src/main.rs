use clap::Parser;
use creature_life_cycle::{
    Board, Random, TurnStats, load_standard_data, parse_aphid_params, parse_board,
    parse_ladybug_params,
};
use std::fs;
use std::thread;
use std::time::Duration;

/// Command-line simulation options.
#[derive(Clone, Copy, Debug, Parser, PartialEq, Eq)]
#[command(version, about = "Simulate aphids and ladybugs on a board.")]
struct SimOptions {
    /// Maximum number of turns to simulate.
    #[arg(long, default_value_t = 60, value_name = "number")]
    turns: usize,
    /// Delay between printed turns in milliseconds.
    #[arg(long, default_value_t = 400, value_name = "number")]
    delay_ms: u64,
    /// Optional deterministic random seed.
    #[arg(long, value_name = "number")]
    seed: Option<u64>,
}

/// Program entry point: parse options, load configuration, and run the simulation loop.
fn main() {
    let options = SimOptions::parse();
    let mut random = match options.seed {
        Some(seed) => Random::with_seed(seed),
        None => Random::new(),
    };
    let mut board = Board::new();

    read_board(&mut board, &mut random);
    read_aphids(&mut board);
    read_ladybugs(&mut board);

    print_board(&board);

    let mut extinction = false;
    let mut turn = 0;

    while !extinction && turn < options.turns {
        let stats = board.refresh(&mut random);
        extinction = stats.summary.is_extinct();
        print_board(&board);
        turn += 1;
        print_turn_stats(turn, stats);

        if options.delay_ms > 0 {
            thread::sleep(Duration::from_millis(options.delay_ms));
        }
    }
}

/// Reads `board.conf`, falling back to built-in standard data on errors.
fn read_board(board: &mut Board, random: &mut Random) {
    let Ok(contents) = fs::read_to_string("board.conf") else {
        eprintln!("File \"board.conf\" not found, using standard data.");
        load_standard_data(board, random);
        return;
    };

    if let Err(error) = parse_board(&contents, board, random) {
        eprintln!("Invalid board.conf ({error}), using standard data.");
        load_standard_data(board, random);
    }
}

/// Reads `aphid.conf`, keeping default aphid parameters on errors.
fn read_aphids(board: &mut Board) {
    let Ok(contents) = fs::read_to_string("aphid.conf") else {
        eprintln!("File \"aphid.conf\" not found, using standard data.");
        return;
    };

    match parse_aphid_params(&contents) {
        Ok(params) => board.set_aphid_params(params),
        Err(error) => eprintln!("Invalid aphid.conf ({error}), using standard data."),
    }
}

/// Reads `ladybug.conf`, keeping default ladybug parameters on errors.
fn read_ladybugs(board: &mut Board) {
    let Ok(contents) = fs::read_to_string("ladybug.conf") else {
        eprintln!("File \"ladybug.conf\" not found, using standard data.");
        return;
    };

    match parse_ladybug_params(&contents) {
        Ok(params) => board.set_ladybug_params(params),
        Err(error) => eprintln!("Invalid ladybug.conf ({error}), using standard data."),
    }
}

/// Prints the board as two count symbols per cell: aphids first, ladybugs second.
fn print_board(board: &Board) {
    println!();
    for x in 0..board.rows() {
        for y in 0..board.cols() {
            let (aphids, ladybugs) = board.cell_counts(x, y).unwrap_or((0, 0));
            print!(" {}{}", count_symbol(aphids), count_symbol(ladybugs));
        }
        println!();
    }
}

/// Prints the numeric summary for a completed turn.
fn print_turn_stats(turn: usize, stats: TurnStats) {
    println!(
        "Turn: {turn} | Aphids: {} | Ladybugs: {} | Births: {} | Deaths: {} | Food: {}",
        stats.summary.aphids,
        stats.summary.ladybugs,
        stats.births,
        stats.deaths,
        stats.summary.food
    );
}

/// Converts a creature count to the compact board display symbol.
fn count_symbol(count: usize) -> char {
    match count {
        0 => '_',
        1..=9 => char::from_digit(count as u32, 10).unwrap_or('~'),
        _ => '~',
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::error::ErrorKind;

    #[test]
    fn parse_options_accepts_seed() {
        let options = SimOptions::try_parse_from([
            "creature_life_cycle",
            "--turns",
            "3",
            "--delay-ms",
            "0",
            "--seed",
            "99",
        ])
        .unwrap();

        assert_eq!(
            options,
            SimOptions {
                turns: 3,
                delay_ms: 0,
                seed: Some(99)
            }
        );
    }

    #[test]
    fn parse_options_accepts_help() {
        assert_eq!(
            SimOptions::try_parse_from(["creature_life_cycle", "--help"])
                .unwrap_err()
                .kind(),
            ErrorKind::DisplayHelp
        );
        assert_eq!(
            SimOptions::try_parse_from(["creature_life_cycle", "-h"])
                .unwrap_err()
                .kind(),
            ErrorKind::DisplayHelp
        );
    }

    #[test]
    fn parse_options_rejects_invalid_input() {
        assert!(SimOptions::try_parse_from(["creature_life_cycle", "--turns"]).is_err());
        assert!(SimOptions::try_parse_from(["creature_life_cycle", "--turns", "abc"]).is_err());
        assert!(SimOptions::try_parse_from(["creature_life_cycle", "--unknown"]).is_err());
    }
}
