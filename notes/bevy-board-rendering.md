# Bevy board rendering — design sketch

A record of how the Bevy GUI in `src/bin/gui.rs` was built, and of the
trade-offs behind it. It began as a sketch for porting the board away from the
earlier macroquad GUI, written against Bevy 0.19.1.

That macroquad implementation has since been removed, so the comparisons below
describe code that no longer exists in the repository. They are kept because
they explain why this implementation looks the way it does: the constants,
layouts and colours are largely ports of it. The last revision that still
contains it is commit `939f503`.

```sh
cargo run --bin gui
```

Set `CLC_SCREENSHOT=path.png` to have the app run a few fast turns, exercise a
slider, the food toggle, the edit controls and the chart crosshair, save a
frame, and quit. That is how the UI was checked without a pointer.

## What disappears

| Current | Under Bevy |
| --- | --- |
| `BoardLayout` (17 fields, gui.rs:397) | `Camera2d` + `Transform` |
| `layout_for_size` viewport fitting | camera `scale` set once to fit the board |
| `UiState::zoom` / `UiState::pan` | camera transform |
| `visible_cells` culling | Bevy frustum culling, automatic |
| `BoardLayerCache` + `render_cached_layer` (gui.rs:452) | change-detection on a `BoardRevision` resource |
| `StaticGuiLayers` + `draw_static_gui_layer` (gui.rs:417, 581) | `bevy_ui` nodes — retained by construction |
| `draw_cached_target` / `draw_cached_target_at` | — |
| `smooth_progress` + `animation_elapsed` bookkeeping | a tween component + one system |
| `clip()` / `unclip()` and its `unsafe` scissor call (interface.rs:895) | `ScrollPosition` / UI clipping |

That is the whole reason to consider this. The render-target caching in
`BoardLayerCache` and `StaticGuiLayers` exists to work around macroquad's
per-quad draw-call overhead; Bevy's batched renderer makes the problem go away
rather than making the workaround faster.

## Board geometry

Cells live in world space at a fixed size. Row/col → world position is the only
coordinate math left, and it is two lines instead of `BoardLayout`:

```rust
const CELL: f32 = 32.0;

/// Mirrors `creature_position` in gui.rs:1272 — note the same row→y, col→x
/// swap, and the y negation because Bevy's 2D y-axis points up.
fn cell_to_world(row: usize, col: usize) -> Vec2 {
    Vec2::new(col as f32 * CELL, -(row as f32) * CELL)
}
```

Hit-testing inverts it through the camera, which replaces the current manual
inverse-layout arithmetic:

```rust
fn hovered_cell(
    windows: Query<&Window>,
    camera: Query<(&Camera, &GlobalTransform)>,
    board: Res<BoardRes>,
    mut hovered: ResMut<Hovered>,
) {
    let Ok(window) = windows.single() else { return };
    let Ok((camera, camera_transform)) = camera.single() else { return };

    hovered.0 = window
        .cursor_position()
        .and_then(|cursor| camera.viewport_to_world_2d(camera_transform, cursor).ok())
        .and_then(|world| {
            let col = (world.x / CELL + 0.5).floor();
            let row = (-world.y / CELL + 0.5).floor();
            (col >= 0.0 && row >= 0.0).then_some((row as usize, col as usize))
        })
        .filter(|&(row, col)| row < board.0.rows() && col < board.0.cols());
}
```

Zoom and pan become camera mutations, replacing the `zoom`/`pan` fields and
every place `layout` is recomputed from them:

```rust
fn camera_controls(
    mut camera: Query<(&mut Transform, &mut Projection), With<Camera2d>>,
    scroll: Res<AccumulatedMouseScroll>,
    motion: Res<AccumulatedMouseMotion>,
    mouse: Res<ButtonInput<MouseButton>>,
) {
    let Ok((mut transform, mut projection)) = camera.single_mut() else { return };

    if let Projection::Orthographic(ref mut ortho) = *projection {
        if scroll.delta.y != 0.0 {
            ortho.scale = (ortho.scale * (1.0 - scroll.delta.y * 0.1)).clamp(0.2, 4.0);
        }
        if mouse.pressed(MouseButton::Middle) {
            transform.translation.x -= motion.delta.x * ortho.scale;
            transform.translation.y += motion.delta.y * ortho.scale;
        }
    }
}
```

