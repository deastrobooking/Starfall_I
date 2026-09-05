//! Typed VFX authoring documents and a module-registry compiler ("Niagara-lite").
//!
//! Real Niagara compiles an artist's node graph to HLSL per platform. The
//! tractable slice of that value proposition — **artist-editable particle
//! behavior, expressed as data, that runs without recompiling the engine** —
//! does not require a shader compiler to exist first. This crate defines the
//! data model and a fixed [`ModuleRegistry`] of native module kinds; a
//! [`VfxSystemDocument`] is compiled against that registry the same way
//! `starfall-platformer-graph` compiles a route against a chunk resolver.
//!
//! The node-graph *editor* (Part 4 of the original design sketch) is
//! deliberately out of scope here: it is a UI concern that produces exactly
//! the [`ModuleInstance`] lists this crate already consumes, the same
//! separation `starfall-platformer-graph` keeps between its graph builder and
//! its runtime compiler. Ship the compiler and a CPU runtime first; the graph
//! UI is additive once the compile target it targets is proven.

use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};
use starfall_graph::StableId;

pub const VFX_SCHEMA_VERSION: u32 = 1;

// ── Parameters ──────────────────────────────────────────────────────────────

/// A module parameter value, as authored. `Curve`/`Gradient` keys are time
/// (`0.0..=1.0` of the particle's lifetime), kept sorted by
/// [`VfxSystemDocument::validate_schema`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum VfxParam {
    Float(f32),
    Vec3([f32; 3]),
    /// Time-keyed scalar curve, e.g. size-over-life.
    Curve(Vec<(f32, f32)>),
    /// Time-keyed RGBA gradient, e.g. color-over-life.
    Gradient(Vec<(f32, [f32; 4])>),
}

/// What shape of value a module parameter expects. Declared once per module
/// in the [`ModuleRegistry`]; `compile_runtime` checks every authored
/// [`VfxParam`] against it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ParamType {
    Float,
    Vec3,
    Curve,
    Gradient,
}

impl VfxParam {
    fn matches(&self, expected: ParamType) -> bool {
        matches!(
            (self, expected),
            (VfxParam::Float(_), ParamType::Float)
                | (VfxParam::Vec3(_), ParamType::Vec3)
                | (VfxParam::Curve(_), ParamType::Curve)
                | (VfxParam::Gradient(_), ParamType::Gradient)
        )
    }
}

// ── Module instances (authoring) ────────────────────────────────────────────

/// One module selected onto an emitter's init or update stack, with the
/// parameter values the artist set. `kind` is looked up in a
/// [`ModuleRegistry`] at compile time — this crate does not hardcode which
/// modules exist.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModuleInstance {
    pub kind: String,
    pub params: Vec<(String, VfxParam)>,
}

impl ModuleInstance {
    pub fn new(kind: impl Into<String>) -> Self {
        Self {
            kind: kind.into(),
            params: Vec::new(),
        }
    }

    pub fn with_param(mut self, name: impl Into<String>, value: VfxParam) -> Self {
        self.params.push((name.into(), value));
        self
    }

    fn param(&self, name: &str) -> Option<&VfxParam> {
        self.params
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, v)| v)
    }
}

// ── Spawn / renderer / loop ──────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BurstDef {
    pub time: f32,
    pub count: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct SpawnDef {
    pub rate_per_second: f32,
    pub burst: Vec<BurstDef>,
    /// Modules that set a spawned particle's initial state (position offset,
    /// initial velocity, starting color/size, lifetime).
    pub init_modules: Vec<ModuleInstance>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BlendMode {
    Alpha,
    Additive,
    Premultiplied,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BillboardMode {
    Camera,
    VelocityAligned,
    Horizontal,
}

/// What draws each live particle. `Mesh` reuses an existing mesh/material
/// pair (e.g. debris chunks); ribbon and light renderers are not modeled yet
/// — see the plan doc's cut list.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum RendererDef {
    Sprite {
        blend: BlendMode,
        billboard: BillboardMode,
    },
    Mesh {
        mesh: String,
        material: String,
    },
}

/// Whether an emitter runs forever (ambient effects: torches, waterfalls) or
/// spawns once and retires (impact sparks, one-shot bursts).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum LoopMode {
    Looping,
    /// Stop spawning after this many seconds; the emitter itself is retired
    /// once every particle it spawned has also finished.
    OneShot {
        spawn_seconds: f32,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EmitterDef {
    pub name: String,
    pub max_particles: u32,
    pub spawn: SpawnDef,
    pub update_modules: Vec<ModuleInstance>,
    pub renderer: RendererDef,
    pub loop_mode: LoopMode,
}

/// The authored, serialized VFX asset — one or more emitters that make up one
/// visual effect (e.g. "impact_spark" is one emitter; a campfire might be
/// three: flame, embers, smoke).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VfxSystemDocument {
    pub id: StableId,
    pub label: String,
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    pub emitters: Vec<EmitterDef>,
}

fn default_schema_version() -> u32 {
    VFX_SCHEMA_VERSION
}

// ── Errors ───────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum VfxLoadError {
    Parse(String),
    Schema(String),
}

