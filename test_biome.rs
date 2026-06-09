use bevy::prelude::*;
use bevy::post_process::bloom::Bloom;

fn see_if_color_grading_is_prelude() {
    let mut cg = ColorGrading::default();
    cg.exposure = 1.0;
}
