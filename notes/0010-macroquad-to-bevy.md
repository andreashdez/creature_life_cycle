# 0010 — The GUI moves from macroquad to Bevy

Decided in `5f85ae0`. Status: active.

## Context

The original GUI was built on macroquad (`5a0fc02`) and had grown well past
what an immediate-mode drawing library makes comfortable. Everything was
positioned by hand: a `BoardLayout` struct with seventeen fields computed cell
sizes and offsets each frame, panning and zooming were arithmetic on those
fields, and every widget — sliders, buttons, text fields — was drawn and
hit-tested manually. Adding a control meant finding pixels for it and writing
its interaction from scratch.

## Decision

The GUI was rewritten on Bevy 0.19 with the `bevy_feathers` widget set, taking
only the parts of the engine it needs: `2d`, `ui`, `mouse`, `keyboard`, and
`bevy_feathers`, with default features off.

Most of the hand-rolled machinery was deleted in the process. `BoardLayout`
became a `Camera2d` and a `Transform`, so panning and zooming are camera
operations. Widgets came from `bevy_feathers`, which also supplies focus
handling and the embedded Fira Sans typeface. Sprites and the cell shader are
embedded into the executable at build time rather than loaded from disk beside
it, which is what lets the macOS bundle from record
[0007](0007-macos-app-bundle.md) ship as a single self-contained artifact.

[bevy-board-rendering.md](bevy-board-rendering.md) is the design sketch written
while doing this work. It records the layout arithmetic, the three options
considered for cell backgrounds, the animation approach, and the panel, chart,
food overlay, and editing designs in detail.

## Consequences

The UI gained real widgets, keyboard focus, and a scrolling settings panel for
roughly the cost of describing them, and the board gained smooth pan and zoom
for free from the camera.

Build time got much worse. Bevy is a game engine with a large dependency tree,
and the first build takes a long time — which is the entire motivation for
record [0011](0011-optional-gui-feature.md).

The macroquad implementation was removed rather than kept behind a flag. The
comparisons in the design sketch therefore describe code that no longer exists;
they are kept because the current constants, layouts, and colours are largely
ports of it. The last commit that still contains it is `939f503`.