impl fmt::Display for VfxLoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            VfxLoadError::Parse(msg) => write!(f, "could not parse VFX document: {msg}"),
            VfxLoadError::Schema(msg) => write!(f, "invalid VFX document: {msg}"),
        }
    }
}

impl std::error::Error for VfxLoadError {}

impl From<serde_json::Error> for VfxLoadError {
    fn from(error: serde_json::Error) -> Self {
        VfxLoadError::Parse(error.to_string())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum VfxRuntimeCompileError {
    UnknownModule {
        emitter: String,
        kind: String,
    },
    MissingParam {
        emitter: String,
        kind: String,
        param: &'static str,
    },
    WrongParamType {
        emitter: String,
        kind: String,
        param: &'static str,
    },
}

impl fmt::Display for VfxRuntimeCompileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            VfxRuntimeCompileError::UnknownModule { emitter, kind } => {
                write!(f, "emitter '{emitter}' uses unknown module kind '{kind}'")
            }
            VfxRuntimeCompileError::MissingParam {
                emitter,
                kind,
                param,
            } => write!(
                f,
                "emitter '{emitter}' module '{kind}' is missing required param '{param}'"
            ),
            VfxRuntimeCompileError::WrongParamType {
                emitter,
                kind,
                param,
            } => write!(
                f,
                "emitter '{emitter}' module '{kind}' param '{param}' has the wrong value type"
            ),
        }
    }
}

impl std::error::Error for VfxRuntimeCompileError {}

// ── Module registry ──────────────────────────────────────────────────────────

/// One native module's identity and parameter contract. A CPU runtime
/// interprets `kind` directly today; a future WGSL backend would instead map
/// `kind` to a fixed shader snippet — the compiled output ([`CompiledModule`])
/// is the same either way, which is the point of keeping this a registry
/// rather than a hardcoded enum.
#[derive(Debug, Clone, Copy)]
pub struct ModuleSpec {
    pub kind: &'static str,
    pub params: &'static [(&'static str, ParamType)],
}

#[derive(Debug, Clone, Default)]
pub struct ModuleRegistry {
    modules: BTreeMap<&'static str, ModuleSpec>,
}

