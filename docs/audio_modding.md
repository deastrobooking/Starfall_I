# Starfall Player Music And Action SFX

Starfall keeps background music, custom action MP3s, and built-in arcade effects
on separate layers. Custom files are optional; an empty library remains fully
playable and audible.

## Background music

1. Copy MP3 files into `assets/user_music`.
2. Launch Starfall and press `F6`.
3. Press `R` to rescan after adding or removing files.
4. Use Left/Right or D-pad Left/Right to change tracks.
5. Use Space or controller A to pause/resume. `S` toggles shuffle.

Set `STARFALL_MUSIC_DIR=/absolute/path/to/music` before launch to keep personal
music outside the installation. Files are sorted case-insensitively by name.
The deck uses the Settings music slider and master volume; SFX remains active.

## Action sound assignments

Put MP3 one-shots in `assets/user_sfx`, then edit `actions.json`:

```json
{
  "actions": {
    "weapon.fire": "laser_shot.mp3",
    "melee.slash": "energy_slash.mp3",
    "hoverboard.grind_start": "rail_start.mp3"
  }
}
```

Press `F6`, then `R` to hot reload. Set `STARFALL_SFX_DIR` to use an external
folder containing both `actions.json` and its MP3 files.

Built-in IDs are `weapon.fire`, `weapon.reload`, `melee.slash`, `combat.hit`,
`combat.parry`, `combat.kill`, `player.hurt`, `player.level_up`, `reward.loot`,
and `reward.chest`. Modular systems may emit additional IDs through
`ModularActionSfxEvent`. The stunt-road slice currently emits
`hoverboard.spring`, `hoverboard.grind_start`, `hoverboard.grind_exit`, and
`hoverboard.trick_land`.

Action IDs may contain ASCII letters, digits, `.`, `_`, and `-`. MP3 paths must
be relative and cannot contain `..`. Missing, rejected, unreadable, or malformed standard
assignments automatically use Starfall's synthesized arcade effect.

Only use music and sound effects you have permission to play and distribute.