`AccumulatedMouseScroll` / `AccumulatedMouseMotion` are resources rather than
readers, which is the current recommended way to read mouse input and avoids a
reader entirely.

The three layout tests in interface.rs (`board_fits_viewport_at_multiple_window_and_board_sizes`,
`zoomed_board_keeps_viewport_covered_and_culls_offscreen_cells`) lose their
subject matter — the camera guarantees what they currently assert by hand. That
is a real loss of explicit test coverage in exchange for not owning the code.

## Cell backgrounds — three options

**Current implementation:** one entity per cell using a shared quad and ten
shared `CellMaterial` assets (food levels 0–9). The embedded
`assets/shaders/board_cell.wgsl` draws an antialiased rounded square with a
12% corner radius and a subtle outside border. The fill and rim colours
match Macroquad's food palette; gutters are 5% of the cell width. The border
uses the same signed distance as the fill, so its corners follow the fill
exactly. It stays one physical pixel wide during zoom, capped at a quarter
of the gutter to keep neighbouring cells apart when zoomed out. A Euclidean
screen-space gradient gives straight sides and corners the same one-pixel
antialiasing transition. The quad includes transparent padding so the outside
border and its smoothing are not clipped at the original fill boundary.
The food bars and hover/edit outlines are rounded as well. Cells only switch
material handles on board revisions, and retain their handle when food is
unchanged. The options below describe the original prototype exploration.

`draw_cell_background` (gui.rs:1177) tints each cell by `food / 9.0` and draws a
rounded rect plus rim. Bevy's `Sprite` has **no corner radius** (`BorderRadius`
exists on `bevy_ui` `Node`, not on sprites), so the rounding needs a decision:

**Option A — one entity per cell.** Simplest, most idiomatic, spawns
`rows * cols` entities (10,000 at 100×100 — fine to render, but a slow spawn and
a lot of change-detection traffic).

```rust
#[derive(Component)]
struct Cell { row: usize, col: usize }

fn spawn_cells(mut commands: Commands, board: Res<BoardRes>) {
    for row in 0..board.0.rows() {
        for col in 0..board.0.cols() {
            commands.spawn((
                Cell { row, col },
                Sprite {
                    color: Color::BLACK,
                    custom_size: Some(Vec2::splat(CELL - GAP)),
                    ..default()
                },
                Transform::from_translation(cell_to_world(row, col).extend(0.0)),
            ));
        }
    }
}

/// Replaces `BoardLayerCache::draw` — runs only when the board actually changed,
/// which is the same idea as the existing `board_revision` signature check.
fn recolor_cells(board: Res<BoardRes>, mut cells: Query<(&Cell, &mut Sprite)>) {
    if !board.is_changed() {
        return;
    }
    for (cell, mut sprite) in &mut cells {
        let Some(snapshot) = board.0.cell_snapshot(cell.row, cell.col) else { continue };
        let food = (snapshot.food as f32 / 9.0).clamp(0.0, 1.0);
        sprite.color = EMPTY_CELL.mix(&FED_CELL, food);
    }
}
```

**Option B — one `Mesh2d` for the whole board**, rebuilt with vertex colors when
the revision changes. One entity, one draw call, no per-cell change detection.
More code, but closest in spirit to what `BoardLayerCache` already does.

**Option C — `bevy_ecs_tilemap` 0.19.** Purpose-built for exactly this, handles
chunking and batching. An extra third-party dependency tracking Bevy's release
cadence, which is the thing to weigh.

Rounded corners in all three cases want a small `Material2d` with an SDF rounded
rect, or a pre-made rounded-rect texture used as a tinted sprite. The rim stroke
from `draw_round_rect_lines` folds into the same shader.

