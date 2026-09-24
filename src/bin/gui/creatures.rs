//! Creature sprites, their movement tweens and trails, and the count badges on
//! crowded cells.

use crate::board::{BoardDetail, CELL, GAP, cell_to_world};
use crate::layout::BoardCamera;
use crate::simulation::{BoardRes, Revision, STEP_SECONDS};
use bevy::prelude::*;
use creature_life_cycle::{Coordinates, CreatureSnapshot, CreatureSnapshotKind};
use std::collections::HashMap;

/// Nominal creature body radius as a fraction of the inner cell.
pub const CREATURE_RADIUS: f32 = 0.16;
/// Includes the sprites' transparent margin and appendages. The painted
/// silhouette stays inside the edit ring; crowded cells use the usual scale.
pub const CREATURE_SPRITE_SIZE: f32 = (CELL - GAP) * CREATURE_RADIUS * 2.75;
// Count badges, from `draw_count_badge` in the previous macroquad GUI: a small disc on a
// dropped shadow with the count in dark text, sized relative to the cell.
pub const BADGE_RADIUS: f32 = 0.157;
pub const BADGE_FONT: f32 = 0.23;
pub const BADGE_SHADOW_OFFSET: f32 = 0.014;
/// Aphid and ladybug badge corners, matching the mixed-cell creature layout.
pub const BADGE_SLOTS: [Vec2; 2] = [Vec2::new(0.22, 0.22), Vec2::new(0.78, 0.78)];
/// Badge counts are rasterised in steps of this many pixels. Every distinct
/// size builds its own font atlas, so zooming should not mint a new one per
/// frame.
pub const BADGE_RASTER_STEP: f32 = 4.0;
pub const BADGE_RASTER_RANGE: (f32, f32) = (8.0, 96.0);
/// A badge is only worth drawing once the cell is this many logical pixels
/// across, as the `size >= 26.0` gate in the previous macroquad GUI.
pub const BADGE_MIN_CELL_PIXELS: f32 = 26.0;

pub const BADGE_APHID: Color = Color::srgb_u8(93, 188, 85);
// A step lighter than the macroquad badge red (#d94234). The count sits inside
// the fill, and dark ink on that red measures 4.07:1, under WCAG's 4.5:1 for
// normal text, with white no better at 4.39:1. Holding the hue and chroma and
// lifting OKLCH lightness to 0.63 gives 4.60:1.
pub const BADGE_LADYBUG: Color = Color::srgb_u8(228, 76, 61);
/// Dark ink, chosen over white by the fills' luminance: 7.48:1 on the aphid
/// badge and 4.60:1 on the ladybug badge.
const BADGE_TEXT: Color = Color::srgb_u8(18, 25, 20);
pub const BADGE_SHADOW: Color = Color::srgba_u8(0, 0, 0, 115);

/// The streak a creature drags behind it while a move plays. A bright core sits
/// on a wider, fainter glow so the edges fall off instead of ending in a hard
/// rectangle; both ramp to nothing at the tail.
const TRAIL: Color = Color::srgba(0.96, 0.90, 0.69, 0.42);
const TRAIL_GLOW: Color = Color::srgba(0.96, 0.90, 0.69, 0.12);
/// Core and glow width as a fraction of the inner cell, so the streak keeps its
/// proportion to the creature it trails as the camera zooms.
const TRAIL_WIDTH: f32 = 0.15;
const TRAIL_GLOW_WIDTH: f32 = 0.34;
/// Physical-pixel clamp on those widths: under the minimum the streak is back to
/// the hairline it used to be, over the maximum it swamps the creature.
const TRAIL_WIDTH_RANGE: (f32, f32) = (1.5, 22.0);
/// How much of the move passes before the tail starts catching up. The streak
/// grows, holds its length, then collapses into the creature as it settles,
/// rather than popping out of existence when the tween ends.
pub const TRAIL_LAG: f32 = 0.64;
/// Points along the streak: enough for the alpha ramp to read as a gradient
/// rather than a staircase, few enough to stay cheap per creature.
pub const TRAIL_POINTS: usize = 8;

// The movement streak is drawn twice, as a core over a glow, and gizmo line
// width is set per group, so each half needs its own group.
#[derive(Default, Reflect, GizmoConfigGroup)]
pub struct TrailGizmos;

