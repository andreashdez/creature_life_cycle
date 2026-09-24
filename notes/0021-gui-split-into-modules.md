# 0021 — The GUI is split into modules by concern

Decided when the GUI outgrew one file. Status: active.

## Context

The Bevy GUI started as a single file, `src/bin/gui.rs`, and by this point it
held 6,231 lines: about 5,000 of application code and 1,200 of tests. Banner
comments divided it into sections, but the sections had drifted. All resources
and components sat in one block at the top, away from the systems that use
them. The viewport layout and chart resizing lived under the parameters panel,
and cell editing lived under the chart. Finding everything that touched one
feature meant searching the whole file.

## Decision

The binary became `src/bin/gui/`, with `main.rs` as its root. It is split by
concern, not by kind of item: each module owns the resources, components,
constants, and systems for one part of the GUI (`board`, `creatures`, `chart`,
`editing`, `panel`, `params`, and so on). `AGENTS.md` lists them all.

`main.rs` still holds the whole `Update` schedule. System order matters, and
the comments explaining it only make sense when the full chain is visible in
one place. Per-module Bevy plugins were rejected because they would spread
that order across files.

The split moved code without changing it. Items became `pub` only so sibling
modules can reach them. The asset macros now use `../../../assets/...`,
because `embedded_asset!` and `load_embedded_asset!` resolve paths relative to
the calling file.

The tests stay together in `tests.rs`, because most of them share the
`world_mid_run` fixture.

## Consequences

The largest module is `chart.rs`, at about 900 lines, and most are between 100
and 600. A change to one feature usually touches one or two files.

Cross-module items are imported by name, so each module's `use` lines show
what it depends on. The cost is some visibility noise: `pub` on items that
were file-private before.

Every file that uses the embedded-asset macros must stay directly in
`src/bin/gui/`. A file in a subdirectory would compute a different embedded
path from the same relative string, and its assets would fail to load at
runtime, not at compile time.
