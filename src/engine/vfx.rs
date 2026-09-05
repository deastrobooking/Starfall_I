//! Data-driven particle systems ("Niagara-lite"): the CPU-simulated runtime
//! for [`starfall_vfx_graph`] documents.
//!
//! This is Phase 1 of the VFX plan in `docs/guides/vfx-forge-plan.md`: an
//! artist-editable module stack (spawn shape, gravity, drag, curl noise,
//! color/size-over-life) that ships as data and needs no engine recompile to
//! retune, simulated on the CPU and rendered as ordinary `Mesh3d` entities —
//! the same pattern `combat::feedback`'s hit-flash orb already uses, just
//! generalized to n particles per event instead of one. A GPU compute backend
//! is a planned, additive Phase 2: nothing here assumes CPU simulation is
//! permanent, since [`CompiledModule`] is exactly the input a WGSL codegen
//! backend would also consume.
//!
//! **What's real in this slice:** a fixed native module registry, document
//! validation/compilation, spawn rate + burst timing, per-particle module
//! simulation, and color/size-over-life sampling — wired live to
//! [`EnemyDamagedEvent`] so `impact_spark` fires in actual combat.
//! **What's deliberately deferred:** true camera-facing sprite billboards
//! (particles render as small unlit orbs, matching the existing hit-flash
//! convention, rather than quads), the ribbon/light renderers, the
//! `assets/published/` publish-pipeline hook, and any node-graph editor UI —
//! see the plan doc's cut list for the reasoning.

use bevy::prelude::*;
use rand::Rng;

use starfall_vfx_graph::{
    BillboardMode, BurstDef, CompiledEmitter, CompiledModule, CompiledVfxSystem, LoopMode,
    ModuleRegistry, RendererDef, VfxSystemDocument,
};

use crate::events::EnemyDamagedEvent;

pub struct VfxPlugin;

impl Plugin for VfxPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<SpawnVfxEvent>()
            .add_systems(Startup, setup_vfx)
            .add_systems(
                Update,
                (
                    trigger_impact_spark_on_damage,
                    handle_spawn_requests,
                    advance_emitters_and_spawn_particles,
                    simulate_particles,
                )
                    .chain(),
            );
    }
}

/// Request to instantiate one compiled VFX system at a world position. Public
/// so any gameplay system can trigger a built-in effect without depending on
/// this module's internals — `combat::feedback` and future weapon/prefab
/// hooks fire the same event this module's own damage hook uses.
#[derive(Message, Debug, Clone)]
pub struct SpawnVfxEvent {
    pub system_id: String,
    pub position: Vec3,
}

/// Hard cap on simultaneously live particles across every emitter, protecting
/// frame time in 4-player split-screen fights — the same budget-guard idiom
/// `combat::feedback::spawn_hit_flash` already uses for its orbs.
const MAX_LIVE_PARTICLES: usize = 500;

// ── Catalog ──────────────────────────────────────────────────────────────────

/// The compiled, ready-to-play built-in VFX systems. Hand-authored as
/// [`VfxSystemDocument`]s and compiled once at startup — the same "Rust
/// catalog first, file/publish-pipeline loading later" sequencing
/// `world::platformer_chunk_library` shipped with.
#[derive(Resource)]
pub struct VfxCatalog {
    systems: Vec<(String, CompiledVfxSystem)>,
}

impl VfxCatalog {
    fn get(&self, system_id: &str) -> Option<&CompiledVfxSystem> {
        self.systems
            .iter()
            .find(|(id, _)| id == system_id)
            .map(|(_, system)| system)
    }
}

fn setup_vfx(mut commands: Commands, mut meshes: ResMut<Assets<Mesh>>) {
    let registry = ModuleRegistry::builtin();
    let mut systems: Vec<(String, CompiledVfxSystem)> = built_in_documents()
        .into_iter()
        .map(|(id, document)| {
            let compiled = document.compile_runtime(&registry).unwrap_or_else(|error| {
                panic!("built-in VFX system '{id}' failed to compile: {error}")
            });
            (id.to_string(), compiled)
        })
        .collect();
    let vfx_dir = crate::engine::platform_paths::asset_root().join("vfx");
    for (id, document) in load_authored_documents(&vfx_dir) {
        match document.compile_runtime(&registry) {
            Ok(compiled) => match systems.iter_mut().find(|(existing, _)| *existing == id) {
                Some(slot) => slot.1 = compiled,
                None => systems.push((id, compiled)),
            },
            Err(error) => warn!("authored VFX system '{id}' failed to compile: {error}; ignoring"),
        }
    }
    commands.insert_resource(VfxCatalog { systems });
    commands.insert_resource(VfxAssets {
        particle_mesh: meshes.add(Mesh::from(Sphere::new(0.18))),
    });
}

