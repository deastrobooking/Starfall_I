use std::{collections::BTreeSet, path::PathBuf};

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

pub const USAGE: &str = "Starfall rendering lab
  --scene geometry|lighting       Deterministic test scene (default geometry)
  --renderer pbr|meshlets         Requested backend (default pbr); fallback is reported
  --shadows on|off               Shadow maps (default on); isolate geometry from shadows
  --meshlet-precision 4          Quantization exponent, 1..16 (units: 1/2^n centimeters)
  --views 1|2|4                   Cameras sharing one fixed-size window (default 1)
  --width 1280 --height 720       Physical window pixels
  --grid 8                       Geometry grid side, 1..32
  --warmup 120 --frames 300       Warmup and measured frame counts
  --validate-probes              Compare bounded GPU voxel rays with the CPU oracle first
  --capture path.png             Optional final-pose screenshot after measurement
  --output path.json             Required; existing reports are never overwritten

Example: cargo run --release --no-default-features --features render-lab \
--example render_lab -- --views 4 --output target/render-lab/four-views.json";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LabScene {
    Geometry,
    Lighting,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LabRenderer {
    #[default]
    Pbr,
    Meshlets,
}

/// Bounded, explicit inputs recorded alongside every measurement.
#[derive(Resource, Debug, Clone, Serialize, Deserialize)]
pub struct LabConfig {
    pub scene: LabScene,
    #[serde(default)]
    pub renderer: LabRenderer,
    #[serde(default = "default_shadows")]
    pub shadows: bool,
    #[serde(default = "default_meshlet_precision")]
    pub meshlet_precision: u8,
    pub views: u32,
    pub width: u32,
    pub height: u32,
    pub grid: u32,
    pub warmup_frames: u32,
    pub measured_frames: u32,
    #[serde(default)]
    pub validate_probes: bool,
    #[serde(default)]
    pub capture: Option<PathBuf>,
    pub output: PathBuf,
}

impl LabConfig {
    pub fn parse(args: impl IntoIterator<Item = String>) -> Result<Self, String> {
        let mut config = Self {
            scene: LabScene::Geometry,
            renderer: LabRenderer::Pbr,
            shadows: default_shadows(),
            meshlet_precision: default_meshlet_precision(),
            views: 1,
            width: 1280,
            height: 720,
            grid: 8,
            warmup_frames: 120,
            measured_frames: 300,
            validate_probes: false,
            capture: None,
            output: PathBuf::new(),
        };
        let mut args = args.into_iter();
        let mut seen = BTreeSet::new();
        while let Some(flag) = args.next() {
            if !seen.insert(flag.clone()) {
                return Err(format!("Repeated option: {flag}"));
            }
            if flag == "--validate-probes" {
                config.validate_probes = true;
                continue;
            }
            let value = args
                .next()
                .ok_or_else(|| format!("Missing value for {flag}"))?;
            let number = || {
                value
                    .parse::<u32>()
                    .map_err(|_| format!("Invalid integer for {flag}"))
            };
            match flag.as_str() {
                "--shadows" => {
                    config.shadows = match value.as_str() {
                        "on" => true,
                        "off" => false,
                        _ => return Err("Shadows must be on or off".into()),
                    }
                }
                "--meshlet-precision" => {
                    config.meshlet_precision = u8::try_from(number()?)
                        .map_err(|_| "Meshlet precision must be within 1..16")?
                }
                "--renderer" => {
                    config.renderer = match value.as_str() {
                        "pbr" => LabRenderer::Pbr,
                        "meshlets" => LabRenderer::Meshlets,
                        _ => return Err("Renderer must be pbr or meshlets".into()),
                    }
                }
                "--scene" => {
                    config.scene = match value.as_str() {
                        "geometry" => LabScene::Geometry,
                        "lighting" => LabScene::Lighting,
                        _ => return Err("Scene must be geometry or lighting".into()),
                    }
                }
                "--views" => config.views = number()?,
                "--width" => config.width = number()?,
                "--height" => config.height = number()?,
                "--grid" => config.grid = number()?,
                "--warmup" => config.warmup_frames = number()?,
                "--frames" => config.measured_frames = number()?,
                "--output" => config.output = value.into(),
                "--capture" => config.capture = Some(value.into()),
                _ => return Err(format!("Unknown option: {flag}")),
            }
        }
        config.validate()?;
        Ok(config)
    }

