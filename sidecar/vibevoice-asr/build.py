#!/usr/bin/env python3
"""
Build script for VibeVoice-ASR sidecar executable.

Usage:
    python build.py              # Build using spec file
    python build.py --onedir     # Build as directory (faster, for dev)
    python build.py --clean      # Clean build artifacts before building

Prerequisites:
    pip install pyinstaller
"""

import argparse
import shutil
import subprocess
import sys
from pathlib import Path

SIDECAR_DIR = Path(__file__).parent
SPEC_FILE = SIDECAR_DIR / "vibevoice-asr.spec"
DIST_DIR = SIDECAR_DIR / "dist"
BUILD_DIR = SIDECAR_DIR / "build"


def clean():
    """Remove build artifacts."""
    for d in [DIST_DIR, BUILD_DIR]:
        if d.exists():
            print(f"Removing {d}")
            shutil.rmtree(d)
    print("Clean complete.")


def build(onedir: bool = False):
    """Build the sidecar executable."""
    # Check pyinstaller is available
    try:
        subprocess.run(
            [sys.executable, "-m", "PyInstaller", "--version"],
            check=True,
            capture_output=True,
        )
    except (subprocess.CalledProcessError, FileNotFoundError):
        print("ERROR: PyInstaller not found. Install with: pip install pyinstaller")
        sys.exit(1)

    if onedir:
        # Quick dev build: onedir mode (not a single exe)
        cmd = [
            sys.executable, "-m", "PyInstaller",
            "--onedir",
            "--name", "vibevoice-asr",
            "--add-data", f"config.py{os.pathsep}.",
            "--add-data", f"model_loader.py{os.pathsep}.",
            "--add-data", f"inference.py{os.pathsep}.",
            "--noconfirm",
            "main.py",
        ]
    else:
        # Production build: use spec file
        cmd = [
            sys.executable, "-m", "PyInstaller",
            "--noconfirm",
            str(SPEC_FILE),
        ]

    print(f"Building sidecar: {' '.join(cmd)}")
    result = subprocess.run(cmd, cwd=str(SIDECAR_DIR))

    if result.returncode != 0:
        print("BUILD FAILED")
        sys.exit(1)

    # Check output
    if onedir:
        exe_path = DIST_DIR / "vibevoice-asr" / ("vibevoice-asr.exe" if sys.platform == "win32" else "vibevoice-asr")
    else:
        exe_path = DIST_DIR / ("vibevoice-asr.exe" if sys.platform == "win32" else "vibevoice-asr")

    if exe_path.exists():
        size_mb = exe_path.stat().st_size / (1024 * 1024)
        print(f"\nBuild successful!")
        print(f"  Output: {exe_path}")
        print(f"  Size: {size_mb:.1f} MB")
    else:
        print(f"\nWARNING: Expected output not found at {exe_path}")
        print("Check the dist/ directory for the actual output.")


if __name__ == "__main__":
    import os

    parser = argparse.ArgumentParser(description="Build VibeVoice-ASR sidecar")
    parser.add_argument("--onedir", action="store_true", help="Build as directory (faster, for dev)")
    parser.add_argument("--clean", action="store_true", help="Clean build artifacts first")
    args = parser.parse_args()

    if args.clean:
        clean()

    build(onedir=args.onedir)
