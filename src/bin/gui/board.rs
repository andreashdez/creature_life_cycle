//! The board's cells: the cell shader, food shading and the food overlay, the
//! distant-zoom population markers, and the board view that picks between them.

use crate::creatures::{BADGE_APHID, BADGE_LADYBUG, Creature};
use crate::layout::BoardCamera;
use crate::simulation::{BoardRes, Revision};
use bevy::asset::{RenderAssetUsages, embedded_asset};
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use bevy::render::render_resource::AsBindGroup;
use bevy::shader::ShaderRef;
use bevy::sprite_render::{AlphaMode2d, Material2d};
use creature_life_cycle::Board;

/// World-space size of one board cell, including the gap.
pub const CELL: f32 = 32.0;

/// Macroquad's five-percent gutter and twelve-percent corner radius.
pub const GAP: f32 = CELL * 0.05;
pub const CELL_CORNER: f32 = 0.12;

// Colours lifted from `draw_cell_background` and the palette in interface.rs.
const EMPTY_CELL: Color = Color::srgb_u8(31, 33, 34);
const FED_CELL: Color = Color::srgb_u8(53, 70, 48);
pub const EMPTY_CELL_RIM: Color = Color::srgba_u8(48, 53, 50, 90);
pub const FED_CELL_RIM: Color = Color::srgba_u8(135, 153, 89, 65);

/// Food cap per cell, mirroring `MAX_CELL_FOOD` in `src/lib.rs`.
pub const MAX_FOOD: i32 = 9;
// Food overlay marks, lifted from `draw_cell_background` and
// `draw_food_speckles` in the previous macroquad GUI.
const FOOD_BAR: Color = Color::srgba_u8(194, 184, 83, 85);
const FOOD_SPECKLE: Color = Color::srgba_u8(219, 205, 116, 42);

const DETAIL_MIN_CELL_PIXELS: f32 = 28.0;

/// Whether the per-cell food bars and speckles are drawn. The panel checkbox
/// and the `F` key both write this, and the checkbox is synced from it, so the
/// two can never disagree.
#[derive(Resource, Default)]
pub struct FoodOverlay {
    pub on: bool,
    /// Whether switching the details on is what selected the food view, so
    /// that switching them off can hand the view back.
    pub selected_view: bool,
}

#[derive(Resource, Default, Clone, Copy, PartialEq, Eq, Debug)]
pub enum BoardView {
    #[default]
    Population,
    Food,
}

#[derive(Resource, Default)]
pub struct BoardDetail {
    pub simplified: bool,
}

#[derive(Component)]
pub struct PopulationLayer;

/// The single mesh holding every cell's food bar and speckles.
#[derive(Component)]
pub struct FoodLayer;

#[derive(Component)]
pub struct Cell {
    pub row: usize,
    pub col: usize,
}

/// Board coordinates are (row, col); Bevy's 2D y-axis points up, so rows count
/// downwards. This is the whole of the layout math that `BoardLayout` replaced.
pub fn cell_to_world(row: usize, col: usize) -> Vec2 {
    Vec2::new(col as f32 * CELL, -(row as f32) * CELL)
}

/// Bundle board art and shaders so standalone launches find their assets.
pub fn board_render_assets(app: &mut App) {
    embedded_asset!(app, "../../../assets/shaders/board_cell.wgsl");
    embedded_asset!(app, "../../../assets/sprites/aphid.png");
    embedded_asset!(app, "../../../assets/sprites/ladybug.png");
    embedded_asset!(app, "../../../assets/sprites/leaf.png");
}

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct CellMaterial {
    #[uniform(0)]
    pub fill: LinearRgba,
    #[uniform(0)]
    pub rim: LinearRgba,
    #[uniform(0)]
    pub shape: Vec4,
}

impl Material2d for CellMaterial {
    fn fragment_shader() -> ShaderRef {
        bevy::asset::AssetPath::from_path_buf(bevy::asset::embedded_path!(
            "../../../assets/shaders/board_cell.wgsl"
        ))
        .with_source("embedded")
        .into()
    }

    fn alpha_mode(&self) -> AlphaMode2d {
        AlphaMode2d::Blend
    }
}

#[derive(Resource)]
pub struct CellPalette(pub [Handle<CellMaterial>; MAX_FOOD as usize + 1]);

/// The food details only say anything over the food view's shading, so
/// switching them on selects that view. Switching them off then hands the view
/// back, rather than leaving the board tinted with no details on it. A view
/// picked from the buttons owns itself: the details come and go under it.
pub fn set_food_details(on: bool, overlay: &mut FoodOverlay, view: &mut BoardView) {
    overlay.on = on;
    if on {
        overlay.selected_view = *view != BoardView::Food;
        if *view != BoardView::Food {
            *view = BoardView::Food;
        }
    } else if std::mem::take(&mut overlay.selected_view) {
        *view = BoardView::Population;
    }
}

