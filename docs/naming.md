# Starfall I Naming Guide

Feature status is tracked separately in `docs/archive/game_review_2026-07.md`.

This is the canonical naming reference for story, UI, docs, and future content.
Prefer these names in player-facing text. Internal Rust enum names can remain
stable when changing them would create save or API churn.

## Title

- Game title: `Starfall I`
- Short form: `Starfall`
- First chapter: `Invasion of the Scallarians`

## Core Cast

Wizard Scientists:

- Giacoma
- Giovanni
- Gabrio

Hero Brothers:

- Vincenzo, oldest brother
- Antonio, aka Tony
- Angelo
- Joseph, aka Little Joe

Hero Sisters:

- Gabriella
- Nova
- Aurora
- Fortuna

## Enemy Groups

Scallarians:

- Canon description: space aliens invading Earth from another dimension.
- Use `Scallarian` for the species/faction label.
- Use `Scallarians` for the group.
- Avoid generic player-facing labels such as `space aliens`, `dimension aliens`,
  or `rift aliens` unless a line is intentionally describing them indirectly.

Dragon Royalty and Domains:

- Collosar, King of the Dragons, Tibet
- Tarack, Collosar's wife
- Spikey, their youngest son
- Shread, their oldest son
- Pink Flame, their daughter
- Ragar, Collosar's uncle/brother to the king, Colorado Rockies domain
- Blackskull, Collosar's uncle/brother to the king, Antarctica domain

Dr. Bile and Mirror Humans:

- Dr. Bile
- Zark
- Crush
- Fang
- Sharp

## Chapter Naming

- Chapter 1: `Invasion of the Scallarians`
- Chapter 13: `Scallarian Front`
- Keep chapter names short and readable in chapter select.
- Use villain names in boss titles when possible.

## Implementation Notes

- `Faction::DimensionalAlien` is the legacy/internal enum name for Scallarians.
  Keep it unless a deliberate save/data migration is planned.
- Player-facing labels should call that faction `Scallarian`.
- Chapter text should present villains as active threats first; deeper story can
  come later.