## Creatures and animation

The current renderer uses transparent illustrated PNGs in `assets/sprites/`:
a pear-shaped chartreuse aphid and a coral ladybug with a spotted round shell.
The PNGs are embedded in the executable so standalone launches find them too.
Each species shares a texture, with an explicit sprite size that includes its
transparent margin and appendages. Existing movement and birth tweens, cell
slots, crowding scale, and count badges are preserved. The original circle-mesh
experiment described below has been replaced. Art details and generation
prompts live in `assets/sprites/README.md`.

This is where Bevy earns the most. `AnimatedCreature` (gui.rs:35) plus
`animation_elapsed`, `animation_progress`, and `smooth_progress` collapse into a
component and one system:

```rust
#[derive(Component)]
struct CreatureId(usize);

#[derive(Component)]
struct MoveTween {
    from: Vec2,
    to: Vec2,
    from_scale: f32,
    to_scale: f32,
    timer: Timer,
}

fn advance_tweens(
    time: Res<Time>,
    mut commands: Commands,
    mut query: Query<(Entity, &mut Transform, &mut MoveTween)>,
    mut gizmos: Gizmos,
) {
    for (entity, mut transform, mut tween) in &mut query {
        tween.timer.tick(time.delta());
        // Same smoothstep as gui.rs:1280.
        let t = tween.timer.fraction().clamp(0.0, 1.0);
        let progress = t * t * (3.0 - 2.0 * t);

        let position = tween.from.lerp(tween.to, progress);
        transform.translation = position.extend(1.0);
        transform.scale = Vec3::splat(tween.from_scale.lerp(tween.to_scale, progress));

        // The movement trail from gui.rs:1246 — Gizmos is immediate-mode, so
        // this one stays almost verbatim.
        if tween.from != tween.to {
            gizmos.line_2d(tween.from, position, TRAIL_COLOR);
        }
    }
}
```

Reconciling `creature_snapshots()` against live entities each turn replaces the
`previous_creatures` / `current_creatures` diffing: keep a
`HashMap<usize, Entity>` keyed by the stable `CreatureSnapshot::id`, spawn for
new ids, despawn for vanished ones, and retarget `MoveTween` for the rest. The
`cell_slot` field already gives the within-cell offset, so
`CreatureRenderSlot` survives roughly intact.

The catch: `draw_aphid` and `draw_ladybug` (gui.rs:1403, 1436) draw procedurally
with circles and arcs. Under Bevy those become either sprite textures — an
asset-pipeline change, and arguably better-looking — or `Mesh2d` circles, which
is more setup than `draw_circle` for the same result.

## Stepping the simulation

The `accumulator` / `speed` loop becomes a timer-driven system, and this is the
only place `lib.rs` is touched at all:

```rust
#[derive(Resource)]
struct BoardRes(Board);

#[derive(Resource)]
struct Rng(Random);

fn step_simulation(
    time: Res<Time>,
    mut timer: ResMut<StepTimer>,
    mut board: ResMut<BoardRes>,
    mut rng: ResMut<Rng>,
    playing: Res<Playing>,
) {
    if !playing.0 || !timer.0.tick(time.delta()).just_finished() {
        return;
    }
    let stats = board.0.refresh(&mut rng.0);
    // `board` is now flagged as changed, which is what drives `recolor_cells`.
}
```

Note that `ResMut` deref triggers change detection on every access, so the
early return before touching `board` matters — otherwise `recolor_cells`
repaints all 10,000 cells every frame and the main perf argument evaporates.

## Honest accounting

**Genuinely better:** camera replaces all viewport/zoom/pan/culling math; batched
rendering removes the need for both render-target caches; tweening is a system
instead of manual elapsed-time bookkeeping; `Gizmos` covers the trail lines;
wasm and asset hot-reload come free.