/// Loose, designer-editable VFX documents outside the (not-yet-built) Forge
/// publish pipeline: any `<asset_root>/vfx/*.json` file is parsed as a
/// [`VfxSystemDocument`] and, if it validates and compiles, added to the
/// catalog — overriding a built-in system of the same id, or adding a new
/// one. A missing directory is a first-class empty state and a bad file is
/// skipped with a warning, matching every other loose/published content
/// loader in this codebase (`world::published_content`): a typo'd asset
/// should never crash the game, only lose the effect.
///
/// Takes the directory explicitly (rather than resolving `asset_root()`
/// itself) so tests can point it at a temp directory instead of racing on
/// the process-global `STARFALL_ASSET_ROOT` environment variable.
fn load_authored_documents(dir: &std::path::Path) -> Vec<(String, VfxSystemDocument)> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut documents = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(error) => {
                warn!("could not read {}: {error}; ignoring", path.display());
                continue;
            }
        };
        match VfxSystemDocument::parse(&text)
            .and_then(|document| document.validate_schema().map(|_| document))
        {
            Ok(document) => documents.push((document.id.to_string(), document)),
            Err(error) => warn!(
                "{} is not a valid VFX document: {error}; ignoring",
                path.display()
            ),
        }
    }
    documents
}

#[derive(Resource)]
struct VfxAssets {
    particle_mesh: Handle<Mesh>,
}

/// Weapon and melee impacts fire this automatically; every enemy hit gets a
/// small burst without any per-weapon wiring.
fn trigger_impact_spark_on_damage(
    mut damaged: MessageReader<EnemyDamagedEvent>,
    mut spawn: MessageWriter<SpawnVfxEvent>,
) {
    for event in damaged.read() {
        spawn.write(SpawnVfxEvent {
            system_id: "impact_spark".to_string(),
            position: event.position,
        });
    }
}

// ── Emitter instances ────────────────────────────────────────────────────────

/// One live instance of one emitter within a spawned [`SpawnVfxEvent`]
/// system. Owns only spawn timing; particles it spawns are independent
/// entities that outlive it, so a one-shot emitter can despawn itself the
/// moment it finishes spawning.
#[derive(Component)]
struct VfxEmitterInstance {
    system_id: String,
    emitter_index: usize,
    origin: Vec3,
    age: f32,
    spawn_accumulator: f32,
    fired_bursts: usize,
}

fn handle_spawn_requests(
    mut commands: Commands,
    mut requests: MessageReader<SpawnVfxEvent>,
    catalog: Res<VfxCatalog>,
) {
    for request in requests.read() {
        let Some(system) = catalog.get(&request.system_id) else {
            warn!(
                "SpawnVfxEvent referenced unknown VFX system '{}'",
                request.system_id
            );
            continue;
        };
        for (index, _emitter) in system.emitters.iter().enumerate() {
            commands.spawn(VfxEmitterInstance {
                system_id: request.system_id.clone(),
                emitter_index: index,
                origin: request.position,
                age: 0.0,
                spawn_accumulator: 0.0,
                fired_bursts: 0,
            });
        }
    }
}