impl ModuleRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, spec: ModuleSpec) {
        self.modules.insert(spec.kind, spec);
    }

    pub fn get(&self, kind: &str) -> Option<&ModuleSpec> {
        self.modules.get(kind)
    }

    pub fn kinds(&self) -> impl Iterator<Item = &'static str> + '_ {
        self.modules.keys().copied()
    }

    /// The module set this crate ships modeling for: gravity/drag/curl-noise
    /// update modules, color/size-over-life, and the init modules an emitter
    /// needs to give particles a starting position, velocity, and lifetime.
    /// A runtime is free to register additional kinds; unknown kinds at
    /// compile time are a hard error rather than a silent no-op, so a typo in
    /// an authored asset fails at publish time, not on a player's screen.
    pub fn builtin() -> Self {
        let mut registry = Self::new();
        registry.register(ModuleSpec {
            kind: "gravity",
            params: &[("strength", ParamType::Float)],
        });
        registry.register(ModuleSpec {
            kind: "drag",
            params: &[("coefficient", ParamType::Float)],
        });
        registry.register(ModuleSpec {
            kind: "curl_noise",
            params: &[("strength", ParamType::Float), ("scale", ParamType::Float)],
        });
        registry.register(ModuleSpec {
            kind: "color_over_life",
            params: &[("gradient", ParamType::Gradient)],
        });
        registry.register(ModuleSpec {
            kind: "size_over_life",
            params: &[("curve", ParamType::Curve)],
        });
        registry.register(ModuleSpec {
            kind: "spawn_position_sphere",
            params: &[("radius", ParamType::Float)],
        });
        registry.register(ModuleSpec {
            kind: "spawn_velocity_cone",
            params: &[
                ("direction", ParamType::Vec3),
                ("half_angle_degrees", ParamType::Float),
                ("speed", ParamType::Float),
                ("speed_variance", ParamType::Float),
            ],
        });
        registry.register(ModuleSpec {
            kind: "spawn_lifetime",
            params: &[
                ("seconds", ParamType::Float),
                ("variance", ParamType::Float),
            ],
        });
        registry.register(ModuleSpec {
            kind: "spawn_color",
            params: &[("color", ParamType::Vec3)],
        });
        registry.register(ModuleSpec {
            kind: "spawn_size",
            params: &[("size", ParamType::Float)],
        });
        registry
    }
}

// ── Compiled output (runtime-facing) ─────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub struct CompiledModule {
    pub kind: &'static str,
    pub params: Vec<(&'static str, VfxParam)>,
}

impl CompiledModule {
    pub fn param(&self, name: &str) -> Option<&VfxParam> {
        self.params
            .iter()
            .find(|(key, _)| *key == name)
            .map(|(_, v)| v)
    }

    pub fn float(&self, name: &str, default: f32) -> f32 {
        match self.param(name) {
            Some(VfxParam::Float(value)) => *value,
            _ => default,
        }
    }

    pub fn vec3(&self, name: &str, default: [f32; 3]) -> [f32; 3] {
        match self.param(name) {
            Some(VfxParam::Vec3(value)) => *value,
            _ => default,
        }
    }

    pub fn curve(&self, name: &str) -> Option<&[(f32, f32)]> {
        match self.param(name) {
            Some(VfxParam::Curve(points)) => Some(points),
            _ => None,
        }
    }