**API churn, concretely:** three things in this document had to be corrected
against a real compiler rather than the docs. Bevy 0.19 renamed
`EventReader`/`EventWriter` to `MessageReader`/`MessageWriter`;
`TextFont::font_size` is no longer an `f32` but a `FontSize` enum
(`FontSize::Px(15.0)`); and `WindowResolution::from((u32, u32))` takes
**physical** pixels, so a requested 1120x860 is 560x430 logical on a 2x
display. 0.20.0-rc.1 landed on 2026-09-15, the day before this was written.
That is the maintenance tax, measured.

**What building it actually confirmed:**

- `ScalingMode::AutoMin { min_width, min_height }` fits the board to any window
  size and aspect ratio in one line, which is the whole of what `layout_for_size`
  does by hand. This was the single largest deletion.
- Cells as `Sprite::from_color` entities work and batch; 10,000 of them is not a
  problem to render, though it is a noticeable spawn cost at startup.
- The first prototype used **one shared `Circle` mesh plus one `ColorMaterial`
  per kind** for placeholder creatures. Illustrated sprites now share one image
  per kind; neither approach allocates an asset per creature. Cells now share
  ten materials for their food levels.
- Sprites have no native corner radius. The cells now use an SDF material
  for rounded cells with an outside border following the same contour.
- Camera-based hit-testing via `viewport_to_world_2d` is a real simplification
  over the inverse-layout arithmetic, and it keeps working under zoom and pan
  with no extra code.

**Genuinely worse:** no corner radius on sprites without a custom material;
the layout tests lose
their subject; change detection has a footgun (above) that the current explicit
revision signature does not; and `README.md` currently advertises "Runtime
dependencies are intentionally small" over five crates, which a game engine ends.

## The panel, in bevy_feathers

Also implemented, in the same binary: a tabbed left-hand panel with the nine
probability sliders and live readouts, a fixed playback toolbar, and the turn
and population stats.

Feathers turned out to be further along than its "experimental" label suggests.
The nine sliders plus buttons are about 70 lines, against roughly 400 lines of
hand-rolled widget code in `src/bin/gui/interface.rs` for the same panel.
`FeathersSlider` handles the drag, the fill, the numeric overlay, the theming,
the focus ring and keyboard interaction.

Four things are worth knowing before starting:

**It is BSN, not builder calls.** Feathers 0.19 is built on Bevy's new scene
macro. The panel is a `bsn!` tree with `@FeathersSlider { @min, @max, @value }`
and inline `on(...)` observers. `CommandsSceneExt::spawn_scene` is what lets a
normal system build one with runtime values, which the `fn scene() -> impl
SceneList` form in the official examples does not allow.

**Any component used inside `bsn!` needs `Clone + Default + FromTemplate`.**
A bare marker struct will not compile until it derives all three, and
`UiTargetCamera` has no `Default` at all, so it has to be inserted on the
spawned root afterwards rather than declared in the tree.

**`bevy_ui` lays out relative to its camera's viewport.** Giving the board
camera a viewport that starts to the right of the panel also pushed the panel
right by the same amount. The fix is a second, full-window camera at
`order: 1` with `ClearColorConfig::None` holding the UI, plus
`RenderLayers::layer(1)` on it — without the render layer that second `Camera2d`
draws the entire board a second time, on top of the first.

**One observer can serve every slider.** `ValueChange<f32>` carries
`source: Entity`, so a single `on_prob_changed` reads a `ProbSlider(index)`
component off the source rather than needing nine closures over nine fields.

## The population history chart

The chart now has an adjustable 180–420px height (also constrained to leave at
least 180px for the board) and a 40px collapsed header. Cameras, chart marks,
labels, hover tests and board hit tests share the resolved width and height.
Height buttons and the top-edge drag affordance update the same panel state;
collapse keeps the preferred expanded height and continues recording turns.

