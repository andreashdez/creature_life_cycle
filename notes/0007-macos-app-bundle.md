# 0007 — A shell script packages the GUI as a macOS app bundle

Decided in `32e75e6`. Status: active.

## Context

`cargo build --release` produces a bare executable. On macOS that is awkward to
share: double-clicking it opens a terminal, it has no icon or display name, and
it is not what anyone expects to receive. Distributing the GUI to someone who
does not have a Rust toolchain needed a real `.app`.

## Decision

`scripts/package_gui.sh` builds the release GUI and assembles a bundle by hand:
it creates `Contents/MacOS` and `Contents/Resources`, copies the executable in,
writes an `Info.plist`, and zips the result to
`target/package/creature_life_cycle_gui-macos.zip`. The version in the plist is
read from `cargo pkgid`, so bumping `Cargo.toml` is the only place a version
number is edited. The script refuses to run anywhere other than macOS.

A plain POSIX shell script was chosen over a packaging crate: the bundle format
is a handful of directories and one plist, and the script has no dependencies
to keep current.

## Consequences

Releasing to a macOS user is one command. Output lands under `target/`, which
is already ignored by Git.

The bundle is unsigned and unnotarized, so Gatekeeper will object on another
machine unless the user overrides it. Signing needs a developer certificate and
is out of scope.

There is no equivalent for Linux or Windows. Those platforms build and run from
source; only macOS has a packaging path.

This decision is what made asset embedding matter (record
[0010](0010-macroquad-to-bevy.md)): a bundle that loaded sprites from a
directory beside the executable would break as soon as it was moved.