fn advance_emitters_and_spawn_particles(
    mut commands: Commands,
    time: Res<Time>,
    catalog: Res<VfxCatalog>,
    assets: Res<VfxAssets>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut emitters: Query<(Entity, &mut VfxEmitterInstance)>,
    live_particles: Query<(), With<VfxParticle>>,
) {
    let dt = time.delta_secs();
    let mut budget = MAX_LIVE_PARTICLES.saturating_sub(live_particles.iter().count());
    for (entity, mut instance) in emitters.iter_mut() {
        let Some(system) = catalog.get(&instance.system_id) else {
            commands.entity(entity).despawn();
            continue;
        };
        let Some(emitter) = system.emitters.get(instance.emitter_index) else {
            commands.entity(entity).despawn();
            continue;
        };
        instance.age += dt;

        let spawning_window_open = match emitter.loop_mode {
            LoopMode::Looping => true,
            LoopMode::OneShot { spawn_seconds } => instance.age <= spawn_seconds,
        };

        let mut to_spawn = 0u32;
        if spawning_window_open {
            instance.spawn_accumulator += emitter.rate_per_second * dt;
            to_spawn += instance.spawn_accumulator.floor() as u32;
            instance.spawn_accumulator -= to_spawn as f32;
            while instance.fired_bursts < emitter.burst.len()
                && emitter.burst[instance.fired_bursts].time <= instance.age
            {
                to_spawn += emitter.burst[instance.fired_bursts].count;
                instance.fired_bursts += 1;
            }
        }

        for _ in 0..to_spawn {
            if budget == 0 {
                break;
            }
            budget -= 1;
            spawn_particle(
                &mut commands,
                instance.system_id.clone(),
                instance.emitter_index,
                emitter,
                instance.origin,
                &assets,
                &mut materials,
            );
        }

        let finished_spawning = matches!(
            emitter.loop_mode,
            LoopMode::OneShot { spawn_seconds } if instance.age > spawn_seconds
        );
        if finished_spawning {
            commands.entity(entity).despawn();
        }
    }
}

fn spawn_particle(
    commands: &mut Commands,
    system_id: String,
    emitter_index: usize,
    emitter: &CompiledEmitter,
    origin: Vec3,
    assets: &VfxAssets,
    materials: &mut Assets<StandardMaterial>,
) {
    let mut rng = rand::thread_rng(); // Presentation-only, like camera_shake_offset: no determinism/replay contract applies to spark scatter.
    let mut position = origin;
    let mut velocity = Vec3::ZERO;
    let mut lifetime = 1.0f32;
    let mut base_color = Vec4::new(1.0, 1.0, 1.0, 1.0);
    let mut base_size = 1.0f32;

    for module in &emitter.init_modules {
        match module.kind {
            "spawn_position_sphere" => {
                let radius = module.float("radius", 0.0);
                position += random_point_in_sphere(&mut rng, radius);
            }
            "spawn_velocity_cone" => {
                let [dx, dy, dz] = module.vec3("direction", [0.0, 1.0, 0.0]);
                let direction = Vec3::new(dx, dy, dz).try_normalize().unwrap_or(Vec3::Y);
                let half_angle = module.float("half_angle_degrees", 0.0).to_radians();
                let speed = module.float("speed", 0.0);
                let variance = module.float("speed_variance", 0.0);
                let spread = random_direction_in_cone(&mut rng, direction, half_angle);
                let sampled_speed = speed + rng.gen_range(-variance..=variance);
                velocity = spread * sampled_speed.max(0.0);
            }
            "spawn_lifetime" => {
                let seconds = module.float("seconds", 1.0);
                let variance = module.float("variance", 0.0);
                lifetime = (seconds + rng.gen_range(-variance..=variance)).max(0.01);
            }
            "spawn_color" => {
                let [r, g, b] = module.vec3("color", [1.0, 1.0, 1.0]);
                base_color = Vec4::new(r, g, b, base_color.w);
            }
            "spawn_size" => {
                base_size = module.float("size", 1.0);
            }
            _ => {}
        }
    }

    let material = materials.add(StandardMaterial {
        base_color: Color::srgba(base_color.x, base_color.y, base_color.z, base_color.w),
        emissive: LinearRgba::rgb(base_color.x * 2.0, base_color.y * 2.0, base_color.z * 2.0),
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        ..default()
    });

    commands.spawn((
        Mesh3d(assets.particle_mesh.clone()),
        MeshMaterial3d(material),
        Transform::from_translation(position).with_scale(Vec3::splat(base_size.max(0.001))),
        VfxParticle {
            system_id,
            emitter_index,
            velocity,
            age: 0.0,
            lifetime,
            base_size,
        },
    ));
}

fn random_point_in_sphere(rng: &mut impl Rng, radius: f32) -> Vec3 {
    if radius <= 0.0 {
        return Vec3::ZERO;
    }
    loop {
        let candidate = Vec3::new(
            rng.gen_range(-1.0..=1.0),
            rng.gen_range(-1.0..=1.0),
            rng.gen_range(-1.0..=1.0),
        );
        if candidate.length_squared() <= 1.0 {
            return candidate * radius;
        }
    }
}

