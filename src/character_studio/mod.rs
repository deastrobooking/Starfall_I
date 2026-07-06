//! Character Studio — the in-game human character generator.
//!
//! First pillar of the in-game asset-designer suite: a Blender-like viewport
//! (orbit camera, turntable, studio lighting) driven entirely by reusable
//! preset generators over a [`spec::CharacterSpec`] source of truth:
//!
//! ```text
//! keys/presets/randomize → CharacterSpec → generators → CharacterPatch → meshes
//! ```
//!
//! Everything on screen is generated in `human_mesh.rs` from math templates —
//! no external model files — so generated characters can later be exported
//! (e.g. `.glb`) directly from the data we build here. The sibling armor/parts
//! tool on the roster heroes is the **Robot Builder** (`AppState::CharacterDesign`);
//! this studio is entered from it with F6.

pub mod generators;
pub mod human_mesh;
pub mod spec;

use bevy::prelude::*;
use std::path::PathBuf;

use crate::state::AppState;
use generators::build_character_patch;
use spec::{CharacterSpec, HairStyle, MorphField, OutfitLayer};

pub struct CharacterStudioPlugin;

impl Plugin for CharacterStudioPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<StudioState>()
            .add_systems(OnEnter(AppState::CharacterStudio), setup_studio)
            .add_systems(OnExit(AppState::CharacterStudio), cleanup_studio)
            .add_systems(
                Update,
                (
                    studio_input,
                    rebuild_preview.after(studio_input),
                    orbit_camera,
                    update_panel_text,
                )
                    .run_if(in_state(AppState::CharacterStudio)),
            );
    }
}

// ── State ─────────────────────────────────────────────────────────────────────

#[derive(Resource)]
pub struct StudioState {
    pub spec: CharacterSpec,
    cursor: usize,
    undo: Vec<CharacterSpec>,
    dirty: bool,
    yaw: f32,
    pitch: f32,
    distance: f32,
    status: String,
}

impl Default for StudioState {
    fn default() -> Self {
        Self {
            spec: generators::preset_male(),
            cursor: 0,
            undo: Vec::new(),
            dirty: true,
            yaw: 0.35,
            pitch: 0.12,
            distance: 3.4,
            status: String::new(),
        }
    }
}

impl StudioState {
    fn push_undo(&mut self) {
        self.undo.push(self.spec);
        if self.undo.len() > 64 {
            self.undo.remove(0);
        }
    }
}

#[derive(Component)]
struct StudioRoot;

#[derive(Component)]
struct StudioPreview;

#[derive(Component)]
struct StudioCamera;

#[derive(Component)]
struct StudioPanelText;

// ── Setup / teardown ──────────────────────────────────────────────────────────

