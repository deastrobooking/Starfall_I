"""Guard the minimal facade and its consumer against native runtime dependencies."""

import pathlib
import subprocess


ROOT = pathlib.Path(__file__).resolve().parents[1]
NATIVE_PACKAGES = {
    "avian3d",
    "bevy",
    "bevy_audio",
    "bevy_dev_tools",
    "bevy_feathers",
    "bevy_render",
    "bevy_winit",
    "cpal",
    "image",
    "rodio",
    "wgpu",
    "winit",
}


def check(package, *feature_args):
    result = subprocess.run(
        [
            "cargo", "tree", "--locked", "-p", package,
            "--edges", "normal,build", "--prefix", "none", "--format", "{p}",
            *feature_args,
        ],
        cwd=ROOT,
        check=False,
        capture_output=True,
        text=True,
    )
    if result.returncode:
        raise SystemExit(result.stderr)
    packages = {line.split()[0] for line in result.stdout.splitlines() if line.strip()}
    unexpected = packages & NATIVE_PACKAGES
    if unexpected:
        raise SystemExit(f"{package} pulls native runtime packages: {sorted(unexpected)}")
    label = " ".join([package, *feature_args])
    print(f"{label}: {len(packages)} packages, no native runtime")


if __name__ == "__main__":
    check("starfall-i", "--no-default-features")
    # Build modifiers must not silently activate the native runtime.
    check("starfall-i", "--no-default-features", "--features", "dynamic,tracy")
    # Select the consumer alone: --workspace would unify the demo's features.
    check("starfall-framework-consumer")
