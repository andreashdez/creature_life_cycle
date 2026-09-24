# Engineering notes

Records of how this project was built and why it looks the way it does. These
are internal engineering notes, kept as plain Markdown and deliberately
separate from [`docs/`](../docs), which is the user-facing
[Quarkdown](https://quarkdown.com) site.

Two kinds of file live here.

**Decision records** are numbered and named after the decision. Each one states
the context at the time, what was decided, and what it cost. They are written
after the fact, from the commit history, and are not updated when the code
moves on: a record describes the decision as it was made. When a decision is
replaced, a new record supersedes the old one rather than editing it.

**Design sketches** keep their descriptive names. They are longer, more
exploratory documents written while working something out.

## Decision records

| # | Decision | Commit |
|---|----------|--------|
| [0001](0001-deterministic-seeded-simulation.md) | A seeded random wrapper makes every run reproducible | `42ebf2f` |
| [0002](0002-phased-turn-order.md) | A turn is seven phases over a start-of-turn snapshot | `42ebf2f` |
| [0003](0003-toml-configuration.md) | TOML replaces the positional `.conf` files | `fd0d343`, `cb057a7` |
| [0004](0004-xdg-config-location.md) | The config lives in the XDG config home and is created on demand | `a1421f4`, `15ffa6b` |
| [0005](0005-food-regeneration.md) | Food regrows slowly and is capped per cell | `1d3690b` |
| [0006](0006-gui-writes-the-shared-config.md) | The GUI edits the board and saves to the CLI's config file | `2f50845` |
| [0007](0007-macos-app-bundle.md) | A shell script packages the GUI as a macOS app bundle | `32e75e6` |
| [0008](0008-benchmark-as-performance-guard.md) | A Criterion benchmark guards `Board::refresh` | `2967bc4` |
| [0009](0009-flat-creature-storage.md) | Creatures live in flat vectors indexed by ID | `39b5e6f` |
| [0010](0010-macroquad-to-bevy.md) | The GUI moves from macroquad to Bevy | `5f85ae0` |
| [0011](0011-optional-gui-feature.md) | Bevy sits behind an optional, default-on `gui` feature | `5f85ae0` |
| [0012](0012-github-actions-ci.md) | CI runs on GitHub Actions instead of Woodpecker | `2b16e22` |
| [0013](0013-docs-deployed-to-github-pages.md) | The documentation site deploys to GitHub Pages from CI | — |
| [0014](0014-stable-rng-algorithm.md) | The RNG algorithm is named explicitly as `ChaCha12Rng` | — |

## Design sketches

- [Bevy board rendering](bevy-board-rendering.md) — how the Bevy board, panel,
  chart, food overlay, and cell editing were built, and the trade-offs behind
  them. The deep companion to records 0010 and 0011.