fn setup_studio(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut state: ResMut<StudioState>,
) {
    state.dirty = true;
    state.status = "Character Studio ready".to_string();

    commands.spawn((
        StudioRoot,
        StudioCamera,
        Camera3d::default(),
        Transform::from_xyz(0.0, 1.5, 3.4).looking_at(Vec3::new(0.0, 0.95, 0.0), Vec3::Y),
    ));
    // Key light + cool fill, matching a studio turntable setup.
    commands.spawn((
        StudioRoot,
        DirectionalLight {
            illuminance: 11_000.0,
            shadow_maps_enabled: true,
            ..default()
        },
        Transform::from_xyz(2.5, 4.0, 2.5).looking_at(Vec3::new(0.0, 1.0, 0.0), Vec3::Y),
    ));
    commands.spawn((
        StudioRoot,
        DirectionalLight {
            illuminance: 3_200.0,
            shadow_maps_enabled: false,
            ..default()
        },
        Transform::from_xyz(-3.0, 2.0, -2.0).looking_at(Vec3::new(0.0, 1.0, 0.0), Vec3::Y),
    ));
    // Pedestal + floor disc.
    commands.spawn((
        StudioRoot,
        Mesh3d(meshes.add(Mesh::from(Cylinder::new(1.15, 0.08)))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.22, 0.24, 0.28),
            perceptual_roughness: 0.6,
            ..default()
        })),
        Transform::from_xyz(0.0, -0.04, 0.0),
    ));
    commands.spawn((
        StudioRoot,
        Mesh3d(meshes.add(Mesh::from(Cylinder::new(4.5, 0.02)))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.10, 0.11, 0.13),
            perceptual_roughness: 0.9,
            ..default()
        })),
        Transform::from_xyz(0.0, -0.09, 0.0),
    ));

    // Left control panel.
    commands
        .spawn((
            StudioRoot,
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(10.0),
                top: Val::Px(10.0),
                bottom: Val::Px(10.0),
                width: Val::Px(330.0),
                padding: UiRect::all(Val::Px(10.0)),
                flex_direction: FlexDirection::Column,
                ..default()
            },
            BackgroundColor(Color::srgba(0.02, 0.03, 0.05, 0.86)),
        ))
        .with_children(|panel| {
            panel.spawn((
                StudioPanelText,
                Text::new(""),
                TextFont {
                    font_size: FontSize::Px(12.0),
                    ..default()
                },
                TextColor(Color::srgb(0.86, 0.92, 0.98)),
            ));
        });
}

fn cleanup_studio(
    mut commands: Commands,
    roots: Query<Entity, With<StudioRoot>>,
    previews: Query<Entity, With<StudioPreview>>,
) {
    for e in roots.iter().chain(previews.iter()) {
        commands.entity(e).despawn();
    }
}

// ── Rebuild pipeline ──────────────────────────────────────────────────────────

fn rebuild_preview(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut state: ResMut<StudioState>,
    previews: Query<Entity, With<StudioPreview>>,
) {
    if !state.dirty {
        return;
    }
    state.dirty = false;
    for e in previews.iter() {
        commands.entity(e).despawn();
    }
    let patch = build_character_patch(&state.spec);
    let root = human_mesh::spawn_human(
        &mut commands,
        &mut meshes,
        &mut materials,
        &patch,
        Transform::IDENTITY,
    );
    commands.entity(root).insert(StudioPreview);
}

// ── Input ─────────────────────────────────────────────────────────────────────

