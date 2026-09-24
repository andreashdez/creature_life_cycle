//! Building the world at launch: the board from the shared config, the three
//! cameras, the cells, and every panel.

use crate::board::{
    CELL, CELL_CORNER, Cell, CellMaterial, CellPalette, EMPTY_CELL_RIM, FED_CELL_RIM, FoodLayer,
    GAP, MAX_FOOD, PopulationLayer, cell_to_world, food_colour, food_overlay_mesh, population_mesh,
};
use crate::chart::{CHART_SERIES, CHART_SURFACE, EndDot, spawn_chart_overlay};
use crate::creatures::{
    BADGE_APHID, BADGE_LADYBUG, BADGE_RADIUS, BADGE_SHADOW, BADGE_SHADOW_OFFSET,
    CREATURE_SPRITE_SIZE, CreatureAssets,
};
use crate::editing::EditPreview;
use crate::history::History;
use crate::hover::spawn_cell_popup;
use crate::layout::{BoardCamera, ChartCamera};
use crate::panel::spawn_panel;
use crate::params::{Params, SavedParams};
use crate::probe::ScreenshotProbe;
use crate::run_control::StatusLine;
use crate::simulation::{
    BoardRes, DEFAULT_SEED, Rng, STEP_SECONDS, StartingSetup, Stats, StepTimer,
};
use crate::summary::spawn_summary;
use bevy::asset::load_embedded_asset;
use bevy::camera::ScalingMode;
use bevy::camera::visibility::RenderLayers;
use bevy::feathers::constants::fonts;
use bevy::image::{ImageLoaderSettings, ImageSampler};
use bevy::prelude::*;
use creature_life_cycle::{Board, Random, load_configured_board};

