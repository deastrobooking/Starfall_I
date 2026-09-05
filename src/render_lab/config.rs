use std::{collections::BTreeSet, path::PathBuf};

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

pub const USAGE: &str = "Starfall rendering lab
  --scene geometry|lighting       Deterministic test scene (default geometry)
  --views 1|2|4                   Cameras sharing one fixed-size window (default 1)
  --width 1280 --height 720       Physical window pixels
  --grid 8                       Geometry grid side, 1..32
  --warmup 120 --frames 300       Warmup and measured frame counts
  --validate-probes              Compare bounded GPU voxel rays with the CPU oracle first
  --output path.json             Required; existing reports are never overwritten

Example: cargo run --release --no-default-features --features render-lab \
--example render_lab -- --views 4 --output target/render-lab/four-views.json";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LabScene {
    Geometry,
    Lighting,
}

/// Bounded, explicit inputs recorded alongside every measurement.
#[derive(Resource, Debug, Clone, Serialize, Deserialize)]
pub struct LabConfig {
    pub scene: LabScene,
    pub views: u32,
    pub width: u32,
    pub height: u32,
    pub grid: u32,
    pub warmup_frames: u32,
    pub measured_frames: u32,
    #[serde(default)]
    pub validate_probes: bool,
    pub output: PathBuf,
}

impl LabConfig {
    pub fn parse(args: impl IntoIterator<Item = String>) -> Result<Self, String> {
        let mut config = Self {
            scene: LabScene::Geometry,
            views: 1,
            width: 1280,
            height: 720,
            grid: 8,
            warmup_frames: 120,
            measured_frames: 300,
            validate_probes: false,
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
            || !(1..=3600).contains(&self.warmup_frames)
            || !(1..=10_000).contains(&self.measured_frames)
        {
            return Err("Grid or frame count exceeds the lab's bounded workload".into());
        }
        if self.output.as_os_str().is_empty() || self.output.file_name().is_none() {
            return Err("Supply --output with a new JSON report filename".into());
        }
        Ok(())
    }
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