#[derive(Default, Reflect, GizmoConfigGroup)]
pub struct TrailGlowGizmos;

/// The size badge counts are currently rasterised at, and the entity scale that
/// puts that raster back at the right world size.
#[derive(Resource, Clone, Copy)]
pub struct BadgeRaster {
    pub font_size: f32,
    pub scale: f32,
}

impl Default for BadgeRaster {
    fn default() -> Self {
        let (font_size, scale) = badge_text_raster(1.0);
        Self { font_size, scale }
    }
}

/// Badges currently on the board, keyed by cell and creature kind.
#[derive(Resource, Default)]
pub struct BadgeIndex(pub HashMap<(usize, usize, usize), Badge>);

#[derive(Clone, Copy)]
pub struct Badge {
    pub root: Entity,
    pub text: Entity,
}

#[derive(Component)]
pub struct CountBadge;

/// Maps `CreatureSnapshot::id` to the entity currently rendering it, which is
/// what replaces the `previous_creatures` / `current_creatures` diffing in the
/// macroquad GUI.
#[derive(Resource, Default)]
pub struct CreatureIndex(pub HashMap<usize, Entity>);

#[derive(Component)]
pub struct Creature;

#[derive(Component)]
pub struct MoveTween {
    pub from: Vec2,
    pub to: Vec2,
    pub from_scale: f32,
    pub to_scale: f32,
    pub timer: Timer,
}

impl MoveTween {
    fn still(at: Vec2, scale: f32) -> Self {
        Self {
            from: at,
            to: at,
            from_scale: scale,
            to_scale: scale,
            timer: Timer::from_seconds(STEP_SECONDS, TimerMode::Once),
        }
    }

    fn born(at: Vec2, scale: f32) -> Self {
        Self {
            from: at,
            to: at,
            from_scale: 0.0,
            to_scale: scale,
            timer: Timer::from_seconds(STEP_SECONDS, TimerMode::Once),
        }
    }
}

/// Where a creature sits in its cell and how large it is drawn, ported from
/// `creature_slots` and `creature_render_scale` in the previous macroquad GUI. The offset
/// is a fraction of the cell from its top-left corner (x right, y down).
///
/// `cell_slot` counts per kind, so a mixed cell needs separate layouts: with one
/// shared layout the first aphid and the first ladybug land on the same spot and
/// one hides the other. Aphids take the upper left and ladybugs the lower right.
pub fn creature_slot(
    kind: CreatureSnapshotKind,
    cell_slot: usize,
    aphids: usize,
    ladybugs: usize,
) -> (Vec2, f32) {
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

    let slots = if aphids == 0 || ladybugs == 0 {
        &CENTER
    } else if kind == CreatureSnapshotKind::Aphid {
        &APHIDS
    } else {
        &LADYBUGS
    };
    let (x, y) = slots[cell_slot.min(slots.len() - 1)];
    let scale = if aphids + ladybugs > 5 { 0.82 } else { 1.0 };
    (Vec2::new(x, y), scale)
}

pub fn creature_world(location: Coordinates, offset: Vec2) -> Vec2 {
    // `Coordinates.x` is the row and `.y` is the column, matching
    // `creature_position` in the previous macroquad GUI.
    let inner = CELL - GAP;
    let top_left = cell_to_world(location.x, location.y) + Vec2::new(-inner * 0.5, inner * 0.5);
    top_left + Vec2::new(inner * offset.x, -inner * offset.y)
}

#[derive(Resource)]
pub struct CreatureAssets {
    pub aphid: Handle<Image>,
    pub ladybug: Handle<Image>,
    pub badge: Handle<Mesh>,
    pub badge_shadow_mesh: Handle<Mesh>,
    pub badge_colours: [Handle<ColorMaterial>; 2],
    pub badge_shadow: Handle<ColorMaterial>,
    pub font: Handle<Font>,
}

