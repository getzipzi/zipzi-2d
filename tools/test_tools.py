#!/usr/bin/env python3
"""Discover repository tooling tests (no SDK build); forward unittest discovery flags."""

from pathlib import Path
import sys
import unittest

ROOT = Path(__file__).resolve().parents[1]


def main():
    sys.path.insert(0, str(ROOT / "tools"))
    unittest.main(module=None, argv=[sys.argv[0], "discover", "-s", str(ROOT / "tools/tests"), *sys.argv[1:]])


if __name__ == "__main__":
    main()