History also owns a rolling list of events. Parameter changes compare the new
controls against the board's active rules and coalesce per parameter at the
current turn boundary. Extinction is recorded only on a positive-to-zero
population transition. Diamonds and crosses mark these events; hover summaries
explain their meaning. Events are pruned alongside the 240-turn history and are
included in edit-undo snapshots. A one-point history displays a start prompt.

Also implemented: the population history strip under the board, ported from
`draw_history_graph` in the macroquad GUI. It keeps the same 240-turn window, a
legend, and a crosshair with a tooltip listing both series, and adds direct end
labels plus end-of-line dots.

**Structure.** The chart has its own camera, with a viewport on the strip, and
that camera clears to the chart surface. Lines are gizmos in two
`GizmoConfigGroup`s on a chart-only render layer, since line width is set per
group and gridlines (1px) differ from series (2px). Text, the legend, and the
tooltip are `bevy_ui` nodes on the UI camera. The projection is
`ScalingMode::Fixed` sized so that one world unit equals one logical pixel,
which lets the marks and the text share a single set of layout functions.

**Palette.** The board's aphid and ladybug colours fail a validator's
lightness band on the chart's dark surface. Their deuteranopia separation also
sits in the warning band. The chart therefore uses the same hues, stepped darker
with the hue held. They pass every check against `#1f1f24`: worst deuteranopia
delta E 10.0, normal-vision 28.6. The board keeps its own steps, which are tuned
for its green cells. Feathers' dim text colour measures 4.34:1 on that surface,
just under WCAG's 4.5:1, so axis labels use a slightly lighter gray.

**Four things the chart taught:**

- **Gizmo line width is in physical pixels.** The line shader measures against
  `view.viewport`, so on a 2x display a width of 2.0 draws 1 logical pixel.
  Widths are multiplied by the window scale factor on resize.
- **`GizmoLineStyle::Dashed` restarts its pattern on every segment.** A full
  history gives segments shorter than one dash, so the line renders solid. The
  ladybug dashes are computed on the CPU, carrying the phase across vertices
  like the macroquad version did. A unit test covers that exact case.
- **Gizmos draw all strips after all lines, whatever the call order.** Dashes
  drawn with `line_2d` ended up underneath the aphid `linestrip_2d`. Drawing
  each dash as a two-point strip puts both series in one batch, where call order
  is draw order.
- **The dash is for coincident values, not colour.** Populations are small
  integers, so both lines often sit on the same value for many turns. With both
  lines solid, the one drawn on top hides the other entirely.

**A regression it exposed.** When the panel's UI camera was added,
`camera_controls` and `hovered_cell` were still calling `.single()` on
`Query<_, With<Camera2d>>`. That query now matched two cameras, so zoom, pan,
and cell hover all stopped working without any error. The screenshot probe
never moves the pointer, so it did not notice. Cameras are now selected by
marker component (`BoardCamera`, `ChartCamera`). Zooming, panning, and board
hover also only react while the pointer is over the board. Before, scrolling
the panel zoomed the board and dragging a slider panned it.

## The food overlay

Population is now the default view and uses the neutral cell material. Food
view restores the food palette with quieter fills, bars and speckles; `F`
selects Food and toggles the details. Switching the details off hands the view
back, so the board returns to the neutral shading instead of staying tinted
with nothing drawn on it. A view picked from the two buttons owns itself: the
details then come and go underneath it. Creatures have a shared dark circular
backing that inherits their animated position and scale. Below a 28px cell size,
one mesh of species markers replaces detailed creature sprites: circles and
diamonds, with separate positions in mixed cells. This mesh rebuilds only when
needed by board changes or the detail threshold crossing.

The food overlay is also implemented and ported from the macroquad GUI. When it
is on, each cell gets a gold bar along its bottom sized by its food level (0-9),
plus one speckle per unit of food at fixed positions. The macroquad GUI caps
speckles at six, which makes cells with 6-9 food look the same apart from bar
length. The prototype drops that cap. All nine fit: across a 100x100 board, the
closest two speckle centres are 0.17 of a cell apart against a 0.04 diameter,
and none reaches the bar. It is off by default. The panel's
"food details" checkbox or the `F` key toggles it. A 0-9 food scale for the
always-on cell shading sits beside the checkbox and uses the same colour
function as the cells, so it cannot drift from them.