/// Reconciles live creature entities against `Board::creature_snapshots`,
/// keyed by the stable snapshot id: spawn the new, despawn the gone, retarget
/// the rest. This is the counterpart to building `AnimatedCreature` values.
pub fn sync_creatures(
    mut commands: Commands,
    board: Res<BoardRes>,
    revision: Res<Revision>,
    assets: Res<CreatureAssets>,
    mut index: ResMut<CreatureIndex>,
    mut tweens: Query<(&mut MoveTween, &Transform, &mut Sprite), With<Creature>>,
) {
    if !revision.is_changed() {
        return;
    }

    let snapshots = board.0.creature_snapshots();
    let mut seen = HashMap::with_capacity(snapshots.len());

    for snapshot in &snapshots {
        let location = snapshot.location;
        let (aphids, ladybugs) = board
            .0
            .cell_snapshot(location.x, location.y)
            .map_or((0, 0), |cell| (cell.aphids, cell.ladybugs));
        let (offset, scale) = creature_slot(snapshot.kind, snapshot.cell_slot, aphids, ladybugs);
        let target = creature_world(location, offset);
        let image = match snapshot.kind {
            CreatureSnapshotKind::Aphid => assets.aphid.clone(),
            CreatureSnapshotKind::Ladybug => assets.ladybug.clone(),
        };

        match index.0.get(&snapshot.id) {
            Some(&entity) => {
                if let Ok((mut tween, transform, mut sprite)) = tweens.get_mut(entity) {
                    // Undo followed by a new edit can reuse an ID for another species.
                    if sprite.image != image {
                        sprite.image = image;
                    }
                    *tween = MoveTween {
                        from: transform.translation.truncate(),
                        to: target,
                        from_scale: transform.scale.x.max(0.001),
                        to_scale: scale,
                        timer: Timer::from_seconds(STEP_SECONDS, TimerMode::Once),
                    };
                }
                seen.insert(snapshot.id, entity);
            }
            None => {
                let entity = commands
                    .spawn((
                        Creature,
                        Sprite {
                            image,
                            custom_size: Some(Vec2::splat(CREATURE_SPRITE_SIZE)),
                            ..default()
                        },
                        Transform::from_translation(target.extend(1.0))
                            .with_scale(Vec3::splat(0.001)),
                        if index.0.is_empty() {
                            MoveTween::still(target, scale)
                        } else {
                            MoveTween::born(target, scale)
                        },
                    ))
                    .id();
                seen.insert(snapshot.id, entity);
            }
        }
    }

    for (id, entity) in index.0.iter() {
        if !seen.contains_key(id) {
            commands.entity(*entity).despawn();
        }
    }

    index.0 = seen;
}

/// Cells holding more than one creature of a kind, as `(location, kind index,
/// count)`. Kind index 0 is aphids and 1 ladybugs, matching `BADGE_SLOTS`.
pub fn crowded_cells(snapshots: &[CreatureSnapshot]) -> Vec<(Coordinates, usize, usize)> {
    let mut counts: HashMap<(Coordinates, usize), usize> = HashMap::new();
    for snapshot in snapshots {
        let kind = usize::from(snapshot.kind == CreatureSnapshotKind::Ladybug);
        *counts.entry((snapshot.location, kind)).or_default() += 1;
    }
    let mut crowded: Vec<(Coordinates, usize, usize)> = counts
        .into_iter()
        .filter(|&(_, count)| count > 1)
        .map(|((location, kind), count)| (location, kind, count))
        .collect();
    // A stable order keeps spawning deterministic, which the tests rely on.
    crowded.sort_by_key(|&(location, kind, _)| (location.x, location.y, kind));
    crowded
}

