# Combat Feel — hitstop, knockback, and hit feedback

How a landed hit becomes *felt*. Two modules own this:
`src/combat/hitstop.rs` (the freeze) and `src/combat/feedback.rs` (everything visible).

## The hit pipeline

```
weapon/melee system
  → DamageInfo::new(dmg, type).with_knockback(f).with_hit_direction(dir)
  → apply_damage(&mut health, &mut damageable, &info)     [damage.rs]
      • resistances (faction-flavoured, set at enemy spawn)
      • toughness   100 / (100 + defense)
      • pending_knockback += dir × force
  → events: EnemyDamagedEvent / CombatImpactEvent / EnemyKilledEvent
```

Readers of those events (all presentation, all in `combat_feedback.rs` unless
noted):

| Effect | Trigger | Mechanism |
|---|---|---|
| Hitstop | `CombatImpactEvent`, kills | `HitstopState.remaining` (max-not-sum, 28–90 ms, drains on `Time<Real>`); sim chains skip via `run_if(hitstop_inactive)` |
| Knockback | `apply_damage` | `Damageable.pending_knockback` drained by `apply_enemy_knockback` (enemy_plugin) as a decaying shove |
| Flinch | `EnemyDamagedEvent` | `Flinch{timer}` inserted; AI/attack queries filter `Without<Flinch>` |
| Hit-flash | `EnemyDamagedEvent` | short unlit emissive orb at impact (no shared-material mutation) |
| Death dissolve | `Added<DeadEnemy>` | shrink-out over the despawn window |
| Damage numbers | `EnemyDamagedEvent` | per-active-camera `world_to_viewport` + viewport offset (split-screen correct), rise/fade, capped at 32 |
| Camera shake | damage + kills | proximity-scaled `CameraShake::add_player_trauma` per player |
| Rumble | same | `trigger_player_rumble` (currently a stub body — wire when Bevy exposes rumble) |
| SFX | 10 events (fire/slash/hit/parry/kill/hurt/loot/chest/level-up/reload) | `src/audio/sfx.rs` bus over the procedural retro palette in `src/audio/synth.rs` — zero external assets, 50 ms per-kind cooldown, deterministic ±5% pitch jitter, obeys `GameSettings.sfx_volume` |

## Rules of thumb

- **Never gate event-consuming reward/cleanup systems on hitstop** — Bevy
  messages live ~2 frames; a 90 ms freeze would drop them. Only *simulation*
  systems take `run_if(hitstop_inactive)`.
- **Hitstop uses `max`, not `+=`** — burst weapons produce one clean pause.
- **Don't scale `Time<Virtual>`** for freezes — it drags UI/camera tweens.
- Feedback spawners carry budgets (flash 24, numbers 32) for 4-player fights.

## Adding new feedback

1. Find (or add) the event in `src/events.rs` — many are already written by
   gameplay and simply need a reader.
2. Add a reader system in `combat_feedback.rs`, register it in
   `CombatFeedbackPlugin` under `in_state(AppState::Playing)`.
3. Presentation systems stay UNgated by hitstop so they animate during the
   pause; cap live entities; despawn on `OnExit(Playing)` if they can persist.

## Frame data (MoveDef)

Melee moves are authored data, not code: `assets/combat/moves.json` holds each
move's `startup` (wind-up) / `active` (strike) / `recovery` phases,
`cancel_after` (seconds into recovery when a buffered next attack may chain),
`damage`, `knockback`, and per-move `hitstop`. Edit + relaunch to retune —
no recompile. Delete the file to restore defaults (they're also written on
first run). Loading/validation: `src/combat/data.rs` (`MoveLibrary`; invalid
files log a warning and fall back to defaults, never soft-lock).

Execution: `melee_combo_system` runs a Startup→Active→Recovery machine per
player; the hit lands at the start of Active; buffered inputs chain during the
cancel window. Chain reach/arc stay per-chain constants in the system.

## Sound

The palette is synthesized at startup (`audio_synth.rs` presets → WAV bytes →
`Assets<AudioSource>`). To retune a sound, edit its `preset_*` recipe
(waveform, sweep, decay, crush) — renders are deterministic and unit-tested.
To replace one with a recorded file later, swap the handle in
`bake_sfx_library`; the event wiring doesn't change.

## Tuning knobs

`hitstop.rs` consts (`HITSTOP_BASE/PER_DAMAGE/KILL/MAX`) · flinch duration ·
knockback decay in `apply_enemy_knockback` · trauma amounts in
`outgoing_hit_shake` (incoming damage shakes harder than outgoing by design).