fn random_direction_in_cone(rng: &mut impl Rng, axis: Vec3, half_angle: f32) -> Vec3 {
    if half_angle <= 0.0 {
        return axis;
    }
    let (right, up) = axis.any_orthonormal_pair();
    let cos_max = half_angle.cos();
    let z = rng.gen_range(cos_max..=1.0);
    let phi = rng.gen_range(0.0..std::f32::consts::TAU);
    let radius = (1.0 - z * z).max(0.0).sqrt();
    (right * (radius * phi.cos()) + up * (radius * phi.sin()) + axis * z).normalize_or(axis)
}

// ── Particle simulation ──────────────────────────────────────────────────────

#[derive(Component)]
struct VfxParticle {
    system_id: String,
    emitter_index: usize,
    velocity: Vec3,
    age: f32,
    lifetime: f32,
    base_size: f32,
}

fn simulate_particles(
    mut commands: Commands,
    time: Res<Time>,
    catalog: Res<VfxCatalog>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut particles: Query<(
        Entity,
        &mut VfxParticle,
        &mut Transform,
        &MeshMaterial3d<StandardMaterial>,
    )>,
) {
    let dt = time.delta_secs();
    for (entity, mut particle, mut transform, material_handle) in particles.iter_mut() {
        particle.age += dt;
        if particle.age >= particle.lifetime {
            commands.entity(entity).despawn();
            continue;
        }
        let Some(system) = catalog.get(&particle.system_id) else {
            commands.entity(entity).despawn();
            continue;
        };
        let Some(emitter) = system.emitters.get(particle.emitter_index) else {
            commands.entity(entity).despawn();
            continue;
        };

        for module in &emitter.update_modules {
            apply_update_module(module, &mut particle, dt);
        }
        transform.translation += particle.velocity * dt;

        let t = (particle.age / particle.lifetime).clamp(0.0, 1.0);
        if let Some(mut material) = materials.get_mut(&material_handle.0) {
            if let Some(gradient) = emitter
                .update_modules
                .iter()
                .find(|module| module.kind == "color_over_life")
                .and_then(|module| module.gradient("gradient"))
            {
                let [r, g, b, a] = starfall_vfx_graph::sample_gradient(gradient, t);
                material.base_color = Color::srgba(r, g, b, a);
                material.emissive = LinearRgba::rgb(r * 2.0, g * 2.0, b * 2.0);
            }
        }
        if let Some(curve) = emitter
            .update_modules
            .iter()
            .find(|module| module.kind == "size_over_life")
            .and_then(|module| module.curve("curve"))
        {
            let scale = starfall_vfx_graph::sample_curve(curve, t) * particle.base_size;
            transform.scale = Vec3::splat(scale.max(0.0));
        }
    }
}

fn apply_update_module(module: &CompiledModule, particle: &mut VfxParticle, dt: f32) {
    match module.kind {
        "gravity" => {
            particle.velocity.y -= module.float("strength", 0.0) * dt;
        }
        "drag" => {
            let coefficient = module.float("coefficient", 0.0);
            particle.velocity *= (1.0 - coefficient * dt).max(0.0);
        }
        "curl_noise" => {
            let strength = module.float("strength", 0.0);
            let scale = module.float("scale", 1.0);
            let offset = cheap_curl_noise(particle.velocity, particle.age * scale);
            particle.velocity += offset * strength * dt;
        }
        // color_over_life / size_over_life are sampled directly against the
        // particle's Transform/material in `simulate_particles`, since they
        // drive presentation rather than motion.
        _ => {}
    }
}

/// A deterministic, dependency-free stand-in for real curl noise: cheap
/// enough to run per-particle per-frame and good enough for embers/sparks,
/// which only need *some* incoherent drift. A real noise crate is a drop-in
/// swap behind this one function once GPU codegen (Phase 2) needs a shared
/// noise texture anyway.
fn cheap_curl_noise(seed_from: Vec3, t: f32) -> Vec3 {
    let x = (seed_from.x * 12.9898 + t * 3.7).sin();
    let y = (seed_from.y * 78.233 + t * 5.1).sin();
    let z = (seed_from.z * 37.719 + t * 2.3).sin();
    Vec3::new(x, y, z)
}

