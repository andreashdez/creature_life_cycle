# AGENTS.md

Guidance for coding agents working on this repository. Read this first, then
the file you are about to change.

## What this is

A small Rust (edition 2024) simulation of aphids and ladybugs on a 2D board:
they move, fight, reproduce, eat, and starve. One library holds the rules; two
front ends drive it.

| Path                            | Contents                                                                                                                                   |
| ------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------ |
| `src/lib.rs`                    | The whole simulation: `Board`, `Random`, params, TOML config load/parse/format/save, and the rule tests. The only place rules live.        |
| `src/main.rs`                   | CLI (`clap`): prints the board and a per-turn summary. Small.                                                                              |
| `src/bin/gui/`                  | Bevy GUI, one module per concern: board, creatures, `bevy_feathers` sidebars, population chart, editing, GUI tests. `main.rs` wires it up. |
| `tests/seeded_e2e_snapshots.rs` | End-to-end CLI runs compared byte-for-byte against expected stdout.                                                                        |
| `benches/board_refresh.rs`      | Criterion benchmark of `Board::refresh`.                                                                                                   |
| `assets/`                       | Cell shader and 32×32 pixel-art sprites, embedded into the binary with `embedded_asset!`.                                                  |
| `docs/`                         | User-facing [Quarkdown](https://quarkdown.com) site (`.qd`), deployed to GitHub Pages.                                                     |
| `notes/`                        | Internal engineering notes: numbered decision records plus design sketches.                                                                |
| `scripts/package_gui.sh`        | macOS `.app` bundle + zip (macOS only).                                                                                                    |
| `simulation.example.toml`       | Mirrors the built-in defaults; the e2e tests use it as their config.                                                                       |

## Commands

Run these before calling a change done. They are exactly what the Linux CI
job runs (`.github/workflows/ci.yml`). A second job runs `cargo test --locked`
on macOS, where the GUI ships:

```sh
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo test --locked --bench '*'
cargo check --locked --no-default-features --all-targets
RUSTDOCFLAGS="-D warnings" cargo doc --locked --no-deps --document-private-items
```

Other useful commands:

```sh
cargo run -- --turns 10 --delay-ms 0 --seed 42   # quick deterministic CLI run
cargo run --bin gui                              # GUI; first build compiles Bevy and is slow
cargo build --no-default-features                # library + CLI only, no Bevy (fast)
cargo bench                                      # Criterion; reports in target/criterion
cd docs && quarkdown c main.qd --strict          # build the docs site
```

For work that touches only `src/lib.rs` or `src/main.rs`, iterating with
`cargo test --no-default-features` avoids compiling Bevy. Still run the full
set above before finishing.

## Invariants to preserve

**Determinism.** Every random decision goes through the `Random` wrapper
(`rand_chacha::ChaCha12Rng`) and takes `&mut Random`. Never call `rand` directly or
add a second RNG. The same config and seed must give the same run. Changing
the order or number of RNG calls changes seeded output. That is sometimes
intended (a rule change), but then the expected strings in
`tests/seeded_e2e_snapshots.rs` must be updated on purpose, and the change
should be called out. If the change is not meant to alter behaviour, the
snapshots must still pass unchanged. See `notes/0001`.

**Turn order.** `Board::refresh` runs seven phases over a snapshot of the
creatures that existed at turn start: movement, combat, remove dead, procreation,
starvation, remove dead, food regeneration. Within a phase, creatures go in
creation order. Newborns do not act until the next turn. See `notes/0002` and
the README's rule sections, which are the spec.

**Rules live in the library.** Neither front end reimplements simulation logic.
The GUI keeps `Board` in a resource and advances only through `Board::refresh`;
edits go through `add_*` / `remove_*_at`.

**The library never prints.** Failures are returned as `ConfigError` /
`InvalidConfig`, and things a successful load should report as `ConfigNotice`.
Printing, and choosing where a message goes, belongs to `src/main.rs` and
`src/bin/gui/`. Add a variant rather than returning a `String`. See
`notes/0017`.

**The `gui` feature gate.** Bevy is optional behind the default-on `gui`
feature. The library, CLI, tests, and benches must build without it (CI checks
with `--no-default-features`). Do not use Bevy types outside `src/bin/gui/`.

**One config format.** TOML parsing and writing live together in `lib.rs`
(`parse_simulation_config`, `format_simulation_config`, `save_simulation_config`).
They must round-trip; there is a test for that. Unknown fields are rejected and
probabilities are validated to `0.0..=1.0`. The CLI and GUI share
`${XDG_CONFIG_HOME:-~/.config}/creature_life_cycle/simulation.toml`.

**Tests never touch the real config.** Tests that need a config set
`XDG_CONFIG_HOME` to a directory under `target/` (see the e2e helpers). Because
`set_var` is process-wide (and `unsafe` in edition 2024), GUI tests that depend
on it are merged into one test instead of running in parallel. Follow the same
pattern.

**Performance.** Creatures are stored in flat vectors indexed by ID, with
per-cell occupant slots maintained by swap-remove (`notes/0009`). Keep
`Board::refresh` allocation-light. If you touch it, run `cargo bench` before
and after and report the difference.

## GUI conventions (`src/bin/gui/`)

- The GUI is split into modules by concern, each owning its resources,
  components, constants, and systems. Put new code in the matching module (see
  `notes/0021`):

  | Module        | Owns                                                                 |
  | ------------- | -------------------------------------------------------------------- |
  | `main`        | The `App`: plugins, resources, and the whole `Update` schedule       |
  | `startup`     | `setup`: loading the board, spawning cameras, cells, and panels      |
  | `simulation`  | `BoardRes`, `Rng`, `Stats`, and `Simulation`, the one path to a turn |
  | `history`     | The population history and its rule-change and extinction events     |
  | `board`       | Cell shader, food shading and overlay, distant population markers    |
  | `creatures`   | Sprites, movement tweens and trails, count badges                    |
  | `hover`       | The hovered cell, its outline, and the inspector popup               |
  | `editing`     | Edit tools, `CellEdit`, undo, and the edit tab's controls            |
  | `input`       | Keyboard shortcuts, board pan and zoom, click versus drag            |
  | `run_control` | `RunAction` and `Reseed`: playback, reset, seed, save, status line   |
  | `params`      | The nine probabilities and the parameters tab                        |
  | `panel`       | The properties panel: tabs, sections, scrolling, board view          |
  | `summary`     | The left sidebar: population cards, playback buttons, readouts       |
  | `chart`       | The population chart and the palette its text is checked against     |
  | `layout`      | Sidebar widths, camera markers, and viewport layout                  |
  | `widgets`     | Small shared UI helpers                                              |
  | `probe`       | The `CLC_SCREENSHOT` probe                                           |
  | `tests`       | GUI tests, sharing one `World` fixture                               |

- Items are `pub` only so sibling modules can reach them; nothing leaves the
  binary. Import them by name (`use crate::board::{CELL, GAP};`), not by glob,
  except in `tests`.
- `embedded_asset!`, `load_embedded_asset!`, and `embedded_path!` resolve paths
  relative to the calling file, so every module uses `../../../assets/...`.
  Keep asset macros in `src/bin/gui/` itself, not a subdirectory, or the
  registered and loaded paths stop matching.
- `main` registers `Update` systems as `.chain()`ed tuples: input and
  simulation first, then rendering from the settled state. Order matters and the
  inline comments explain why. A tuple holds at most 20 systems, so split
  rather than nest arbitrarily.
- Tell cameras apart with marker components, never `.single()` over `Camera2d`.
- UI actions go through messages (`RunAction`, `CellEdit`, `Reseed`,
  `ParameterReset`) and widget observers. Do not write resources directly from
  input handlers when a message path exists.
- Colour constants carry their rationale (source, WCAG contrast). Text must
  meet 4.5:1 on its surface. Keep that when adding or changing colours.
- Sprites are 32×32 and sampled with `ImageSampler::nearest()`. New assets must
  be registered with `embedded_asset!` in `board_render_assets` so packaged
  builds work without an asset directory.

**Checking the GUI without a human.** Set `CLC_SCREENSHOT` to have the app run
fast turns, drive a slider, the food toggle, edit tools, and the chart
crosshair through the real input paths, save a PNG, and exit:

```sh
CLC_SCREENSHOT=target/gui.png cargo run --bin gui
CLC_SCREENSHOT=target/gui-compact.png CLC_COMPACT=1 cargo run --bin gui  # 900×640 minimum window
```

Look at the resulting image after any visual change. If you add UI the probe
should exercise, extend `screenshot_probe`.

## Documentation

User-visible behaviour is documented in three places. Keep them in sync in the
same change:

1. `README.md`, which covers controls, CLI flags, config format, and rules.
2. `docs/*.qd`, the Quarkdown site: `gui.qd`, `cli.qd`, `configuration.qd`,
   `simulation-rules.qd`, `architecture.qd`, `development.qd`. A new page must
   be linked from `docs/_nav.qd` or it is not compiled. CI builds with
   `--strict`.
3. Code doc comments. Every function has a `///` comment, and comments explain
   _why_ rather than restating the code.

`notes/` holds decision records written after the fact. Do not edit an
existing record to match new code. When a decision changes, add the next
numbered record that supersedes it, and add it to the tables in
`notes/README.md` and `docs/architecture.qd`. Design sketches such as
`notes/bevy-board-rendering.md` are deeper background for the GUI.

Prose style throughout is plain, complete sentences with concrete numbers and
no marketing tone.

## Commits

Messages are short, lowercase, and imperative, with no prefix or scope:
`add benchmark`, `fix creature sizing`, `refactor ui`. Work lands on `main`;
CI runs on pushes to `main` and on PRs. Dependabot opens weekly update PRs
(`.github/dependabot.yml`). One that changes the seeded snapshots needs the
same deliberate handling as any other change to seeded output.
