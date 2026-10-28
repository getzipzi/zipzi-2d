#!/usr/bin/env python3
"""Build a local PSD2Live checkout and export the synthetic meshing reference."""
from pathlib import Path
import argparse
import os
import subprocess
import tempfile


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("checkout", type=Path)
    parser.add_argument("--output", type=Path, default=Path("fixtures/mesh-generation/psd2live.json"))
    parser.add_argument("--gradle-home", type=Path, default=Path.home() / ".gradle")
    args = parser.parse_args()
    checkout = args.checkout.resolve()
    subprocess.run([str(checkout / "gradlew"), "compileKotlin", "--offline"], cwd=checkout, check=True)
    revision = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=checkout, text=True).strip()
    classes = sorted(checkout.glob("**/build/classes/kotlin/main"))
    jars = []
    for name in ("kotlin-stdlib", "kotlinx-serialization-core-jvm", "kotlinx-serialization-json-jvm"):
        candidates = sorted((args.gradle_home / "caches/modules-2/files-2.1").glob(f"*/{name}/*/*/*.jar"))
        if not candidates:
            parser.error(f"Missing cached dependency: {name}")
        jars.append(candidates[-1])
    classpath = os.pathsep.join(str(p) for p in classes + jars)
    source = Path(__file__).with_name("Reference.java")
    args.output.parent.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix="zipzi-mesh-reference-") as compiled:
        subprocess.run(["javac", "-cp", classpath, "-d", compiled, str(source)], check=True)
        subprocess.run(["java", "-cp", compiled + os.pathsep + classpath, "Reference", str(args.output.resolve()), revision], check=True)


if __name__ == "__main__":
    main()