// ── Built-in documents ───────────────────────────────────────────────────────

use starfall_graph::StableId;
use starfall_vfx_graph::{BlendMode, ModuleInstance, SpawnDef, VfxParam};

fn built_in_documents() -> Vec<(&'static str, VfxSystemDocument)> {
    vec![
        ("impact_spark", impact_spark()),
        ("ember_torch", ember_torch()),
    ]
}

/// A short-lived burst of hot sparks at a weapon or melee impact: the data
/// equivalent of `combat::feedback`'s hardcoded hit-flash orb, but with
/// several independently-moving particles instead of one scaling sphere.
fn impact_spark() -> VfxSystemDocument {
    VfxSystemDocument {
        id: StableId::new("impact_spark").unwrap(),
        label: "Impact Spark".into(),
        schema_version: starfall_vfx_graph::VFX_SCHEMA_VERSION,
        emitters: vec![EmitterDefBuilder {
            name: "sparks",
            max_particles: 24,
            spawn: SpawnDef {
                rate_per_second: 0.0,
                burst: vec![BurstDef {
                    time: 0.0,
                    count: 10,
                }],
                init_modules: vec![
                    ModuleInstance::new("spawn_position_sphere")
                        .with_param("radius", VfxParam::Float(0.15)),
                    ModuleInstance::new("spawn_velocity_cone")
                        .with_param("direction", VfxParam::Vec3([0.0, 1.0, 0.0]))
                        .with_param("half_angle_degrees", VfxParam::Float(55.0))
                        .with_param("speed", VfxParam::Float(5.5))
                        .with_param("speed_variance", VfxParam::Float(2.0)),
                    ModuleInstance::new("spawn_lifetime")
                        .with_param("seconds", VfxParam::Float(0.35))
                        .with_param("variance", VfxParam::Float(0.1)),
                    ModuleInstance::new("spawn_color")
                        .with_param("color", VfxParam::Vec3([1.0, 0.95, 0.7])),
                    ModuleInstance::new("spawn_size").with_param("size", VfxParam::Float(0.35)),
                ],
            },
            update_modules: vec![
                ModuleInstance::new("gravity").with_param("strength", VfxParam::Float(11.0)),
                ModuleInstance::new("drag").with_param("coefficient", VfxParam::Float(1.5)),
                ModuleInstance::new("color_over_life").with_param(
                    "gradient",
                    VfxParam::Gradient(vec![
                        (0.0, [1.0, 0.95, 0.7, 1.0]),
                        (0.5, [1.0, 0.55, 0.15, 0.9]),
                        (1.0, [0.6, 0.1, 0.05, 0.0]),
                    ]),
                ),
                ModuleInstance::new("size_over_life")
                    .with_param("curve", VfxParam::Curve(vec![(0.0, 1.0), (1.0, 0.15)])),
            ],
            renderer: RendererDef::Sprite {
                blend: BlendMode::Additive,
                billboard: BillboardMode::Camera,
            },
            loop_mode: LoopMode::OneShot {
                spawn_seconds: 0.02,
            },
        }
        .build()],
    }
}

