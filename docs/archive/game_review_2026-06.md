# Full-Game Review — 2026-06 (city physics · gameplay upgrades · missing logic)

Historical review. See `current_state.md` for current status and
`game_review_2026-07.md` for the later four-track snapshot.

Three parallel audits of the whole game (post Bevy 0.19 + Avian 0.7 migration),
plus the fixes landed the same session. Companion to `docs/engine_roadmap.md`
(`EC#`) and the `M#`/`MM#` roadmaps.

## 1. City "invisible ceilings / can't jump" — diagnosis

Decisive context: physics moved from Rapier to **Avian 0.7** behind the
hand-rolled compat shim in `src/physics.rs`; that shim is the newest,
least-tested code and produced both symptoms. Ruled out: kill volumes, Y-clamps,
zone-based jump gates (none exist).

| # | Root cause | Status |
|---|---|---|
| 1 | **Grounding probe re-latched `grounded` right after takeoff.** The probe cast the whole capsule down by `snap_to_ground` (0.28) even while ascending; at 120–144 Hz a jump's first frame rises < 0.28, so the player was instantly re-grounded, coyote re-latched, and the jump was swallowed. | **FIXED** — `src/physics.rs::character_grounded` now takes `rising`; ascending characters probe only a thin skin (`offset.max(0.02)`). |
| 2 | **Scaled road/bridge deck colliders (fragile divide-then-rescale).** Six spawns combine `.with_scale(..)` with world-unit colliders and rely on `world_space_collider_scale()` (`world_plugin.rs:76`) dividing extents so Avian can re-multiply them. Any axis mismatch (incl. the `radial_scale = max(x,z)` cylinder path, `physics.rs:136-185`) inflates decks into invisible street-level blockers. Applied by hand to only 6 sites — inconsistency-prone. | **OPEN (planned fix below)** |
| 3 | **Overhead city lattice vs. jump apex.** Elevated highways at y≈6–8 + boost roads + sky bridges, while jump apex ≈ 8 units — street jumps smack unseen thin decks, reading as "invisible ceilings". | **OPEN (design tuning)** |

**Planned fix for #2 (next session):** bake colliders at final world dimensions on
an *unscaled* child entity (or force Avian collider scale to 1) so collider size
never depends on rescale timing; then walk a city with the F9 collider-AABB
debug overlay to verify visual == collider. For #3: raise elevated-deck
clearance above max jump apex on street-crossing spans.

Also noted: the motor never zeroes `velocity.y` on ceiling hits (player "pins"
against decks). Fold into EC2/EC3 motor work.

## 2. Missing / dead game logic (audit)

**Fixed this session (were CRITICAL — mechanics silently doing nothing):**
- **Damage resistances** were never populated → all 10 `DamageType`s hit
  identically. Now populated per faction/type at enemy spawn
  (`enemy_plugin.rs::enemy_damageable`): DragonRoyalty resists Fire 0.6 /
  Melee 0.15; DragonExile Fire 0.3 / Laser 0.25; CorruptedHuman Rift 0.4;
  WizardScientist Electric 0.5; Scallarian default Plasma 0.25; drone chassis
  +Kinetic 0.3.
- **Enemy `defense` stat was ignored.** `Damageable.defense` added;
  `apply_damage` scales by `100/(100+defense)`; populated from
  `EnemyConfig.defense` at spawn.
- **Knockback was authored but never applied** (`DamageInfo.knockback_force`,
  `EnemyConfig.knockback_force`, combo-table values all dead). Now:
  `apply_damage` accumulates `Damageable.pending_knockback` from
  `knockback_force × hit_direction`; `enemy_plugin::apply_enemy_knockback`
  drains it as a decaying shove (~0.25 s). Wired at projectile hits (2.2),
  explosions (distance-falloff 1.0–5.5), light/heavy melee combos (their
  authored table values, finally used), and sabre slashes (3.0).
- **F9 double-binding** (perf overlay vs collider debug) — perf overlay moved
  to **F11**.

**Still open (MODERATE):**
- ~16 events written but never read (`WeaponFiredEvent`, `ComboHitEvent`,
  `PlayerParryEvent`, `LootCollectedEvent`, …) — these are the natural hook
  points for the audio/feedback work below.
- `UiDamageNumberEvent` registered with zero writers and readers.
- `PlayerState::Stunned` never entered; `DamageResult.was_blocked/was_parried`
  hardcoded false; `trigger_player_rumble` stub with no callers; `SwapPart`
  message has a reader but no writer.
- Player does not yet receive knockback (enemies→player drain system pending;
  belongs with EC2 hit-reactions).
- GLBs in `assets/Characters/` are reference-only by design (documented in
  `player_mesh.rs`); `assets/voice/` wiring is complete, mp3 files absent by
  intent.

## 3. Top gameplay upgrades (ranked, from the design review)

The world itself is dense (caves, temples, dragon lairs, traversal courses,
puzzles); the gaps are almost entirely **combat feel and guidance**:

1. **Combat is silent** — one `AudioPlayer` call in the whole game (dialogue).
   Hook SFX + combat music to the existing unread events. (M)
2. **Enemies don't react to hits** — flinch/stagger + hit-flash + hitstop on
   kills. Knockback now exists (this session); stagger next. (M)
3. **Boss variety** — only `DragonBoss` has real phase logic; alien/human bosses
   are stat-inflated melee bruisers. Add 2–3 faction boss controllers. (L)
4. **No death effects** — enemies blink out after 2 s; add burst + dissolve. (S–M)
5. **Damage numbers/hitmarkers** — plumbing half-exists (`UiDamageNumberEvent`);
   emit + render. (S)
6. **Loot doesn't scale** — same 5-item table for a drone and a boss; shop is a
   stub ("purchase wiring next"). Tier drops, make shop buy. (M)
7. **No outgoing-hit camera shake** — trauma system exists, only wired for
   incoming damage. (S)
8. **Onboarding** — single wall-of-text; add just-in-time prompts on
   unlock/first-context for grapple/jetpack/sabre. (M)
9. **HUD gaps** — no minimap/objective markers; special-weapon cooldowns
   tracked but invisible. (M)
10. **Enemy archetype variety** — 6 types collapse to ~3 behaviors; add
    shielded/charger/exploder/support reusing existing VFX helpers. (L)

## 4. Character design system (Mœbius × Secret of Mana pass)

`src/player_mesh.rs` styling on top of the GLB-profile builder:
- **Silhouette:** global elegance constants — legs +14%, arms +7%, limb
  cross-section −10%, head −7% (≈7.5 head-heights); `TORSO_CENTER_Y` 0.65→0.68
  keeps feet on the capsule bottom.
- **Palette:** indigo-shaded under-suit (shadows go blue-violet, not black),
  cream-lifted trim/accent, warm brass metal, hero hue identity preserved.
- **Shading:** `cloth_mat` — very matte (roughness 0.92, reflectance 0.06) with
  a lifted emissive floor (0.22) for flat luminous colour; local to heroes.
- **Visual verification still pending** — screen capture blocked by macOS
  permission; verify in the character-design screen or gallery next session.

## 5. Session verification

`cargo check` clean · 127/127 tests pass · 20 s autostart boot with zero panics
on Bevy 0.19 + Avian 0.7.