    pub fn validate(&self) -> Result<(), String> {
        if !matches!(self.views, 1 | 2 | 4) {
            return Err("Views must be 1, 2, or 4".into());
        }
        if !(320..=3840).contains(&self.width) || !(240..=2160).contains(&self.height) {
            return Err("Resolution must be within 320..3840 by 240..2160".into());
        }
        if !(1..=32).contains(&self.grid)
            || !(1..=16).contains(&self.meshlet_precision)
            || !(1..=3600).contains(&self.warmup_frames)
            || !(1..=10_000).contains(&self.measured_frames)
        {
            return Err("Grid or frame count exceeds the lab's bounded workload".into());
        }
        if self.output.as_os_str().is_empty() || self.output.file_name().is_none() {
            return Err("Supply --output with a new JSON report filename".into());
        }
        if self.capture.as_ref().is_some_and(|path| {
            path.extension().is_none_or(|extension| extension != "png") || path == &self.output
        }) {
            return Err("Capture must have a .png filename distinct from the report".into());
        }
        Ok(())
    }
}

fn default_shadows() -> bool {
    true
}

fn default_meshlet_precision() -> u8 {
    4
}

/// Tile the full pixel extent, including odd resolutions, without gaps/overlap.
pub(crate) fn view_rects(views: u32, size: UVec2) -> Vec<URect> {
    let columns = if views == 1 { 1 } else { 2 };
    let rows = if views == 4 { 2 } else { 1 };
    (0..views)
        .map(|index| {
            let x = index % columns;
            let y = index / columns;
            URect::from_corners(
                UVec2::new(size.x * x / columns, size.y * y / rows),
                UVec2::new(size.x * (x + 1) / columns, size.y * (y + 1) / rows),
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<LabConfig, String> {
        LabConfig::parse(args.iter().map(|arg| (*arg).into()))
    }

    #[test]
    fn rejects_unbounded_or_ambiguous_runs() {
        for args in [
            vec!["--views", "3"],
            vec!["--frames", "0"],
            vec!["--warmup", "0"],
            vec!["--grid", "1000000"],
            vec!["--width", "0"],
            vec!["--views", "2", "--views", "4"],
            vec!["--typo", "1"],
            vec!["--renderer", "unknown"],
            vec!["--capture", "image.jpg"],
            vec!["--shadows", "yes"],
            vec!["--meshlet-precision", "0"],
            vec!["--meshlet-precision", "17"],
            vec!["--meshlet-precision", "256"],
        ] {
            let mut args = args;
            args.extend(["--output", "report.json"]);
            assert!(parse(&args).is_err());
        }
        assert!(parse(&[]).is_err());
        assert_eq!(
            parse(&["--output", "report.json", "--views", "4"])
                .unwrap()
                .views,
            4
        );
    }

    #[test]
    fn renderer_selection_and_legacy_config_default_are_explicit() {
        let config = parse(&["--renderer", "meshlets", "--output", "report.json"]).unwrap();
        assert_eq!(config.renderer, LabRenderer::Meshlets);
        let mut legacy = serde_json::to_value(config).unwrap();
        legacy.as_object_mut().unwrap().remove("renderer");
        legacy.as_object_mut().unwrap().remove("capture");
        let restored: LabConfig = serde_json::from_value(legacy).unwrap();
        assert_eq!(restored.renderer, LabRenderer::Pbr);
        assert!(restored.capture.is_none());
        assert!(parse(&["--output", "same.png", "--capture", "same.png"]).is_err());
    }

    #[test]
    fn all_layouts_cover_odd_pixel_extents_exactly_once() {
        let size = UVec2::new(321, 241);
        for views in [1, 2, 4] {
            let rects = view_rects(views, size);
            let mut pixels = vec![0; (size.x * size.y) as usize];
            for rect in rects {
                for y in rect.min.y..rect.max.y {
                    for x in rect.min.x..rect.max.x {
                        pixels[(y * size.x + x) as usize] += 1;
                    }
                }
            }
            assert!(pixels.iter().all(|count| *count == 1));
        }
    }
}
