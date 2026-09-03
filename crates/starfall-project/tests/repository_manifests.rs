use starfall_project::{ModuleManifest, ProjectManifest};

#[test]
fn repository_project_manifest_is_current_and_valid() {
    let manifest = ProjectManifest::parse(include_str!("../../../starfall.project.toml")).unwrap();
    assert_eq!(
        manifest.to_toml_pretty().unwrap().lines().next(),
        Some("schema_version = 1")
    );
    assert!(manifest.validate().is_empty());
}

#[test]
fn extracted_crate_manifests_are_current_and_valid() {
    for source in [
        include_str!("../../starfall-graph/starfall.module.toml"),
        include_str!("../../starfall-platformer/starfall.module.toml"),
        include_str!("../../starfall-platformer-graph/starfall.module.toml"),
        include_str!("../starfall.module.toml"),
    ] {
        let manifest = ModuleManifest::parse(source).unwrap();
        assert!(manifest.validate().is_empty());
        assert_eq!(
            ModuleManifest::parse(&manifest.to_toml_pretty().unwrap()).unwrap(),
            manifest
        );
    }
}
