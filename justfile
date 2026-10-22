# Python handles arguments directly on every OS; no Bash or PowerShell is needed.
set shell := ["python3", "-B", "scripts/dev.py", "--just"]
set windows-shell := ["python", "-B", "scripts/dev.py", "--just"]
set positional-arguments

# Show available recipes.
default:
    @list

# Run the desktop application; arguments are passed to Amigo.
run *args:
    @run

# Run the optimized application.
run-release *args:
    @run-release

# Build the optimized executable for the current operating system.
build:
    @build

# Run workspace tests; accepts extra cargo test arguments.
test *args:
    @test

# Run optional tests requiring Node.js or network access.
test-optional:
    @test-optional

# Format the Rust workspace.
fmt:
    @fmt

# Check Rust formatting.
fmt-check:
    @fmt-check

# Check every crate and target for warnings.
lint:
    @lint

# Test build, packaging, and release helpers without external services.
test-recipes:
    @test-recipes

# Run formatting, Clippy, workspace tests, and helper tests.
check:
    @check

# Check native package tools and metadata, or one specific format.
dist-check format="all":
    @dist-check

# Build a native Linux RPM.
package-rpm:
    @package-rpm

# Build a native Linux DEB.
package-deb:
    @package-deb

# Build a native Linux AppImage.
package-appimage:
    @package-appimage

# Build a native macOS .app inside a ZIP archive.
package-macos:
    @package-macos

# Build a native macOS drag-to-install DMG.
package-dmg:
    @package-dmg

# Build a native Windows portable ZIP.
package-windows:
    @package-windows

# Build a native Windows per-user installer using Inno Setup.
package-installer:
    @package-installer

# Build all package formats for the current OS and architecture.
dist:
    @dist

# Check, package, and write checksums, source information, and editable notes.
release-prepare:
    @release-prepare

# Upload assets to an unpublished GitHub draft for an already pushed tag.
release-draft tag:
    @release-draft
