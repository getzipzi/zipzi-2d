#!/usr/bin/env python3
"""Verify CMO3/CAN3 with a user's local Cubism Editor; no proprietary code is copied."""
import argparse
import os
import math
from pathlib import Path
import subprocess


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("models", type=Path, nargs="+")
    parser.add_argument("--editor", type=Path, default=os.environ.get("ZIPZI_CUBISM_EDITOR"),
                        help="Cubism installation directory containing res/ (or ZIPZI_CUBISM_EDITOR)")
    parser.add_argument("--expected-geometry", type=Path,
                        help="Compare a single CMO3 against captured Zipzi poses (canvas pixels and optional render_order)")
    parser.add_argument("--geometry-tolerance-px", type=float, default=0.05)
    parser.add_argument("--expected-animation", type=Path,
                        help="Compare a single CAN3 against captured parameter/part values, optional subframe checks and scene FPS; validate serialized curves separately from native Bezier approximation")
    parser.add_argument("--resave-animation", type=Path,
                        help="Save a single CAN3 through Cubism and verify expression metadata survives reloading")
    args = parser.parse_args()
    if not math.isfinite(args.geometry_tolerance_px) or args.geometry_tolerance_px <= 0:
        parser.error("--geometry-tolerance-px must be positive and finite")
    if args.expected_geometry and (len(args.models) != 1 or args.models[0].suffix != ".cmo3"):
        parser.error("--expected-geometry requires exactly one CMO3 file")
    if args.expected_animation and (len(args.models) != 1 or args.models[0].suffix != ".can3"):
        parser.error("--expected-animation requires exactly one CAN3 file")
    if args.resave_animation and (len(args.models) != 1 or args.models[0].suffix != ".can3"):
        parser.error("--resave-animation requires exactly one CAN3 file")
    if args.resave_animation and (args.resave_animation.exists() or args.resave_animation.suffix != ".can3"):
        parser.error("--resave-animation requires a new CAN3 destination")
    if args.editor is None:
        parser.error("set --editor or ZIPZI_CUBISM_EDITOR to your Cubism installation")
    root = Path(__file__).resolve().parents[2]
    res = args.editor.resolve() / "res"
    if not (res / "Live2D_Cubism.jar").is_file():
        parser.error(f"missing Cubism Editor JAR under {res}")
    target = root / "target/cmo3-probe"
    classes = target / "classes"
    work = target / "work"
    classes.mkdir(parents=True, exist_ok=True)
    work.mkdir(parents=True, exist_ok=True)
    subprocess.run(["javac", "-proc:none", "-cp", str(res / "*"), "-d", str(classes),
                    str(root / "tools/validation/cmo3/ReadCmo3.java")], check=True, timeout=60)
    for model in args.models:
        geometry = ([str(args.expected_geometry.resolve()), str(args.geometry_tolerance_px)]
                    if args.expected_geometry else [])
        if args.expected_animation:
            geometry = [str(args.expected_animation.resolve())]
        if args.resave_animation:
            geometry += ["--resave", str(args.resave_animation.resolve())]
        result = subprocess.run(["java", "-Djava.awt.headless=true", "-cp",
                                 os.pathsep.join([str(classes), str(res / "*")]),
                                 "zipzi.cmo3probe.ReadCmo3", str(model.resolve()), *geometry],
                                cwd=work, capture_output=True, text=True,
                                timeout=180 if args.resave_animation else 60)
        read_errors = any(marker in result.stderr for marker in ("error :", "error >>", "Exception"))
        if result.returncode or read_errors:
            print(result.stdout)
            print(result.stderr)
            raise SystemExit(result.returncode or 1)
        print(model)
        print("\n".join(line for line in result.stdout.splitlines()
                        if line.startswith(("Cubism read:", "Objects:", "Geometry:", "Draw order:", "Animation:", "Expression operations:"))))


if __name__ == "__main__":
    main()