/// One marker per occupied cell and species replaces illegible distant sprites.
pub fn population_mesh(board: &Board) -> Mesh {
    let mut mesh = OverlayMesh::default();
    for row in 0..board.rows() {
        for col in 0..board.cols() {
            let Some(cell) = board.cell_snapshot(row, col) else {
                continue;
            };
            let mixed = cell.aphids > 0 && cell.ladybugs > 0;
            let centre = cell_to_world(row, col);
            for (index, count) in [cell.aphids, cell.ladybugs].into_iter().enumerate() {
                if count == 0 {
                    continue;
                }
                let offset = if mixed {
                    Vec2::new(-1.0, 1.0) * CELL * 0.2 * if index == 0 { 1.0 } else { -1.0 }
                } else {
                    Vec2::ZERO
                };
                let radius = CELL * if mixed { 0.18 } else { 0.27 };
                let at = centre + offset;
                mesh.disc(at, radius + 1.4, Color::srgb_u8(10, 16, 17));
                if index == 0 {
                    mesh.disc(at, radius, BADGE_APHID);
                } else {
                    mesh.diamond(at, radius, BADGE_LADYBUG);
                }
            }
        }
    }
    mesh.into_mesh()
}

pub fn update_board_detail(
    cameras: Query<(&Camera, &Projection), With<BoardCamera>>,
    board: Res<BoardRes>,
    revision: Res<Revision>,
    mut detail: ResMut<BoardDetail>,
    mut creatures: Query<&mut Visibility, (With<Creature>, Without<PopulationLayer>)>,
    mut layer: Query<(&Mesh2d, &mut Visibility), With<PopulationLayer>>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    let Ok((camera, Projection::Orthographic(ortho))) = cameras.single() else {
        return;
    };
    let Some(viewport) = camera.logical_viewport_size() else {
        return;
    };
    let simplified = (CELL - GAP) * viewport.y / ortho.area.height() < DETAIL_MIN_CELL_PIXELS;
    let changed = simplified != detail.simplified;
    if changed {
        detail.simplified = simplified;
    }
    if !(changed || revision.is_changed()) {
        return;
    }
    for mut visibility in &mut creatures {
        *visibility = if simplified {
            Visibility::Hidden
        } else {
            Visibility::Inherited
        };
    }
    if let Ok((handle, mut visibility)) = layer.single_mut() {
        *visibility = if simplified {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        if simplified && let Some(mut mesh) = meshes.get_mut(&handle.0) {
            *mesh = population_mesh(&board.0);
        }
    }
}

pub fn recolor_cells(
    board: Res<BoardRes>,
    revision: Res<Revision>,
    palette: Res<CellPalette>,
    view: Res<BoardView>,
    mut cells: Query<(&Cell, &mut MeshMaterial2d<CellMaterial>)>,
) {
    if !(revision.is_changed() || view.is_changed()) {
        return;
    }

    for (cell, mut material) in &mut cells {
        let Some(snapshot) = board.0.cell_snapshot(cell.row, cell.col) else {
            continue;
        };
        let level = if *view == BoardView::Food {
            snapshot.food.clamp(0, MAX_FOOD) as usize
        } else {
            0
        };
        let next = &palette.0[level];
        if material.0 != *next {
            material.0 = next.clone();
        }
    }
}

/// Cell shading by food. Shared with the panel's food scale so the legend is
/// the same colours as the board by construction.
pub fn food_colour(food: i32) -> Color {
    let t = (food as f32 / MAX_FOOD as f32).clamp(0.0, 1.0);
    EMPTY_CELL.mix(&FED_CELL, t)
}

/// Collects the overlay's triangles. Bars and speckles are appended in draw
/// order, so speckles land on top of bars as in the previous macroquad GUI.
#[derive(Default)]
pub struct OverlayMesh {
    pub positions: Vec<[f32; 3]>,
    pub colours: Vec<[f32; 4]>,
    pub indices: Vec<u32>,
}

impl OverlayMesh {
    fn diamond(&mut self, centre: Vec2, radius: f32, colour: Color) {
        let base = self.positions.len() as u32;
        for offset in [Vec2::Y, Vec2::X, Vec2::NEG_Y, Vec2::NEG_X] {
            let point = centre + offset * radius;
            self.positions.push([point.x, point.y, 0.0]);
            self.colours.push(colour.to_linear().to_f32_array());
        }
        self.indices
            .extend([base, base + 1, base + 2, base, base + 2, base + 3]);
    }

    /// A single triangle fan avoids dark seams from overlapping translucent
    /// rectangles and end caps. The radius is clamped for very short bars.
    fn rounded_rect(&mut self, min: Vec2, max: Vec2, radius: f32, colour: Color) {
        let base = self.positions.len() as u32;
        let colour = colour.to_linear().to_f32_array();
        let radius = radius.min((max.x - min.x) * 0.5).min((max.y - min.y) * 0.5);
        let centre = (min + max) * 0.5;
        self.positions.push([centre.x, centre.y, 0.0]);
        self.colours.push(colour);
        for (corner, start) in [
            (Vec2::new(max.x - radius, max.y - radius), 0.0),
            (Vec2::new(min.x + radius, max.y - radius), 1.0),
            (Vec2::new(min.x + radius, min.y + radius), 2.0),
            (Vec2::new(max.x - radius, min.y + radius), 3.0),
        ] {
            for step in 0..=6 {
                let angle = (start + step as f32 / 6.0) * std::f32::consts::FRAC_PI_2;
                let point = corner + Vec2::new(angle.cos(), angle.sin()) * radius;
                self.positions.push([point.x, point.y, 0.0]);
                self.colours.push(colour);
            }
        }
        for step in 0..28 {
            self.indices
                .extend([base, base + 1 + step, base + 1 + (step + 1) % 28]);
        }
    }

    /// A hexagon: at speckle size it is indistinguishable from a circle, at a
    /// fraction of the triangles.
    fn disc(&mut self, centre: Vec2, radius: f32, colour: Color) {
        let base = self.positions.len() as u32;
        let colour = colour.to_linear().to_f32_array();
        self.positions.push([centre.x, centre.y, 0.0]);
        self.colours.push(colour);
        for step in 0..6 {
            let angle = step as f32 * std::f32::consts::TAU / 6.0;
            self.positions.push([
                centre.x + radius * angle.cos(),
                centre.y + radius * angle.sin(),
                0.0,
            ]);
            self.colours.push(colour);
        }
        for step in 0..6 {
            self.indices
                .extend([base, base + 1 + step, base + 1 + (step + 1) % 6]);
        }
    }

    fn into_mesh(mut self) -> Mesh {
        // A board with no food would otherwise produce an empty vertex buffer.
        // One zero-area, fully transparent triangle keeps the mesh valid.
        if self.indices.is_empty() {
            self.positions.extend([[0.0; 3]; 3]);
            self.colours.extend([[0.0; 4]; 3]);
            self.indices.extend([0, 1, 2]);
        }
        Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::default(),
        )
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, self.positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, self.colours)
        .with_inserted_indices(Indices::U32(self.indices))
    }
}