/// A slow, looping ember field for torches and braziers along the castle
/// stairwell routes (`world::platformer_chunk_library`). Not yet spawned by
/// anything — it is ready for that hookup once `world_plugin`'s route-spawn
/// path is stable again; see the plan doc's near-term follow-ups.
fn ember_torch() -> VfxSystemDocument {
    VfxSystemDocument {
        id: StableId::new("ember_torch").unwrap(),
        label: "Ember Torch".into(),
        schema_version: starfall_vfx_graph::VFX_SCHEMA_VERSION,
        emitters: vec![EmitterDefBuilder {
            name: "embers",
            max_particles: 40,
            spawn: SpawnDef {
                rate_per_second: 6.0,
                burst: vec![],
                init_modules: vec![
                    ModuleInstance::new("spawn_position_sphere")
                        .with_param("radius", VfxParam::Float(0.2)),
                    ModuleInstance::new("spawn_velocity_cone")
                        .with_param("direction", VfxParam::Vec3([0.0, 1.0, 0.0]))
                        .with_param("half_angle_degrees", VfxParam::Float(15.0))
                        .with_param("speed", VfxParam::Float(1.2))
                        .with_param("speed_variance", VfxParam::Float(0.4)),
                    ModuleInstance::new("spawn_lifetime")
                        .with_param("seconds", VfxParam::Float(2.2))
                        .with_param("variance", VfxParam::Float(0.5)),
                    ModuleInstance::new("spawn_color")
                        .with_param("color", VfxParam::Vec3([1.0, 0.6, 0.2])),
                    ModuleInstance::new("spawn_size").with_param("size", VfxParam::Float(0.14)),
                ],
            },
            update_modules: vec![
                ModuleInstance::new("drag").with_param("coefficient", VfxParam::Float(0.3)),
                ModuleInstance::new("curl_noise")
                    .with_param("strength", VfxParam::Float(0.8))
                    .with_param("scale", VfxParam::Float(1.0)),
                ModuleInstance::new("color_over_life").with_param(
                    "gradient",
                    VfxParam::Gradient(vec![
                        (0.0, [1.0, 0.7, 0.25, 1.0]),
                        (1.0, [0.4, 0.1, 0.05, 0.0]),
                    ]),
                ),
                ModuleInstance::new("size_over_life")
                    .with_param("curve", VfxParam::Curve(vec![(0.0, 1.0), (1.0, 0.4)])),
            ],
            renderer: RendererDef::Sprite {
                blend: BlendMode::Additive,
                billboard: BillboardMode::Camera,
            },
            loop_mode: LoopMode::Looping,
        }
        .build()],
    }
}

/// `EmitterDef` has enough fields that positional construction reads badly at
/// two call sites; this local builder is not part of the public API.
struct EmitterDefBuilder {
    name: &'static str,
    max_particles: u32,
    spawn: SpawnDef,
    update_modules: Vec<ModuleInstance>,
    renderer: RendererDef,
    loop_mode: LoopMode,
}

impl EmitterDefBuilder {
    fn build(self) -> starfall_vfx_graph::EmitterDef {
        starfall_vfx_graph::EmitterDef {
            name: self.name.to_string(),
            max_particles: self.max_particles,
            spawn: self.spawn,
            update_modules: self.update_modules,
            renderer: self.renderer,
            loop_mode: self.loop_mode,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn built_in_documents_validate_and_compile() {
        let registry = ModuleRegistry::builtin();
        for (id, document) in built_in_documents() {
            document
                .validate_schema()
                .unwrap_or_else(|error| panic!("'{id}' failed schema validation: {error}"));
            document
                .compile_runtime(&registry)
                .unwrap_or_else(|error| panic!("'{id}' failed to compile: {error}"));
        }
    }

    #[test]
    fn a_missing_authored_directory_is_a_first_class_empty_state() {
        let dir = std::env::temp_dir().join("starfall_vfx_test_missing_dir_does_not_exist");
        let _ = std::fs::remove_dir_all(&dir);
        assert!(load_authored_documents(&dir).is_empty());
    }

    #[test]
    fn authored_documents_load_and_a_bad_file_is_skipped_not_fatal() {
        let dir = std::env::temp_dir().join(format!(
            "starfall_vfx_test_authored_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("good.json"),
            impact_spark().to_json_pretty().unwrap(),
        )
        .unwrap();
        std::fs::write(dir.join("bad.json"), "{ not valid json").unwrap();
        std::fs::write(dir.join("ignored.txt"), "not json at all").unwrap();

        let documents = load_authored_documents(&dir);
        assert_eq!(
            documents.len(),
            1,
            "the malformed file must be skipped, not fatal"
        );
        assert_eq!(documents[0].0, "impact_spark");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn cheap_curl_noise_stays_bounded() {
        let sample = cheap_curl_noise(Vec3::new(3.0, -2.0, 7.0), 12.5);
        assert!(sample.x.abs() <= 1.0 && sample.y.abs() <= 1.0 && sample.z.abs() <= 1.0);
    }

    #[test]
    fn random_point_in_sphere_never_exceeds_radius() {
        let mut rng = rand::thread_rng();
        for _ in 0..50 {
            let point = random_point_in_sphere(&mut rng, 2.0);
            assert!(point.length() <= 2.0 + 1e-4);
        }
    }
}
