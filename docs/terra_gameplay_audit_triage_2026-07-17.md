# Terra Gameplay Audit Triage — July 17, 2026

> Dated review of the “5.6 Terra” Copilot Harness findings. Use
> `current_state.md` and `agent_next_steps.md` for current execution order.

## Decision summary

| Finding | Disposition | Priority |
|---|---|---|
| Projectile world obstruction and target scan | Confirmed | High gameplay correctness |
| Combat outside fixed simulation / unused `GameSet` membership | Confirmed architectural debt | Incremental EC1/EC2 migration |
| No reusable app factory or app-level smoke suite | Confirmed and already first | Immediate engineering guardrail |
| Settlement economy has weak consequences | Partially confirmed product gap | Campaign design slice |
| Party vehicle authority | Partially confirmed; behavior overstated | Product-policy and UX decision |
| Player-facing events lack owner context | Confirmed for several messages | Ownership hardening |
| Armor method naming and dormant poison fields | Confirmed | Small API cleanup plus status design |
| Character Design and Studio are disconnected | Mostly stale | Preserve existing conversion boundary |
| Performance budgets lack committed evidence | Confirmed | Profile before N11c |

## Confirmed findings

### Projectile collision is not world-authoritative

`projectile_update_system` moves each projectile in `Update`, then performs a
manual swept segment-to-point test over every living enemy and, if no enemy was
hit, every road vehicle. It does not query terrain, walls, rails, props, or
dungeon collision and does not compare the nearest target hit with the nearest
world obstruction. Explosions also use distance-only target scans without
world occlusion.

The current cost is proportional to projectiles multiplied by candidate
targets, and an enemy can be damaged through blocking geometry. Aim obstruction
already uses Avian `SpatialQuery`, but projectile travel does not.

First safe slice:

1. Cast the swept projectile segment against world geometry and retain the
   nearest obstruction distance/position.
2. Compare candidate target intersection distance with that obstruction so the
   closest valid impact wins.
3. Explode at the resolved impact position and define whether blast occlusion
   is required by weapon profile.
4. Add wall-before-enemy, high-speed, terrain, explosive-position, and piercing
   regression tests.

Full hurtbox collision layers and physics broad-phase targeting belong to EC2;
they should not be smuggled into the first obstruction fix without a layer and
ownership design.

### Combat timing remains frame-scheduled

The fixed 64 Hz motor and input buffer are shipped, but weapon firing, melee
phase advancement, projectile travel, Sabre work, and hit resolution remain in
`Update`. `GameSet` defines and orders sets only in the `Update` schedule, and
no gameplay system currently opts into those sets.

Most timers use delta time, so this is not proof that combat speed is simply
multiplied by refresh rate. It does mean input/phase resolution is frame
quantized, deterministic replay is unavailable, and motor/combat ordering is
not enforced by the advertised contract. Migrate one state-changing combat
chain at a time, keeping VFX/UI/interpolation in frame schedules, and compare
outcomes across render-frame patterns for the same fixed-tick count.

### App-level regression seam is missing

`main()` still constructs the complete app directly. Unit coverage and CI are
strong, but plugin registration, resource initialization, core transitions,
and schedule contracts lack a reusable smoke boundary. Keep the current plan:
extract an app factory first, add internal/headless app smoke tests, then use it
as the guardrail for module extraction. Creating `/tests` first would not expose
private binary modules by itself.

### Several player-facing messages lack identity

Messages such as heal, death, dodge, level-up, stamina, weapon fire/switch,
loot, and chest-open do not carry `PlayerIndex`, even when their emitter knows
the affected player. Current global SFX can intentionally ignore ownership, but
HUD, viewport feedback, rumble, spatial audio, and future analytics must not
infer it later.

Adopt a rule: messages caused by one player carry a typed player context;
campaign/global messages are explicitly ownerless. Migrate emitters and readers
in vertical slices rather than adding unused fields everywhere at once.

### Armor API naming is misleading

`ArmorSet::calculate_damage_reduction` returns final mitigated incoming damage,
not a reduction amount or ratio. Rename it to communicate the returned value.
`ElementType::poison_dps` and `poison_duration` have no runtime consumers; keep
them only if a typed status-effect contract is scheduled. Negative and zero
damage behavior also needs an explicit contract before broad reuse.

### Performance evidence remains open

F11, Tracy, LOD coverage, terrain patches, and cache limits are present.
Representative one-, two-, and four-camera captures and committed budgets are
not. Record city combat, mountain travel, forest, road, dungeon, and boss-mode
baselines before choosing streaming radii, pooling, or denser content.

## Partial or product-level findings

### Settlement economy

The economy is not consequence-free: builds consume resources and robot parts,
and Defense Outpost, Power Plant, Spaceport, and Research Lab tiers contribute
to raid defense. Terra is correct that most produced resources otherwise feed
future construction and accumulate to a cap; Farm, Factory, Bridge Hub, and
several resource types lack a distinct player-facing unlock or upkeep loop.

Treat this as campaign design, not engine reliability. Give each building one
readable soft-benefit before adding punitive upkeep: route repair, fast-travel
response, research/hacking access, resilience, or a visible settlement action.

### Vehicles

One global ground mode and one global air mode prevent simultaneous independent
forms, but only `active_owner` receives the mode buffs. Boats already implement
driver/passenger seats, late boarding, dismount, stale-passenger cleanup, and
deterministic driver promotion. Remaining gaps are visible ground/air vehicle
bodies, HUD ownership/passenger roles, activation policy, and a deliberate
choice per form: party transformation, rideable shared object, or per-player
capability.

## Mostly stale finding

Character Studio already converts `CharacterSpec` through
`studio_spec_to_blueprint`, stores both the exact Studio recipe and its playable
blueprint on the same player slot, persists both, and tests the conversion.
Those representations have different responsibilities: editable visual source
and gameplay/runtime projection. Preserve and version that conversion boundary;
do not create a third character-authoring path.

## Important correction to Terra's conclusion

Maximum-health recalculation is not fully centralized. The separate follow-up
architecture audit confirmed that authored blueprint health, spawn bonuses,
level-up, save hydration, and armor sync still write or reconstruct effective
maximum health from different bases. The schema-compatible base-stat/derived-cap
work in `current_state.md` remains valid and must not be closed by this review.