/// Builds every cell's food bar and speckles into one mesh, using the
/// macroquad GUI's proportions (`draw_cell_background`, `draw_food_speckles`)
/// converted from a y-down cell rectangle to y-up world space.
pub fn food_overlay_mesh(board: &Board) -> Mesh {
    let inner = CELL - GAP;
    let mut mesh = OverlayMesh::default();

    for row in 0..board.rows() {
        for col in 0..board.cols() {
            let Some(snapshot) = board.cell_snapshot(row, col) else {
                continue;
            };
            let food = snapshot.food.clamp(0, MAX_FOOD);
            if food == 0 {
                continue;
            }
            // Top-left corner of the cell's inner square.
            let origin = cell_to_world(row, col) + Vec2::new(-inner * 0.5, inner * 0.5);

            // Bar along the bottom, its length the food level.
            let left = origin.x + inner * 0.14;
            let top = origin.y - inner * 0.82;
            let width = inner * 0.72 * food as f32 / MAX_FOOD as f32;
            let height = inner * 0.055;
            mesh.rounded_rect(
                Vec2::new(left, top - height),
                Vec2::new(left + width, top),
                height * 0.5,
                FOOD_BAR,
            );

            for offset in speckle_offsets(row, col, food) {
                let centre = origin + Vec2::new(inner * offset.x, -inner * offset.y);
                mesh.disc(centre, inner * 0.02, FOOD_SPECKLE);
            }
        }
    }

    mesh.into_mesh()
}

/// Speckle positions for a cell, one per unit of food, as fractions of the cell
/// measured from its top-left corner (x right, y down). Positions are fixed per
/// cell, so speckles stay put as food changes rather than jittering every turn.
///
/// The macroquad GUI capped this at six, which made cells holding 6, 7, 8 and 9
/// food look identical apart from bar length. There is room for all nine: across
/// a 100x100 board the closest two centres are 0.17 of a cell apart against a
/// speckle diameter of 0.04, and none comes within reach of the bar.
pub fn speckle_offsets(row: usize, col: usize, food: i32) -> impl Iterator<Item = Vec2> {
    (0..food.clamp(0, MAX_FOOD) as usize).map(move |dot| {
        let seed = row * 73 + col * 41 + dot * 19;
        Vec2::new(
            0.18 + (seed % 59) as f32 / 92.0,
            0.18 + (seed % 43) as f32 / 78.0,
        )
    })
}

/// Rebuilds the overlay after a turn, or when it is switched on. While it is
/// off nothing is built at all.
pub fn rebuild_food_overlay(
    board: Res<BoardRes>,
    revision: Res<Revision>,
    overlay: Res<FoodOverlay>,
    view: Res<BoardView>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut layer: Query<(&Mesh2d, &mut Visibility), With<FoodLayer>>,
) {
    if !(revision.is_changed() || overlay.is_changed() || view.is_changed()) {
        return;
    }
    let Ok((handle, mut visibility)) = layer.single_mut() else {
        return;
    };

    if !overlay.on || *view != BoardView::Food {
        visibility.set_if_neq(Visibility::Hidden);
        return;
    }
    if let Some(mut mesh) = meshes.get_mut(&handle.0) {
        *mesh = food_overlay_mesh(&board.0);
    }
    visibility.set_if_neq(Visibility::Inherited);
}