    pub fn gradient(&self, name: &str) -> Option<&[(f32, [f32; 4])]> {
        match self.param(name) {
            Some(VfxParam::Gradient(stops)) => Some(stops),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CompiledEmitter {
    pub name: String,
    pub max_particles: u32,
    pub rate_per_second: f32,
    pub burst: Vec<BurstDef>,
    pub init_modules: Vec<CompiledModule>,
    pub update_modules: Vec<CompiledModule>,
    pub renderer: RendererDef,
    pub loop_mode: LoopMode,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CompiledVfxSystem {
    pub id: StableId,
    pub label: String,
    pub emitters: Vec<CompiledEmitter>,
}

// ── Sampling helpers (shared by any CPU runtime) ────────────────────────────

/// Linear-interpolated sample of a sorted time-keyed scalar curve. `t` is
/// clamped to the curve's own domain rather than the caller's, so a curve
/// authored over a partial range still reads sensibly at the ends.
pub fn sample_curve(points: &[(f32, f32)], t: f32) -> f32 {
    sample_keyed(points, t, |a, b, f| a + (b - a) * f)
}

/// Linear-interpolated sample of a sorted time-keyed RGBA gradient.
pub fn sample_gradient(stops: &[(f32, [f32; 4])], t: f32) -> [f32; 4] {
    sample_keyed(stops, t, |a: [f32; 4], b: [f32; 4], f| {
        [
            a[0] + (b[0] - a[0]) * f,
            a[1] + (b[1] - a[1]) * f,
            a[2] + (b[2] - a[2]) * f,
            a[3] + (b[3] - a[3]) * f,
        ]
    })
}

fn sample_keyed<T: Copy + Default>(
    points: &[(f32, T)],
    t: f32,
    lerp: impl Fn(T, T, f32) -> T,
) -> T {
    if points.is_empty() {
        return T::default();
    }
    if points.len() == 1 || t <= points[0].0 {
        return points[0].1;
    }
    if t >= points[points.len() - 1].0 {
        return points[points.len() - 1].1;
    }
    for window in points.windows(2) {
        let (t0, v0) = window[0];
        let (t1, v1) = window[1];
        if t >= t0 && t <= t1 {
            let span = (t1 - t0).max(f32::EPSILON);
            return lerp(v0, v1, (t - t0) / span);
        }
    }
    points[points.len() - 1].1
}

// ── Validation + compilation ─────────────────────────────────────────────────

impl VfxSystemDocument {
    pub fn to_json_pretty(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    pub fn parse(source: &str) -> Result<Self, VfxLoadError> {
        Ok(serde_json::from_str(source)?)
    }

    /// Structural checks that do not require a [`ModuleRegistry`]: shape a
    /// hand-edited or graph-exported document must have regardless of which
    /// modules end up resolving.
    pub fn validate_schema(&self) -> Result<(), VfxLoadError> {
        if self.label.trim().is_empty() {
            return Err(VfxLoadError::Schema("label is empty".into()));
        }
        if self.emitters.is_empty() {
            return Err(VfxLoadError::Schema(format!(
                "system '{}' has no emitters",
                self.id
            )));
        }
        for emitter in &self.emitters {
            if emitter.name.trim().is_empty() {
                return Err(VfxLoadError::Schema(format!(
                    "system '{}' has an emitter with no name",
                    self.id
                )));
            }
            if emitter.max_particles == 0 {
                return Err(VfxLoadError::Schema(format!(
                    "emitter '{}' has max_particles = 0",
                    emitter.name
                )));
            }
            if emitter.spawn.rate_per_second < 0.0 {
                return Err(VfxLoadError::Schema(format!(
                    "emitter '{}' has a negative spawn rate",
                    emitter.name
                )));
            }
            if let LoopMode::OneShot { spawn_seconds } = emitter.loop_mode {
                if spawn_seconds < 0.0 {
                    return Err(VfxLoadError::Schema(format!(
                        "emitter '{}' has a negative one-shot spawn duration",
                        emitter.name
                    )));
                }
            }
            let mut last_burst_time = f32::NEG_INFINITY;
            for burst in &emitter.spawn.burst {
                if burst.time < last_burst_time {
                    return Err(VfxLoadError::Schema(format!(
                        "emitter '{}' bursts are not sorted by time",
                        emitter.name
                    )));
                }
                last_burst_time = burst.time;
            }
            for modules in [&emitter.spawn.init_modules, &emitter.update_modules] {
                for module in modules.iter() {
                    for (name, param) in &module.params {
                        validate_param_shape(&emitter.name, &module.kind, name, param)?;
                    }
                }
            }
        }
        Ok(())
    }

    /// Resolves every authored module against `registry`, checking required
    /// params are present with the right shape, and produces the flat,
    /// registry-ordered [`CompiledVfxSystem`] a runtime executes. This is the
    /// exact point a future graph-editor compile step also targets — the
    /// graph only ever needs to produce the [`ModuleInstance`] lists this
    /// method already consumes.
    pub fn compile_runtime(
        &self,
        registry: &ModuleRegistry,
    ) -> Result<CompiledVfxSystem, VfxRuntimeCompileError> {
        self.validate_schema()
            .map_err(|_| VfxRuntimeCompileError::UnknownModule {
                emitter: self.label.clone(),
                kind: "<schema>".into(),
            })?;
        let emitters = self
            .emitters
            .iter()
            .map(|emitter| compile_emitter(emitter, registry))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(CompiledVfxSystem {
            id: self.id.clone(),
            label: self.label.clone(),
            emitters,
        })
    }
}

fn validate_param_shape(
    emitter: &str,
    kind: &str,
    name: &str,
    param: &VfxParam,
) -> Result<(), VfxLoadError> {
    if let VfxParam::Curve(points) = param {
        if points.windows(2).any(|w| w[1].0 < w[0].0) {
            return Err(VfxLoadError::Schema(format!(
                "emitter '{emitter}' module '{kind}' param '{name}' curve keys are not sorted"
            )));
        }
    }
    if let VfxParam::Gradient(stops) = param {
        if stops.windows(2).any(|w| w[1].0 < w[0].0) {
            return Err(VfxLoadError::Schema(format!(
                "emitter '{emitter}' module '{kind}' param '{name}' gradient keys are not sorted"
            )));
        }
    }
    Ok(())
}

fn compile_emitter(
    emitter: &EmitterDef,
    registry: &ModuleRegistry,
) -> Result<CompiledEmitter, VfxRuntimeCompileError> {
    let init_modules = emitter
        .spawn
        .init_modules
        .iter()
        .map(|instance| compile_module(&emitter.name, instance, registry))
        .collect::<Result<Vec<_>, _>>()?;
    let update_modules = emitter
        .update_modules
        .iter()
        .map(|instance| compile_module(&emitter.name, instance, registry))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(CompiledEmitter {
        name: emitter.name.clone(),
        max_particles: emitter.max_particles,
        rate_per_second: emitter.spawn.rate_per_second,
        burst: emitter.spawn.burst.clone(),
        init_modules,
        update_modules,
        renderer: emitter.renderer.clone(),
        loop_mode: emitter.loop_mode,
    })
}

fn compile_module(
    emitter: &str,
    instance: &ModuleInstance,
    registry: &ModuleRegistry,
) -> Result<CompiledModule, VfxRuntimeCompileError> {
    let spec =
        registry
            .get(&instance.kind)
            .ok_or_else(|| VfxRuntimeCompileError::UnknownModule {
                emitter: emitter.to_string(),
                kind: instance.kind.clone(),
            })?;
    let mut params = Vec::with_capacity(spec.params.len());
    for (param_name, expected_type) in spec.params {
        let value = instance
            .param(param_name)
            .ok_or(VfxRuntimeCompileError::MissingParam {
                emitter: emitter.to_string(),
                kind: spec.kind.to_string(),
                param: param_name,
            })?;
        if !value.matches(*expected_type) {
            return Err(VfxRuntimeCompileError::WrongParamType {
                emitter: emitter.to_string(),
                kind: spec.kind.to_string(),
                param: param_name,
            });
        }
        params.push((*param_name, value.clone()));
    }
    Ok(CompiledModule {
        kind: spec.kind,
        params,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn minimal_document() -> VfxSystemDocument {
        VfxSystemDocument {
            id: StableId::new("test_system").unwrap(),
            label: "Test System".into(),
            schema_version: VFX_SCHEMA_VERSION,
            emitters: vec![EmitterDef {
                name: "main".into(),
                max_particles: 64,
                spawn: SpawnDef {
                    rate_per_second: 10.0,
                    burst: vec![BurstDef {
                        time: 0.0,
                        count: 8,
                    }],
                    init_modules: vec![
                        ModuleInstance::new("spawn_lifetime")
                            .with_param("seconds", VfxParam::Float(1.0))
                            .with_param("variance", VfxParam::Float(0.2)),
                        ModuleInstance::new("spawn_velocity_cone")
                            .with_param("direction", VfxParam::Vec3([0.0, 1.0, 0.0]))
                            .with_param("half_angle_degrees", VfxParam::Float(20.0))
                            .with_param("speed", VfxParam::Float(4.0))
                            .with_param("speed_variance", VfxParam::Float(1.0)),
                    ],
                },
                update_modules: vec![
                    ModuleInstance::new("gravity").with_param("strength", VfxParam::Float(9.8)),
                    ModuleInstance::new("color_over_life").with_param(
                        "gradient",
                        VfxParam::Gradient(vec![
                            (0.0, [1.0, 1.0, 0.8, 1.0]),
                            (1.0, [1.0, 0.3, 0.1, 0.0]),
                        ]),
                    ),
                ],
                renderer: RendererDef::Sprite {
                    blend: BlendMode::Additive,
                    billboard: BillboardMode::Camera,
                },
                loop_mode: LoopMode::OneShot {
                    spawn_seconds: 0.05,
                },
            }],
        }
    }

    #[test]
    fn a_minimal_document_validates_and_compiles() {
        let document = minimal_document();
        document.validate_schema().expect("schema is valid");
        let compiled = document
            .compile_runtime(&ModuleRegistry::builtin())
            .expect("compiles");
        assert_eq!(compiled.emitters.len(), 1);
        let emitter = &compiled.emitters[0];
        assert_eq!(emitter.update_modules.len(), 2);
        assert_eq!(emitter.update_modules[0].kind, "gravity");
        assert_eq!(emitter.update_modules[0].float("strength", 0.0), 9.8);
    }

    #[test]
    fn json_round_trips() {
        let document = minimal_document();
        let json = document.to_json_pretty().unwrap();
        let parsed = VfxSystemDocument::parse(&json).unwrap();
        assert_eq!(document, parsed);
    }

    #[test]
    fn an_unknown_module_kind_fails_to_compile_rather_than_silently_no_op() {
        let mut document = minimal_document();
        document.emitters[0]
            .update_modules
            .push(ModuleInstance::new("teleport_particles"));
        let error = document
            .compile_runtime(&ModuleRegistry::builtin())
            .unwrap_err();
        assert!(matches!(
            error,
            VfxRuntimeCompileError::UnknownModule { .. }
        ));
    }

    #[test]
    fn a_missing_required_param_fails_to_compile() {
        let mut document = minimal_document();
        document.emitters[0].update_modules[0].params.clear();
        let error = document
            .compile_runtime(&ModuleRegistry::builtin())
            .unwrap_err();
        assert!(matches!(error, VfxRuntimeCompileError::MissingParam { .. }));
    }

    #[test]
    fn a_wrong_typed_param_fails_to_compile() {
        let mut document = minimal_document();
        document.emitters[0].update_modules[0].params[0].1 = VfxParam::Vec3([1.0, 0.0, 0.0]);
        let error = document
            .compile_runtime(&ModuleRegistry::builtin())
            .unwrap_err();
        assert!(matches!(
            error,
            VfxRuntimeCompileError::WrongParamType { .. }
        ));
    }

    #[test]
    fn schema_rejects_an_empty_emitter_list() {
        let mut document = minimal_document();
        document.emitters.clear();
        assert!(document.validate_schema().is_err());
    }

    #[test]
    fn schema_rejects_unsorted_bursts() {
        let mut document = minimal_document();
        document.emitters[0].spawn.burst = vec![
            BurstDef {
                time: 1.0,
                count: 1,
            },
            BurstDef {
                time: 0.0,
                count: 1,
            },
        ];
        assert!(document.validate_schema().is_err());
    }

    #[test]
    fn schema_rejects_an_unsorted_gradient() {
        let mut document = minimal_document();
        document.emitters[0].update_modules[1].params[0].1 = VfxParam::Gradient(vec![
            (1.0, [0.0, 0.0, 0.0, 0.0]),
            (0.0, [1.0, 1.0, 1.0, 1.0]),
        ]);
        assert!(document.validate_schema().is_err());
    }

    #[test]
    fn curve_sampling_interpolates_and_clamps() {
        let points = vec![(0.0, 0.0), (1.0, 10.0)];
        assert_eq!(sample_curve(&points, -1.0), 0.0);
        assert_eq!(sample_curve(&points, 0.5), 5.0);
        assert_eq!(sample_curve(&points, 2.0), 10.0);
    }

    #[test]
    fn gradient_sampling_interpolates_each_channel() {
        let stops = vec![(0.0, [0.0, 0.0, 0.0, 1.0]), (1.0, [1.0, 0.0, 0.0, 0.0])];
        let mid = sample_gradient(&stops, 0.5);
        assert!((mid[0] - 0.5).abs() < 1e-6);
        assert!((mid[3] - 0.5).abs() < 1e-6);
    }

    #[test]
    fn registry_builtin_covers_the_modules_the_default_catalog_uses() {
        let registry = ModuleRegistry::builtin();
        for kind in [
            "gravity",
            "drag",
            "curl_noise",
            "color_over_life",
            "size_over_life",
            "spawn_position_sphere",
            "spawn_velocity_cone",
            "spawn_lifetime",
            "spawn_color",
            "spawn_size",
        ] {
            assert!(
                registry.get(kind).is_some(),
                "missing builtin module '{kind}'"
            );
        }
    }
}
