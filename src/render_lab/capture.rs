//! Capture a held final pose after timing has ended, without readback overhead
//! in the measured interval. Reports and images both use exclusive creation.

use bevy::{
    app::AppExit,
    prelude::*,
    render::view::screenshot::{Screenshot, ScreenshotCaptured},
};

use super::{LabConfig, LabState};

pub(crate) fn capture_finished_run(
    mut commands: Commands,
    config: Res<LabConfig>,
    state: Res<LabState>,
    mut waited: Local<u32>,
    mut exit: MessageWriter<AppExit>,
) {
    let Some(path) = config.capture.clone().filter(|_| state.finished) else {
        return;
    };
    if *waited == 0 {
        let capture_config = config.clone();
        commands.spawn(Screenshot::primary_window()).observe(
            move |capture: On<ScreenshotCaptured>, mut exit: MessageWriter<AppExit>| {
                let result = (|| {
                    let image = capture
                        .image
                        .clone()
                        .try_into_dynamic()
                        .map_err(|error| error.to_string())?;
                    let mut bytes = std::io::Cursor::new(Vec::new());
                    image
                        .write_to(&mut bytes, image::ImageFormat::Png)
                        .map_err(|error| error.to_string())?;
                    super::report::write_new(&path, bytes.get_ref())
                        .and_then(|()| validate_capture(&image, &capture_config))
                })();
                match result {
                    Ok(()) => {
                        println!("Capture {}", path.display());
                        exit.write(AppExit::Success);
                    }
                    Err(error) => {
                        error!("Capture failed: {error}");
                        exit.write(AppExit::error());
                    }
                }
            },
        );
    }
    *waited += 1;
    if *waited == 600 {
        error!("Capture timed out; timing report remains available");
        exit.write(AppExit::error());
    }
}

// A transport/content sanity check for these known non-uniform fixtures. This
// rejects a successful readback of a blank image; it does not certify parity.
fn validate_capture(image: &image::DynamicImage, config: &LabConfig) -> Result<(), String> {
    if image.width() != config.width || image.height() != config.height {
        return Err("Capture dimensions differ from the requested physical window".into());
    }
    let rgb = image.to_rgb8();
    for (view, rect) in
        super::config::view_rects(config.views, UVec2::new(config.width, config.height))
            .into_iter()
            .enumerate()
    {
        let first = rgb.get_pixel(rect.min.x, rect.min.y);
        let varied = (rect.min.y..rect.max.y).any(|y| {
            (rect.min.x..rect.max.x).any(|x| {
                rgb.get_pixel(x, y)
                    .0
                    .iter()
                    .zip(first.0)
                    .any(|(a, b)| a.abs_diff(b) > 8)
            })
        });
        if !varied {
            return Err(format!(
                "Capture view {view} is blank/uniform; artifacts retained for diagnosis"
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_requested_view_must_contain_nonuniform_content() {
        let config = LabConfig::parse(
            [
                "--views",
                "4",
                "--width",
                "320",
                "--height",
                "240",
                "--output",
                "unused.json",
            ]
            .map(String::from),
        )
        .unwrap();
        let mut pixels = image::RgbImage::new(320, 240);
        assert!(
            validate_capture(&image::DynamicImage::ImageRgb8(pixels.clone()), &config).is_err()
        );
        for (x, y) in [(10, 10), (170, 10), (10, 130)] {
            pixels.put_pixel(x, y, image::Rgb([255, 255, 255]));
        }
        assert!(
            validate_capture(&image::DynamicImage::ImageRgb8(pixels.clone()), &config).is_err()
        );
        pixels.put_pixel(170, 130, image::Rgb([255, 255, 255]));
        assert!(validate_capture(&image::DynamicImage::ImageRgb8(pixels), &config).is_ok());
        assert!(validate_capture(&image::DynamicImage::new_rgb8(1, 1), &config).is_err());
    }
}