**One mesh, not entities.** At 100x100, a sprite per bar and per speckle would
be up to 70,000 entities. Instead, the overlay is a single vertex-coloured
`Mesh2d`, rebuilt only when a turn completes or the overlay is switched on, and
not built at all while it is off. That is the same idea as the macroquad
GUI's `BoardLayerCache`. Speckles are hexagons, which are indistinguishable from
circles at that size. The bars have square ends where the macroquad ones were
rounded.

**Cost.** In a release build, one rebuild for a 100x100 board with typical
starting food (about 4.5 per cell) takes about 2.7 ms and yields 352k vertices.
That covers CPU mesh construction only; the GPU upload was not measured. At the
fastest speed setting of 0.05s per turn, that is about 55 ms of CPU per second.

**One source of truth.** The checkbox and the key both write a `FoodOverlay`
resource, and a system copies that resource back onto the checkbox's `Checked`
state. The key and the checkbox therefore stay in agreement. In `bevy_feathers`,
a checkbox does not update its own `Checked` state, so something has to.

**Two 0.19 details.** `Assets::get_mut` returns a guard that must be bound
`mut`. A board with no food at all would produce an empty vertex buffer, so the
mesh falls back to a single zero-area, transparent triangle.

## Cell editing

Cell editing is implemented as a port of the macroquad GUI's edit mode:

- `E` toggles editing. The Edit tab has illustrated Aphid and Ladybug tool
  buttons and an Erase tool; `A` / `L` / `X` select them. Picking a tool starts
  editing and shows a translucent creature preview, or a cross for Erase.
- While editing, a left click adds the selected creature to the hovered cell. A
  right click removes one creature of that kind.
- Hand-placed creatures start with life 10 (aphid) or 15 (ladybug), the same as
  creatures loaded from the config.
- Any edit pauses the run and restarts the turn count and population history
  from the edited board, as `after_board_edit` does.
- Erase clears both species from a cell without changing its food. Undo (`Z`)
  restores the board, RNG, statistics and history from before an edit, paused,
  while keeping the current parameter controls. A deque holds at most 20 exact
  snapshots; stepping or resetting clears it. Done leaves editing paused.
- While editing, the hovered cell shows an outline and a ring in the tool's
  colour.
- **Save setup**, in the Overview tab or with `S`, writes the board and the
  current probabilities to the XDG config file through `save_configured_board`,
  the same function the macroquad GUI's footer button calls. The panel names the
  file it will write, and the toolbar reports the outcome for four seconds.

Population and food totals, species artwork, and per-turn changes now stay above
the board across all tabs. Overview contains births/deaths and food layers, with
Run setup and Controls & shortcuts collapsed by default. Their contents scroll
with the sidebar; the toolbar and tabs stay fixed. The screenshot probe checks
the expanded setup as well as all three tabs and requests a non-resizable window
so tiling window managers cannot silently enlarge its compact 900×640 capture.

**The seed is an entry field.** The seed is a `u64` up to 20 digits, which no
slider or `FeathersNumberInput` can carry, so it is a `FeathersTextInput` with an
`EditableTextFilter` holding it to digits. Applying it sets a `Seed` resource and
writes `RunAction::Reset`, so the restart keeps to the one path the toolbar
already uses rather than growing a second one. A system cannot both read and
write one message stream, so the request to apply a seed is its own `Reseed`
message. Reset reads the seed alongside the starting board and the parameters,
as a `RunSetup` system param: the three things a run is started from.

**A focused text field takes the whole keyboard.** The rule was already there for
Space, so that activating a focused button did not also toggle playback. A text
field needs it for every key: `N`, `Home` and `[` are all things one might type
near a number. Enter, read only while the field has focus, applies the seed.