pub fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    mut cell_materials: ResMut<Assets<CellMaterial>>,
    asset_server: Res<AssetServer>,
    mut status: ResMut<StatusLine>,
) {
    let mut random = Random::with_seed(DEFAULT_SEED);
    let board = match load_configured_board(&mut random) {
        Ok(loaded) => {
            for notice in &loaded.notices {
                eprintln!("{notice}");
            }
            loaded.board
        }
        Err(error) => {
            // A broken config should not keep the window from opening, so the
            // GUI shows the standard board and says so where the reader will
            // see it. Saving later moves the broken file aside instead of
            // overwriting it.
            eprintln!("Error: {error}; showing the standard board.");
            status.pin("Config file is invalid; showing the defaults.", false);
            Board::standard(&mut random)
        }
    };
    let (rows, cols) = (board.rows(), board.cols());
    let summary = board.summary();
    commands.insert_resource(StartingSetup(
        creature_life_cycle::format_simulation_config(&board),
    ));

    // Centre the camera on the board and scale so the whole thing fits, which
    // is the job `layout_for_size` does by hand in the previous macroquad GUI.
    let board_size = Vec2::new(cols as f32 * CELL, rows as f32 * CELL);
    let centre = Vec2::new(
        board_size.x * 0.5 - CELL * 0.5,
        -board_size.y * 0.5 + CELL * 0.5,
    );
    // `AutoMin` keeps the whole board visible whatever the window size and
    // aspect ratio are, preserving the aspect ratio. This is the single line
    // that replaces `layout_for_size` and its fit/clamp arithmetic.
    let mut projection = OrthographicProjection::default_2d();
    projection.scaling_mode = ScalingMode::AutoMin {
        min_width: board_size.x + CELL,
        min_height: board_size.y + CELL,
    };

    commands.spawn((
        BoardCamera,
        Camera2d,
        Camera {
            order: 0,
            ..default()
        },
        Projection::Orthographic(projection),
        Transform::from_translation(centre.extend(999.0)),
    ));

    // The chart gets its own camera so its viewport can be the strip under the
    // board, cleared to the chart surface. `layout_viewports` sets the viewport
    // and makes one world unit equal one logical pixel of the strip.
    commands.spawn((
        ChartCamera,
        Camera2d,
        Camera {
            order: 1,
            clear_color: ClearColorConfig::Custom(CHART_SURFACE),
            ..default()
        },
        Projection::Orthographic(OrthographicProjection::default_2d()),
        Transform::from_xyz(0.0, 0.0, 100.0),
        RenderLayers::layer(2),
    ));

    // `bevy_ui` lays out relative to its camera's viewport, so the panel needs
    // its own full-window camera. Sharing the board camera would shift the
    // whole panel right by the viewport offset.
    // `RenderLayers` keeps this camera from drawing the board a second time.
    // Nothing in the world is on layer 1, so it renders only its UI.
    let ui_camera = commands
        .spawn((
            Camera2d,
            Camera {
                order: 2,
                clear_color: ClearColorConfig::None,
                ..default()
            },
            RenderLayers::layer(1),
        ))
        .id();

    // One shared quad and ten shared materials, one for each food level.
    // The quad extends past the fill to leave room for the outside border
    // and its antialiasing. The visible fill keeps its original dimensions.
    let cell_mesh = meshes.add(Rectangle::from_length(CELL + GAP));
    let cell_palette = CellPalette(std::array::from_fn(|food| {
        cell_materials.add(CellMaterial {
            fill: food_colour(food as i32).to_linear(),
            rim: EMPTY_CELL_RIM
                .mix(&FED_CELL_RIM, food as f32 / MAX_FOOD as f32)
                .to_linear(),
            // Fill half-size, corner radius, quad size, maximum border width.
            shape: Vec4::new(
                (CELL - GAP) * 0.5,
                (CELL - GAP) * CELL_CORNER,
                CELL + GAP,
                GAP * 0.25,
            ),
        })
    }));
    for row in 0..rows {
        for col in 0..cols {
            commands.spawn((
                Cell { row, col },
                Mesh2d(cell_mesh.clone()),
                MeshMaterial2d(cell_palette.0[0].clone()),
                Transform::from_translation(cell_to_world(row, col).extend(0.0)),
            ));
        }
    }
    commands.insert_resource(cell_palette);

    // The 32px insect art uses nearest sampling at every zoom (also in the
    // sidebar and edit preview). Keep smooth sampling for other UI images.
    let inner = CELL - GAP;
    let font: Handle<Font> = asset_server.load(fonts::REGULAR);
    let creature_assets = CreatureAssets {
        aphid: load_embedded_asset!(
            &*asset_server,
            "../../../assets/sprites/aphid.png",
            |settings: &mut ImageLoaderSettings| settings.sampler = ImageSampler::nearest()
        ),
        ladybug: load_embedded_asset!(
            &*asset_server,
            "../../../assets/sprites/ladybug.png",
            |settings: &mut ImageLoaderSettings| settings.sampler = ImageSampler::nearest()
        ),
        badge: meshes.add(Circle::new(inner * BADGE_RADIUS)),
        badge_shadow_mesh: meshes.add(Circle::new(inner * (BADGE_RADIUS + BADGE_SHADOW_OFFSET))),
        badge_colours: [
            materials.add(ColorMaterial::from_color(BADGE_APHID)),
            materials.add(ColorMaterial::from_color(BADGE_LADYBUG)),
        ],
        badge_shadow: materials.add(ColorMaterial::from_color(BADGE_SHADOW)),
        font: font.clone(),
    };
    commands.spawn((
        EditPreview,
        Sprite {
            image: creature_assets.aphid.clone(),
            custom_size: Some(Vec2::splat(CREATURE_SPRITE_SIZE * 1.6)),
            color: Color::srgba(1.0, 1.0, 1.0, 0.65),
            ..default()
        },
        Transform::from_xyz(0.0, 0.0, 5.0),
        Visibility::Hidden,
    ));

    commands.insert_resource(Stats {
        turn: 0,
        aphids: summary.aphids,
        ladybugs: summary.ladybugs,
        food: summary.food,
        births: 0,
        deaths: 0,
        extinct: summary.is_extinct(),
        deltas: [0; 3],
    });
    let params = Params {
        aphid: board.aphid_params(),
        ladybug: board.ladybug_params(),
        food: board.food_params(),
    };
    spawn_panel(&mut commands, &params, ui_camera, &creature_assets);
    spawn_summary(
        &mut commands,
        ui_camera,
        &creature_assets,
        load_embedded_asset!(
            &*asset_server,
            "../../../assets/sprites/leaf.png",
            |settings: &mut ImageLoaderSettings| settings.sampler = ImageSampler::nearest()
        ),
    );
    commands.insert_resource(creature_assets);
    commands.insert_resource(SavedParams(params));
    commands.insert_resource(params);

    let mut history = History::default();
    history.record(0, summary.aphids, summary.ladybugs);
    commands.insert_resource(history);

    // End-of-line dots: an 8px dot on a 2px ring of the surface colour, so the
    // dot stays legible where the other line passes through it.
    let dot = meshes.add(Circle::new(4.0));
    let ring = meshes.add(Circle::new(6.0));
    let ring_material = materials.add(ColorMaterial::from_color(CHART_SURFACE));
    for (series, colour) in CHART_SERIES.into_iter().enumerate() {
        commands.spawn((
            EndDot(series),
            Mesh2d(ring.clone()),
            MeshMaterial2d(ring_material.clone()),
            Transform::from_xyz(0.0, 0.0, 10.0),
            Visibility::Hidden,
            RenderLayers::layer(2),
        ));
        commands.spawn((
            EndDot(series),
            Mesh2d(dot.clone()),
            MeshMaterial2d(materials.add(ColorMaterial::from_color(colour))),
            Transform::from_xyz(0.0, 0.0, 11.0),
            Visibility::Hidden,
            RenderLayers::layer(2),
        ));
    }
    spawn_chart_overlay(&mut commands, ui_camera, font.clone());
    spawn_cell_popup(&mut commands, ui_camera, font);
    // Hidden until switched on; `rebuild_food_overlay` fills it in. Sits
    // between the cells (z 0) and the creatures (z 1).
    commands.spawn((
        FoodLayer,
        Mesh2d(meshes.add(food_overlay_mesh(&board))),
        MeshMaterial2d(materials.add(ColorMaterial::default())),
        Transform::from_xyz(0.0, 0.0, 0.5),
        Visibility::Hidden,
    ));
    commands.spawn((
        PopulationLayer,
        Mesh2d(meshes.add(population_mesh(&board))),
        MeshMaterial2d(materials.add(ColorMaterial::default())),
        Transform::from_xyz(0.0, 0.0, 2.0),
        Visibility::Hidden,
    ));
    commands.insert_resource(BoardRes(board));
    commands.insert_resource(Rng(random));

    if let Ok(path) = std::env::var("CLC_SCREENSHOT") {
        // Run turns quickly so the captured chart has real history to show.
        commands.insert_resource(StepTimer(Timer::from_seconds(0.03, TimerMode::Repeating)));
        commands.insert_resource(ScreenshotProbe {
            path,
            timer: Timer::from_seconds(STEP_SECONDS * 6.0, TimerMode::Once),
            stage: 0,
        });
    }
}
