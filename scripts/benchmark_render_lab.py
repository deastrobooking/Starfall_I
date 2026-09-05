#!/usr/bin/env python3
"""Run comparable release fixtures sequentially; preserve reports and captures."""

import argparse
import json
from pathlib import Path
import subprocess


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=Path("target/release/examples/render_lab"))
    parser.add_argument("--output", type=Path, required=True, help="New evidence directory")
    parser.add_argument("--warmup", type=int, default=120)
    parser.add_argument("--frames", type=int, default=300)
    parser.add_argument("--shadows", choices=("on", "off"), default="on")
    parser.add_argument("--timeout", type=int, default=600, help="Maximum seconds per case")
    args = parser.parse_args()
    if not 1 <= args.warmup <= 3600 or not 1 <= args.frames <= 10000:
        parser.error("warmup/frames exceed the lab workload bounds")
    if not 1 <= args.timeout <= 3600:
        parser.error("timeout must be within 1..3600 seconds")
    binary = args.binary.resolve()
    if not binary.is_file():
        parser.error("Build the render-lab-meshlets release example first")
    try:
        args.output.mkdir(parents=True, exist_ok=False)
    except OSError as error:
        parser.error(str(error))

    results = []
    identity = None
    for scene, renderer in (("geometry", "pbr"), ("geometry", "meshlets"), ("lighting", "pbr")):
        for views in (1, 2, 4):
            name = f"{scene}-{renderer}-{views}"
            report_path = args.output / f"{name}.json"
            command = [
                str(binary), "--scene", scene, "--renderer", renderer,
                "--views", str(views), "--shadows", args.shadows,
                "--warmup", str(args.warmup), "--frames", str(args.frames),
                "--validate-probes", "--output", str(report_path),
                "--capture", str(args.output / f"{name}.png"),
            ]
            with (args.output / f"{name}.log").open("x") as log:
                try:
                    run = subprocess.run(command, stdout=log, stderr=subprocess.STDOUT, check=False, timeout=args.timeout)
                    exit_code = run.returncode
                except subprocess.TimeoutExpired:
                    exit_code = 124
                    log.write(f"\nCase exceeded {args.timeout} seconds; child stopped.\n")
            row = {"name": name, "command": command, "exit_code": exit_code}
            if exit_code == 0:
                report = json.loads(report_path.read_text())
                backend = report["geometry_backend"]
                current_identity = (
                    report["lab_source_fingerprint"], report["device"]["adapter"],
                    report["device"]["backend"], report["view_composition"],
                    backend["meshlet_support_compiled"],
                )
                identity = identity or current_identity
                row.update(
                    actual_renderer=backend["active"],
                    fallback_reason=backend["fallback_reason"],
                    mean_ms=report["frame_time_ms"]["mean"],
                    p95_ms=report["frame_time_ms"]["p95"],
                    comparable=(
                        current_identity == identity
                        and backend["active"] == renderer
                        and backend["meshlet_support_compiled"]
                        and not report["debug_assertions"]
                        and report["probe_validation"]["passed"]
                    ),
                )
            results.append(row)
            # This summary belongs exclusively to the newly created directory.
            (args.output / "summary.json").write_text(json.dumps(results, indent=2) + "\n")
            print(json.dumps(row), flush=True)
            if exit_code or not row.get("comparable", False):
                return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