**A save is the new starting setup.** The macroquad GUI's reset reloads the
config file from disk, so saving there changes what reset restores. Here `Reset`
rebuilds from a `StartingSetup` string captured at launch, so a successful save
replaces that string. One reader of the shared `RunAction` stream handles the
save, which keeps the file writing out of the playback handler and keeps that
handler's signature small.

**Edits are messages.** Pointer input sends a `CellEdit` message, and one
system applies it through `Simulation::edit`. The screenshot probe sends the
same message, so an edit can be exercised in the running app without moving the
real cursor. Tests run `Simulation::edit` against a bare `World`, which covers
the restart rules with no window.

**Click or drag.** A left drag pans the board, so a left press becomes an edit
only if the pointer stays within 4 logical pixels until release. Panning starts
only once the pointer leaves that radius, so a click never nudges the board.

**Play and edit exclude each other in one place.** A single system pauses the
run when editing starts and ends editing when play starts. That holds whichever
control made the change: a key, the panel, or an edit.

**Count badges.** Cells holding more than one creature of a kind carry a count
badge, ported from `draw_count_badge`: a small disc on a dropped shadow with the
count in dark text, aphids upper left and ladybugs lower right, matching the
creature layout. One badge exists per crowded cell and kind, spawned, relabelled
and despawned as the board changes, rather than one per cell up front.

The macroquad GUI hides badges below a cell size of 26 pixels. Here a cell's
size on screen depends on the camera, so the gate is computed from the board
camera's viewport height against the world height its projection shows, and
badges hide when the reader zooms out past that.

The badge red is a step lighter than macroquad's `#d94234`. The count sits
inside the fill, and dark ink on that red measures 4.07:1, under WCAG's 4.5:1
for normal text, with white no better at 4.39:1. Holding hue and chroma and
lifting OKLCH lightness to 0.63 gives `#e44c3d` at 4.60:1. The aphid badge keeps
its colour and measures 7.48:1.

**Sharp badge counts.** `Text2d` builds its glyph atlas from the font size and
the window's scale factor alone (`camera.target_scaling_factor()`), taking no
account of camera zoom. A badge font sized in world units (6.9 units) therefore
rasterised at about 14 physical pixels and was drawn across roughly 32, so the
counts looked pixelated next to the crisp mesh discs beside them. The counts are
now rasterised at the size they are actually drawn and the text entity is scaled
back down, so `font_size * scale` always equals the badge's world height and the
badge keeps its size on the board. The size follows the zoom in steps of 4px,
because every distinct size builds its own font atlas.

**A hidden-creature bug from the first prototype.** Creature slots are numbered
per kind. The prototype gave both kinds one shared layout, so the first aphid
and the first ladybug in a cell were drawn on the same spot and one hid the
other. Editing made this obvious: adding an aphid to a ladybug's cell showed
nothing. Creatures now use the macroquad layout. A cell with one kind centres
its creatures; in a mixed cell, aphids go upper left and ladybugs lower right.
They also use macroquad's radius (0.16 of the cell, down from the 0.22 the
prototype used) and shrink to 0.82 in crowded cells. At 0.22 the creatures were
as large as the edit preview ring and hid it.

**The panel never scrolled.** Bevy 0.19's UI has no built-in mouse-wheel
scrolling, so `Overflow::scroll_y` clipped the panel without scrolling it. With
the edit section added, the content is 1137px tall against a 1041px window, so
the last 96px were unreachable. A small system now scrolls the panel while the
pointer is over it.

**The system tuple limit.** A system tuple holds at most 20 entries, and the
`Update` chain reached 21. It is now an input-and-simulation chain followed by a
rendering chain, in the same order.

The change-detection footgun flagged earlier turned out to be real and was
solved the way the macroquad GUI already solved it: an explicit `Revision`
counter bumped only by a completed turn. Without it, dragging a slider writes
parameters into the board, which flags `BoardRes` as changed, which repaints
all cells and re-targets every creature tween on every frame of the drag.
