# Starfall runtime foundations

The 2026-10-02 engine review used
[`vladtrc/iw4L` at `dc1ea70`](https://github.com/vladtrc/iw4L/tree/dc1ea70cfe91cb600c68ff2d6739a1583dceff3f)
as a technical reference. The implementation direction is Starfall-owned
systems designed around Starfall's requirements, rather than source ports.
The initial adapted budget/history implementations were discarded before
completion. No IW4L source, dependency, license declaration, bundled notice,
fonts, game assets, or export-packaging additions remain in this change.
Upstream source was read during research; this is not a clean-room provenance
claim or a new licensing decision for Starfall.

## Local design requirements

- Account for pending deferred ECS spawns as well as live sound entities.
- Obtain consistent live/peak/refusal counters from one shared capacity ledger.
- Release capacity when a pending command is discarded or an entity despawns.
- Preallocate four-player diagnostic history and retain chronological iteration
  when the circular storage wraps.
- Bound retained entries and payload sizes; reject bad entries before eviction.
- Resolve input before the motor, capture afterward, and clear stale ownership.
- Keep services free of Bevy, graphics, audio, and game-content dependencies.

The resource service uses an instance-owned mutex ledger with remaining
capacity, instead of global atomic memory counters. The timeline uses a
preallocated circular slot array, instead of a growable replay deque. Frame
statistics and native adapters are Starfall implementations.

## Delivered

| Capability | Starfall implementation | Behavior |
|---|---|---|
| Resource reservation | `starfall-runtime::budget` | Synchronized capacity ledger, oversize rejection, RAII release, coherent live/peak/refused counters; independent budgets for separate consumers |
| Bounded tick history | `starfall-runtime::history` | Preallocated circular storage, strict count and reported payload-byte limits; oldest eviction, monotonic ticks, explicit reset, oversize rejection without losing existing history |
| Input flight recorder | `engine::flight_recorder` | Up to 2,048 Playing ticks (32 seconds of active ticks at 64 Hz), 2 MiB payload ceiling, four explicit player slots, input commands and observed transforms |
| Fixed input ordering | `engine::input_buffer`, `engine::game_loop` | Tick count in FixedFirst, input resolution in FixedPreUpdate, motor in FixedUpdate, capture in FixedPostUpdate |
| Input lifecycle | `engine::input_buffer` | Drop vanished slots, clear on leaving Playing, consume optional selectors once across catch-up ticks |
| Frame diagnostics | `starfall-runtime::stats`, `engine::game_loop` | F11 shows nearest-rank p95/p99/max over 600 real wall-frame intervals; invalid/zero samples ignored |
| Sound admission | `audio::sfx` | Shared 32-voice cap for imported and synthesized one-shot action/combat sounds; reserve before deferred spawn, release at despawn or leaving Playing; F11 live/peak/refused counters |

The native foundation registers the recorder in Designer and Game profiles.
Designer **F12** exports newline-delimited JSON into the platform data directory
`starfall_i/input-diagnostics/`, retaining the newest five clips. It writes a
temporary file, flushes and syncs it, then renames the completed clip. Export is
an explicit synchronous developer operation and can cause a frame spike. The
header identifies schema version, engine version, seed, fixed rate, and
`playable_replay: false`. F11 shows the retained tick count. A new chapter,
menu/player-selection session, or play-mode switch clears the clip; pause,
game-over, and victory preserve it. Tick numbers may have gaps across pauses.

Observed transforms are captured after fixed gameplay systems but before the
existing per-frame Avian motion flush. The recorder does not save the world,
physics state, RNG stream positions, progression, AI, or render-frame commands.
It cannot reproduce a whole match or serve as network authority. Combat/AI
fixed-step extraction and complete world snapshot/restore are prerequisites
for deterministic replay. Legacy-motor samples are explicitly marked.

The voice cap covers the existing SFX bus, not music, dialogue, or independently
spawned ambient audio. It rejects new voices when full; it does not steal or
prioritize existing voices, and it does not impose a decoded-audio memory cap.
Procedural fallback sounds, custom MP3s, cooldowns, and pitch jitter remain.
No saved progression format changes are introduced.

## Further engine work informed by the reference review

| IW4L subsystem / source | Assessment for Starfall | Required next integration |
|---|---|---|
| `sim`, `net`, `replay` / `docs/SIM-STEP.md` | Shared authoritative step, ordered commands, snapshot adoption, prediction and replay are useful designs; implementation owns IW playerstate/GSC | Finish fixed combat/AI/physics seam and define complete Starfall snapshots before adding playback, rollback, remote multiplayer, or spectator cameras |
| `bots` / `docs/BOTS.md` | Utility tasks, observation memory, resumable A*, shared collision quotas are promising | Adapt to Avian and existing enemy/companion ownership; preserve distinct outcomes for visible, tested-invisible, and query-denied; validate navigation against generated geometry |
| `movement_iw4`, `clipmap_iw4`, `trace_iw4` | IW units, BSP hulls, stance and shooter movement conflict with Starfall traversal and Avian | Keep existing motor; evaluate individual stepping/grounding algorithms only against current controller/traversal acceptance |
| `render_*`, `dpvs_iw4`, `lighting_iw4` / `docs/RENDER.md` | Sorted draw lists, revision-based uploads, visibility and residency budgets are useful; upstream GPU adapter rejects multiple Camera3d views | Prototype in render-lab, measure one/two/four views and retain Bevy material/Forge contracts; no renderer replacement in this change |
| `d3d9_decl`, `d3d9_sm3`, `d3d9_state` | D3D9-to-WGSL support is useful only for a legacy shader importer | Add an explicit import requirement and semantic validation suite first; current Starfall shaders are native WGSL |
| `asset_*`, `fastfile_*` | Intermediate assets, bounded decode, progress and cache separation fit the cooking roadmap; actual parsers target IW4/IW5/T5 | Extract neutral importer/cook contracts; no Call of Duty content dependency added |
| `asset_transport/artifact_cache.rs` | Single-flight cooking and atomic cache publication are useful | Integrate with Forge's immutable generation publishing; preserve path validation, writer ownership and cache invalidation contracts |
| `audio` | Capacity service implemented locally; channel priorities, attenuation curves, mix scopes and streaming remain useful | Extend existing three-layer audio bus with measured limits; do not replace music/custom-SFX/procedural fallbacks |
| `perf`, `diag` | Rolling frame distributions implemented locally; span tables, CPU coverage and benchmark phase accounting remain useful | Extend render-lab/Tracy with measured CPU/GPU attribution, avoiding false per-frame GPU claims |
| `anim_iw4`, `render_anim`, `xmodel_runtime` | Packed clips, pose sharing and animation LOD are format-bound | Build adapters to imported GLB and Starfall rigs; preserve character recipes and split-screen ownership |
| `fx`, `fx_iw4`, `marks_iw4` | Lifetime budgets, batching and surface marks are useful | Adapt to existing VFX graphs/materials, verify visual parity and four-camera budgets |
| `weapon_iw4`, `gamemode_iw4`, `killcam_iw4` | Useful state-machine examples, strongly coupled to shooter rules | Retain family-friendly energy weapons and current combat/progression; choose individual mechanics through Starfall contracts |
| `console`, `hud`, `ui`, `session` | Typed command routing and session lifecycle can inform tools | Extend current controller-first Forge/menu/message infrastructure; no replacement UI, fonts, or account system imported |
| `master_protocol`, `iw4l-master`, `updater` | Hosting, relays, browsing and updates are separate deployment products | Require an actual online/distribution architecture; not dependencies of this local multiplayer game |

No original game data, maps, characters, audio, branding, or bundled fonts are
included. The reference review informs requirements for future Starfall systems;
it is not authorization to import upstream implementations or content.
Starfall's engine versions remain pinned.

## Verification

Core tests cover concurrent admission, release, overflow, both history limits,
invalid ordering, oversize rejection, percentile spikes and window eviction.
Native tests cover a burst before deferred spawn, entity cleanup, master mute,
fixed-schedule ordering, pause behavior, explicit player slots, exported schema,
and single-use input edges/selectors. The standalone
framework consumer exercises all three new public modules without native
dependencies. Run the profile matrix in [verification](guides/verification.md).