fn studio_input(
    keys: Res<ButtonInput<KeyCode>>,
    mut state: ResMut<StudioState>,
    mut next_state: ResMut<NextState<AppState>>,
) {
    if keys.just_pressed(KeyCode::Escape) {
        next_state.set(AppState::CharacterDesign);
        return;
    }

    let fields = MorphField::ALL;
    if keys.just_pressed(KeyCode::ArrowUp) {
        state.cursor = (state.cursor + fields.len() - 1) % fields.len();
    }
    if keys.just_pressed(KeyCode::ArrowDown) {
        state.cursor = (state.cursor + 1) % fields.len();
    }
    let step = if keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight) {
        0.15
    } else {
        0.05
    };
    let field = fields[state.cursor];
    let adjust = |state: &mut StudioState, dir: f32| {
        state.push_undo();
        let v = field.get(&state.spec) + dir * step;
        field.set(&mut state.spec, v);
        state.dirty = true;
        state.status = format!("{} = {:.2}", field.label(), field.get(&state.spec));
    };
    if keys.just_pressed(KeyCode::ArrowLeft) {
        adjust(&mut state, -1.0);
    }
    if keys.just_pressed(KeyCode::ArrowRight) {
        adjust(&mut state, 1.0);
    }

    // Presets.
    let preset =
        |state: &mut StudioState, label: &str, apply: &dyn Fn(&mut CharacterSpec)| {
            state.push_undo();
            apply(&mut state.spec);
            state.dirty = true;
            state.status = format!("Preset: {label}");
        };
    if keys.just_pressed(KeyCode::Digit1) {
        preset(&mut state, "Male", &|s| *s = generators::preset_male());
    }
    if keys.just_pressed(KeyCode::Digit2) {
        preset(&mut state, "Female", &|s| *s = generators::preset_female());
    }
    if keys.just_pressed(KeyCode::Digit3) {
        preset(&mut state, "Athletic", &generators::apply_athletic);
    }
    if keys.just_pressed(KeyCode::Digit4) {
        preset(&mut state, "Heavy", &generators::apply_heavy);
    }
    if keys.just_pressed(KeyCode::Digit5) {
        preset(&mut state, "Slim", &generators::apply_slim);
    }
    if keys.just_pressed(KeyCode::Digit6) {
        preset(&mut state, "Soft Face", &generators::apply_soft_face);
    }
    if keys.just_pressed(KeyCode::Digit7) {
        preset(&mut state, "Sharp Face", &generators::apply_sharp_face);
    }

    // Style cyclers.
    let cycle = |state: &mut StudioState, label: &str, apply: &dyn Fn(&mut CharacterSpec)| {
        state.push_undo();
        apply(&mut state.spec);
        state.dirty = true;
        state.status = label.to_string();
    };
    if keys.just_pressed(KeyCode::KeyO) {
        cycle(&mut state, "Outfit layer", &|s| {
            let all = OutfitLayer::ALL;
            let i = all.iter().position(|o| *o == s.style.outfit).unwrap_or(0);
            s.style.outfit = all[(i + 1) % all.len()];
        });
    }
    if keys.just_pressed(KeyCode::KeyH) {
        cycle(&mut state, "Hair style", &|s| {
            let all = HairStyle::ALL;
            let i = all.iter().position(|h| *h == s.style.hair).unwrap_or(0);
            s.style.hair = all[(i + 1) % all.len()];
        });
    }
    if keys.just_pressed(KeyCode::KeyC) {
        cycle(&mut state, "Hair color", &|s| {
            s.style.hair_color = (s.style.hair_color + 1) % 8;
        });
    }
    if keys.just_pressed(KeyCode::KeyK) {
        cycle(&mut state, "Skin tone", &|s| {
            s.style.skin_tone = (s.style.skin_tone + 1) % 8;
        });
    }
    if keys.just_pressed(KeyCode::KeyI) {
        cycle(&mut state, "Eye color", &|s| {
            s.style.eye_color = (s.style.eye_color + 1) % 6;
        });
    }
    if keys.just_pressed(KeyCode::KeyP) {
        cycle(&mut state, "Primary color", &|s| {
            s.style.primary_color = (s.style.primary_color + 1) % 8;
        });
    }
    if keys.just_pressed(KeyCode::KeyL) {
        cycle(&mut state, "Secondary color", &|s| {
            s.style.secondary_color = (s.style.secondary_color + 1) % 8;
        });
    }

    if keys.just_pressed(KeyCode::KeyR) {
        state.push_undo();
        let mut spec = state.spec;
        generators::randomize(&mut spec);
        state.spec = spec;
        state.dirty = true;
        state.status = "Randomized".to_string();
    }
    if keys.just_pressed(KeyCode::KeyZ) {
        if let Some(prev) = state.undo.pop() {
            state.spec = prev;
            state.dirty = true;
            state.status = "Undo".to_string();
        }
    }
    if keys.just_pressed(KeyCode::F5) {
        match save_spec(&state.spec) {
            Ok(path) => state.status = format!("Saved {}", path.display()),
            Err(err) => state.status = format!("Save failed: {err}"),
        }
    }
    if keys.just_pressed(KeyCode::F8) {
        match load_spec() {
            Ok(spec) => {
                state.push_undo();
                state.spec = spec;
                state.dirty = true;
                state.status = "Loaded".to_string();
            }
            Err(err) => state.status = format!("Load failed: {err}"),
        }
    }
}

