# Custom Action Sound Effects

Place custom `.mp3` one-shots here and assign them in `actions.json`. Open the
Music Deck with `F6` and press `R` to reload music and action sounds.

The built-in procedural arcade effects remain the fallback for every standard
action whose MP3 is missing, invalid, or unassigned.

An external folder can be used with `STARFALL_SFX_DIR`.

Built-in action IDs:

- `weapon.fire`
- `weapon.reload`
- `melee.slash`
- `combat.hit`
- `combat.parry`
- `combat.kill`
- `player.hurt`
- `player.level_up`
- `reward.loot`
- `reward.chest`
- `hoverboard.spring`
- `hoverboard.grind_start`
- `hoverboard.grind_exit`
- `hoverboard.trick_land`

Forge/runtime modules may emit additional validated IDs through
`ModularActionSfxEvent`.
