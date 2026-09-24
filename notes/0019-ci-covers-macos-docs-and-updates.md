# 0019 — CI covers macOS, the API docs, and dependency updates

Decided after a review of the CI setup. Status: active. Extends record
[0012](0012-github-actions-ci.md); its Linux job and four checks stand.

## Context

Record 0012 set up one Linux job. Three things were not covered.

The GUI is released only as a macOS app bundle (record
[0007](0007-macos-app-bundle.md)), but CI never built it on macOS. Bevy
compiles different windowing, input, and rendering backends per platform, so
a change could pass on Linux and break the only build that ships.

Every function carries a `///` comment, and those comments name other items.
Nothing checked that they still pointed at anything after a rename.

Dependencies moved only when someone ran `cargo update` by hand. With
`--locked` everywhere, `Cargo.lock` does not drift on its own. It also never
moves forward unless someone remembers to update it.

## Decision

- The Linux job gains a fifth check,
  `cargo doc --locked --no-deps --document-private-items` with
  `RUSTDOCFLAGS=-D warnings`. Private items are included because most of the
  code, the whole GUI binary, is private.
- A `macos-latest` job runs `cargo test --locked`. That builds every target,
  the GUI included, and runs the full suite. It skips formatting, Clippy, and
  the docs, which do not depend on the platform. It does not run
  `scripts/package_gui.sh`, because the release profile's thin LTO and single
  codegen unit make a full Bevy release build much slower than the debug build
  the tests need.
- `.github/dependabot.yml` opens weekly pull requests for Cargo and GitHub
  Actions. Cargo patch releases are grouped into one pull request. Larger
  updates arrive one by one, because a new Bevy may need code changes and a
  new `rand_chacha` may change seeded runs (record
  [0014](0014-stable-rng-algorithm.md)). All Actions updates are grouped.

## Consequences

A macOS-only build failure, a broken doc link, or a stale dependency now shows
up in CI instead of at release time or not at all.

The macOS job is the slowest part of CI. GitHub bills macOS minutes at ten
times the Linux rate, but only for private repositories; public ones run
free. Its cache is separate from the Linux one, so its first
run compiles Bevy from scratch.

Dependabot pull requests are subject to the same snapshot tests as any other
change. An update that changes seeded output fails until the snapshots are
updated deliberately.

The packaging script is still only exercised by hand.
