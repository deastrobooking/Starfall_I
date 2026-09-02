# starfall-project

`starfall-project` defines the versioned `starfall.project.toml` and
`starfall.module.toml` contracts used by future project discovery, validation,
scaffolding, upgrades, and Studio workflows.

The crate provides parsing, schema migration, semantic validation, and
deterministic round trips without performing filesystem mutations. This keeps
project inspection safe and makes automation depend on manifests instead of
folder-name guesses.