/// Keeps one badge per crowded cell and kind, spawning, relabelling and
/// despawning as the board changes.
pub fn sync_count_badges(
    mut commands: Commands,
    board: Res<BoardRes>,
    revision: Res<Revision>,
    assets: Res<CreatureAssets>,
    mut badges: ResMut<BadgeIndex>,
    raster: Res<BadgeRaster>,
    mut labels: Query<&mut Text2d, With<CountBadge>>,
) {
    if !revision.is_changed() {
        return;
    }

    let inner = CELL - GAP;
    let mut seen = HashMap::new();
    for (location, kind, count) in crowded_cells(&board.0.creature_snapshots()) {
        let key = (location.x, location.y, kind);
        let label = count.to_string();

        let badge = match badges.0.get(&key) {
            Some(&badge) => {
                if let Ok(mut text) = labels.get_mut(badge.text)
                    && text.0 != label
                {
                    text.0 = label;
                }
                badge
            }
            None => {
                // Above the creatures, which sit at z 1.
                let position = creature_world(location, BADGE_SLOTS[kind]).extend(2.0);
                let offset = inner * BADGE_SHADOW_OFFSET;
                let root = commands
                    .spawn((
                        CountBadge,
                        Mesh2d(assets.badge.clone()),
                        MeshMaterial2d(assets.badge_colours[kind].clone()),
                        Transform::from_translation(position),
                    ))
                    .id();
                let text = commands
                    .spawn((
                        CountBadge,
                        Text2d::new(label),
                        TextFont {
                            font: assets.font.clone().into(),
                            font_size: FontSize::Px(raster.font_size),
                            ..default()
                        },
                        TextColor(BADGE_TEXT),
                        Transform::from_xyz(0.0, 0.0, 0.02).with_scale(Vec3::splat(raster.scale)),
                        ChildOf(root),
                    ))
                    .id();
                commands.spawn((
                    CountBadge,
                    Mesh2d(assets.badge_shadow_mesh.clone()),
                    MeshMaterial2d(assets.badge_shadow.clone()),
                    Transform::from_xyz(offset, -offset, -0.01),
                    ChildOf(root),
                ));
                Badge { root, text }
            }
        };
        seen.insert(key, badge);
    }

    for (key, badge) in badges.0.iter() {
        if !seen.contains_key(key) {
            commands.entity(badge.root).despawn();
        }
    }
    badges.0 = seen;
}

/// Badges are only legible once a cell covers enough of the screen, so they
/// follow the camera's zoom rather than being drawn at every scale.
pub fn scale_count_badges(
    camera: Query<(&Camera, &Projection), With<BoardCamera>>,
    badges: Res<BadgeIndex>,
    mut raster: ResMut<BadgeRaster>,
    mut visibility: Query<&mut Visibility, With<CountBadge>>,
    mut labels: BadgeLabels,
) {
    let Ok((camera, Projection::Orthographic(ortho))) = camera.single() else {
        return;
    };
    let viewport_height = camera.logical_viewport_size().map(|viewport| viewport.y);
    let legible = viewport_height.is_some_and(|height| badges_legible(height, ortho.area.height()));

    // Rasterise the counts at the size they are actually drawn. `Text2d` builds
    // its glyph atlas from the font size and the window's scale factor alone,
    // ignoring camera zoom, so a font size in world units is upscaled on screen
    // and looks pixelated. The entity is scaled back down to keep the badge the
    // same size on the board.
    if let Some(height) = viewport_height
        && ortho.area.height() > 0.0
    {
        let (font_size, scale) = badge_text_raster(height / ortho.area.height());
        if font_size != raster.font_size {
            *raster = BadgeRaster { font_size, scale };
            for (mut font, mut transform) in &mut labels {
                font.font_size = FontSize::Px(font_size);
                transform.scale = Vec3::splat(scale);
            }
        }
    }

    let wanted = if legible {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    };
    for badge in badges.0.values() {
        if let Ok(mut visibility) = visibility.get_mut(badge.root) {
            visibility.set_if_neq(wanted);
        }
    }
}

/// The badge count labels, whose raster size follows the zoom.
pub type BadgeLabels<'w, 's> = Query<
    'w,
    's,
    (&'static mut TextFont, &'static mut Transform),
    (With<CountBadge>, With<Text2d>),
>;

/// The font size to rasterise a badge count at, with the entity scale that keeps
/// its world size unchanged, for a board drawn at `pixels_per_unit` logical
/// pixels per world unit. `font_size * scale` is always the badge's world height.
pub fn badge_text_raster(pixels_per_unit: f32) -> (f32, f32) {
    let height = (CELL - GAP) * BADGE_FONT;
    let (min, max) = BADGE_RASTER_RANGE;
    let wanted = height * pixels_per_unit.max(0.0);
    let font_size = ((wanted / BADGE_RASTER_STEP).round() * BADGE_RASTER_STEP).clamp(min, max);
    (font_size, height / font_size)
}