// ── Camera ────────────────────────────────────────────────────────────────────

fn orbit_camera(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    mut state: ResMut<StudioState>,
    mut cam_q: Query<&mut Transform, With<StudioCamera>>,
) {
    let dt = time.delta_secs();
    if keys.pressed(KeyCode::KeyA) {
        state.yaw += 1.6 * dt;
    }
    if keys.pressed(KeyCode::KeyD) {
        state.yaw -= 1.6 * dt;
    }
    if keys.pressed(KeyCode::KeyW) {
        state.pitch = (state.pitch + 1.0 * dt).min(1.2);
    }
    if keys.pressed(KeyCode::KeyS) {
        state.pitch = (state.pitch - 1.0 * dt).max(-0.4);
    }
    if keys.pressed(KeyCode::Equal) {
        state.distance = (state.distance - 2.4 * dt).max(1.2);
    }
    if keys.pressed(KeyCode::Minus) {
        state.distance = (state.distance + 2.4 * dt).min(8.0);
    }

    let focus = Vec3::new(0.0, 0.95, 0.0);
    let rot = Quat::from_rotation_y(state.yaw) * Quat::from_rotation_x(-state.pitch);
    let eye = focus + rot * Vec3::new(0.0, 0.0, state.distance);
    for mut tf in cam_q.iter_mut() {
        *tf = Transform::from_translation(eye).looking_at(focus, Vec3::Y);
    }
}

// ── Panel ─────────────────────────────────────────────────────────────────────

fn update_panel_text(
    state: Res<StudioState>,
    mut text_q: Query<&mut Text, With<StudioPanelText>>,
) {
    let spec = &state.spec;
    let mut body = String::new();
    body.push_str("CHARACTER STUDIO\n");
    body.push_str("mesh generator · preset templates\n\n");
    body.push_str(&format!(
        "Sex {}   Outfit {}\nHair {} · Skin {} · Eyes {}\nPrimary {} · Secondary {}\n\n",
        spec.sex.label(),
        spec.style.outfit.label(),
        spec.style.hair.label(),
        spec.style.skin_tone,
        spec.style.eye_color,
        spec.style.primary_color,
        spec.style.secondary_color,
    ));
    for (i, field) in MorphField::ALL.iter().enumerate() {
        let cursor = if i == state.cursor { ">" } else { " " };
        let v = field.get(spec);
        let filled = (v * 10.0).round() as usize;
        let bar: String = (0..10).map(|b| if b < filled { '#' } else { '-' }).collect();
        body.push_str(&format!("{cursor} {:<12} [{bar}] {v:.2}\n", field.label()));
    }
    body.push_str(
        "\nUp/Dn select  Lt/Rt adjust (Shift big)\n\
         1 Male 2 Female 3 Athl 4 Heavy 5 Slim\n\
         6 Soft 7 Sharp face   R randomize  Z undo\n\
         O outfit  H hair  C hair-col  K skin\n\
         I eyes  P primary  L secondary\n\
         A/D W/S orbit  +/- zoom\n\
         F5 save  F8 load  Esc back to Robot Builder\n\n",
    );
    body.push_str(&state.status);
    for mut text in text_q.iter_mut() {
        *text = Text::new(body.clone());
    }
}

// ── Save / load ───────────────────────────────────────────────────────────────

fn preset_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("assets")
        .join("presets")
        .join("humans")
}

fn save_spec(spec: &CharacterSpec) -> Result<PathBuf, String> {
    let dir = preset_dir();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join("studio_human.json");
    let json = serde_json::to_string_pretty(spec).map_err(|e| e.to_string())?;
    std::fs::write(&path, json).map_err(|e| e.to_string())?;
    Ok(path)
}

fn load_spec() -> Result<CharacterSpec, String> {
    let path = preset_dir().join("studio_human.json");
    let json = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    serde_json::from_str(&json).map_err(|e| e.to_string())
}
