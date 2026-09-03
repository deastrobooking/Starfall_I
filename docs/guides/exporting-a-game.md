# Exporting a Game

Starfall Forge can export the active project as a standalone native Game
folder. This is the first production-shaped packaging seam: Designer state and
project drafts stay outside the shipped game, while the runtime receives one
explicit published snapshot.

## Export from Project Hub

1. Run the default Designer application with `cargo run`.
2. Open **CREATOR TOOLS** and select or create a project.
3. Save authoring changes, then press **EXPORT GAME**.
4. Keep Forge open until the Project Hub reports `EXPORTED ✓` and the output
   path.

The job runs in the background. Pressing the button again while it is active
does not start a competing build.

## Output contract

Exports are created at:

```text
<project>/build/exports/<project-id>-<os>-<arch>-<timestamp>/
├── <project-id>[.exe]
├── starfall.bundle.json
└── assets/
    ├── ... shipped runtime assets ...
    └── published/
        ├── current.json
        └── generations/<content-hash>/
            ├── weapons.json
            ├── creatures.json
            ├── vehicles.json
            ├── spaceships.json
            ├── dialogues.json
            └── platformer_routes.json
```

`starfall.bundle.json` records schema version 1, project identity, engine
version, host OS and architecture, executable name, asset root, selected
publication generation, and required published files. The application finds
the adjacent `assets/` directory automatically.

The exporter builds this runtime command itself:

```sh
cargo build --release --locked --no-default-features \
  --features heavy-water-demo --bin starfall-i \
  --target-dir <project>/build/cache/game-export-target
```

The isolated target directory prevents an export build from replacing a
currently running Designer executable and is reused to accelerate later
exports.

## Safety and repeatability

- Export first runs the same all-or-nothing project publication used by
  **PUBLISH TO GAME**.
- Shared `assets/published/` history is excluded from the asset copy. Exactly
  the selected generation is installed into the bundle.
- Assembly happens in a hidden staging directory. The public export directory
  appears only after all required files and the bundle manifest exist.
- Existing export folders are immutable and are never overwritten.
- Symlinked assets are rejected so an export cannot accidentally copy files
  from outside the intended asset tree.

## Current boundary

This first exporter targets the machine running Forge and copies the current
shared runtime asset root. It does not yet perform dependency-aware asset
cooking, cross-compilation, signing/notarization, installers, patches, or
automatic product branding. It currently requires the Starfall source checkout
and a working Rust/Cargo toolchain on the export machine. Keep the entire
generated folder together when testing or distributing it.