/// Whether a cell is wide enough on screen for a badge to be readable, given the
/// camera's viewport height and the world height it shows.
pub fn badges_legible(viewport_height: f32, area_height: f32) -> bool {
    if area_height <= 0.0 {
        return false;
    }
    (CELL - GAP) * (viewport_height / area_height) >= BADGE_MIN_CELL_PIXELS
}

/// Replaces `draw_animated_creatures`, `animation_elapsed`, `animation_progress`
/// and `smooth_progress` with one system over a component.
pub fn advance_tweens(
    time: Res<Time>,
    detail: Res<BoardDetail>,
    mut creatures: Query<(&mut Transform, &mut MoveTween)>,
    mut trail: Gizmos<TrailGizmos>,
    mut glow: Gizmos<TrailGlowGizmos>,
) {
    for (mut transform, mut tween) in &mut creatures {
        tween.timer.tick(time.delta());

        let t = tween.timer.fraction().clamp(0.0, 1.0);
        let progress = t * t * (3.0 - 2.0 * t); // same smoothstep as gui.rs:1280

        let position = tween.from.lerp(tween.to, progress);
        transform.translation = position.extend(1.0);
        let scale = tween.from_scale + (tween.to_scale - tween.from_scale) * progress;
        transform.scale = Vec3::splat(scale.max(0.001));

        // Movement trail, carried over from `draw_animated_creatures`. Gizmos
        // is immediate-mode, so this stays a per-frame draw.
        if !detail.simplified && tween.from.distance_squared(tween.to) > 0.01 {
            let ramp = |base: Color| {
                trail_points(tween.from, position, t)
                    .map(move |(point, alpha)| (point, base.with_alpha(base.alpha() * alpha)))
            };
            glow.linestrip_gradient_2d(ramp(TRAIL_GLOW));
            trail.linestrip_gradient_2d(ramp(TRAIL));
        }
    }
}

/// The streak behind a creature that is `t` through its move, from the tail up
/// to the creature itself, each point paired with the fraction of the streak's
/// alpha it carries. The ramp is quadratic, so the streak is a faint wash that
/// gathers into a bright head rather than a slab of even colour.
pub fn trail_points(from: Vec2, head: Vec2, t: f32) -> impl Iterator<Item = (Vec2, f32)> {
    // The tail stays put until `TRAIL_LAG` of the move has passed, then closes
    // on the head, reaching it exactly as the move ends.
    let lag = ((t - TRAIL_LAG) / (1.0 - TRAIL_LAG)).clamp(0.0, 1.0);
    let tail = from.lerp(head, lag * lag * (3.0 - 2.0 * lag));
    (0..TRAIL_POINTS).map(move |i| {
        let along = i as f32 / (TRAIL_POINTS - 1) as f32;
        (tail.lerp(head, along), along * along)
    })
}

/// Keeps the streak's weight proportional to the creatures. Gizmo line width is
/// in physical pixels and takes no notice of the camera, so both widths are
/// recomputed from the board camera's zoom instead of being set once.
pub fn scale_trail_gizmos(
    cameras: Query<(&Camera, &Projection), With<BoardCamera>>,
    mut gizmo_config: ResMut<GizmoConfigStore>,
) {
    let Ok((camera, Projection::Orthographic(ortho))) = cameras.single() else {
        return;
    };
    let Some(viewport) = camera.physical_viewport_size() else {
        return;
    };
    if ortho.area.height() <= 0.0 {
        return;
    }
    let pixels_per_unit = viewport.y as f32 / ortho.area.height();
    let width = |fraction: f32| {
        let (min, max) = TRAIL_WIDTH_RANGE;
        ((CELL - GAP) * fraction * pixels_per_unit).clamp(min, max)
    };

    // Written only on a change: `config_mut` marks the whole store dirty.
    let (core, halo) = (width(TRAIL_WIDTH), width(TRAIL_GLOW_WIDTH));
    if gizmo_config.config::<TrailGizmos>().0.line.width != core {
        gizmo_config.config_mut::<TrailGizmos>().0.line.width = core;
    }
    if gizmo_config.config::<TrailGlowGizmos>().0.line.width != halo {
        gizmo_config.config_mut::<TrailGlowGizmos>().0.line.width = halo;
    }
}
