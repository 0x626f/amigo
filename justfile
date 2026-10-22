# Run `just` to list recipes. Linux packaging prerequisites are in README.md.
set shell := ["bash", "-eo", "pipefail", "-c"]
set positional-arguments

# Show available recipes.
default:
    @just --list

# Run the desktop application; optional arguments are passed to Amigo.
run *args:
    cargo run --locked --package amigo -- "$@"

# Run the optimized application.
run-release *args:
    cargo run --locked --release --package amigo -- "$@"

# Build the optimized native Linux binary used by the packaging recipes.
build:
    cargo build --locked --release --package amigo --target-dir target

# Run all workspace tests; accepts extra cargo test arguments.
test *args:
    cargo test --locked --workspace "$@"

# Run optional tests that require Node.js and network access.
test-optional:
    cargo test --locked --workspace -- --ignored

# Format the Rust workspace.
fmt:
    cargo fmt --all

# Check formatting without changing files.
fmt-check:
    cargo fmt --all --check

# Check every crate and target for warnings.
lint:
    cargo clippy --locked --workspace --all-targets -- -D warnings

# Test the distribution and release preparation helpers without external services.
test-recipes:
    python3 -B -m unittest discover -s scripts -p 'test_*.py'

# Run the checks used before preparing a release.
check: fmt-check lint test test-recipes

# Check tools and metadata for all formats, or just rpm, deb, or appimage.
dist-check format="all":
    python3 -B scripts/distribute.py preflight "$1"

[private]
_preflight format:
    @python3 -B scripts/distribute.py preflight "$1"

# Create an RPM in dist/<version>/<architecture>/.
package-rpm: (_preflight "rpm") build
    python3 -B scripts/distribute.py rpm

# Create a DEB in dist/<version>/<architecture>/.
package-deb: (_preflight "deb") build
    python3 -B scripts/distribute.py deb

# Create an AppImage using linuxdeploy and its AppImage output plugin.
package-appimage: (_preflight "appimage") build
    python3 -B scripts/distribute.py appimage

# Build RPM, DEB, and AppImage packages for the current Linux architecture.
dist: (_preflight "all") package-rpm package-deb package-appimage

# Check, build all packages, and write checksums, provenance, and editable notes locally.
release-prepare: check dist
    python3 -B scripts/distribute.py prepare

# Upload prepared assets as a GitHub DRAFT for an already pushed tag (e.g. v0.1.0).
release-draft tag:
    python3 -B scripts/distribute.py draft "$1"
