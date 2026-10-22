#!/usr/bin/env python3
"""Shell-free development and native packaging commands (Python 3.11+)."""

import os
from pathlib import Path
import subprocess
import sys

import distribute as dist

ROOT = Path(__file__).resolve().parents[1]


def run(*args, **kwargs):
    subprocess.run(args, cwd=ROOT, check=True, **kwargs)


def build():
    if os.environ.get("CARGO_BUILD_TARGET"):
        raise ValueError("Unset CARGO_BUILD_TARGET for a native build.")
    if sys.platform == "darwin":
        os.environ.setdefault("MACOSX_DEPLOYMENT_TARGET", dist.MACOS_MINIMUM)
    run("cargo", "build", "--locked", "--release", "--package", "amigo", "--target-dir", "target")


def dispatch(command, args):
    cargo = {
        "run": ("run", "--locked", "--package", "amigo", "--"),
        "run-release": ("run", "--locked", "--release", "--package", "amigo", "--"),
        "test": ("test", "--locked", "--workspace"),
        "test-optional": ("test", "--locked", "--workspace", "--", "--ignored"),
        "fmt": ("fmt", "--all"),
        "fmt-check": ("fmt", "--all", "--check"),
        "lint": ("clippy", "--locked", "--workspace", "--all-targets", "--", "-D", "warnings"),
    }
    if command in cargo:
        run("cargo", *cargo[command], *args)
    elif command == "list" and not args:
        run("just", "--list")
    elif command == "test-recipes" and not args:
        run(sys.executable, "-B", "-m", "unittest", "discover", "-s", "scripts", "-p", "test_*.py")
    elif command == "check" and not args:
        for check in ("fmt-check", "lint", "test", "test-recipes"):
            dispatch(check, [])
    elif command == "build" and not args:
        build()
    elif command == "dist-check" and len(args) <= 1:
        kind = args[0] if args else "all"
        dist.preflight(dist.native_formats() if kind == "all" else (kind,))
    elif command.startswith("package-") and command[8:] in dist.FORMATS and not args:
        kind = command[8:]
        dist.preflight((kind,))
        build()
        dist.package(kind)
    elif command == "dist" and not args:
        dist.preflight(dist.native_formats())
        build()
        for kind in dist.native_formats():
            dist.package(kind)
    elif command == "release-prepare" and not args:
        dispatch("check", [])
        dispatch("dist", [])
        dist.prepare()
    elif command == "release-draft" and len(args) == 1:
        dist.draft(args[0])
    else:
        raise ValueError("Unknown command or arguments. Run just --list; commands also work as python scripts/dev.py COMMAND.")


def main(argv):
    os.chdir(ROOT)
    if argv[:1] == ["--just"]:
        # just supplies recipe text, recipe name ($0), then positional arguments.
        # Treat the body as a command name, never as executable shell text.
        if len(argv) < 3:
            raise ValueError("Missing just recipe invocation.")
        dispatch(argv[1].strip(), argv[3:])
    elif argv:
        dispatch(argv[0], argv[1:])
    else:
        raise ValueError("Usage: python scripts/dev.py COMMAND [ARGS...]")


if __name__ == "__main__":
    try:
        main(sys.argv[1:])
    except (ValueError, OSError, subprocess.CalledProcessError) as error:
        sys.exit(f"Error: {error}")
