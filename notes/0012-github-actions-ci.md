# 0012 — CI runs on GitHub Actions instead of Woodpecker

Decided in `2b16e22`, replacing `06e48de`. Status: active.

## Context

CI was configured in `.woodpecker.yml` and required a Woodpecker instance to
run it. The repository is hosted on GitHub, so the checks lived somewhere other
than where the code and the pull requests were: results were not attached to a
pull request, and a contributor could not see them.

The Bevy adoption (record [0010](0010-macroquad-to-bevy.md)) also made the
build heavier, so CI needed system libraries and a dependency cache to stay
tolerable.

## Decision

The Woodpecker configuration was replaced by `.github/workflows/ci.yml`,
running on pushes to `main` and on pull requests. The job installs Bevy's Linux
build dependencies, installs a stable toolchain with `rustfmt` and `clippy`,
restores a cache with `Swatinem/rust-cache`, and runs four checks:

```sh
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo check --locked --no-default-features --all-targets
```

Concurrency is grouped per ref with `cancel-in-progress`, so a new push
supersedes the run it replaces.

## Consequences

Checks run where the code lives, report on pull requests, and need no
infrastructure to maintain. The README's status badge points at them.

`--locked` on every command means `Cargo.lock` is authoritative: a dependency
bump has to be committed, and CI fails rather than silently resolving something
newer.

The fourth check exists solely to keep the feature gate from record
[0011](0011-optional-gui-feature.md) honest. It is the slowest thing to notice
if removed and the cheapest to keep.

Installing Bevy's Linux libraries on every run is the main fixed cost. The
cache absorbs the Rust side of the build; the `apt-get` step is not cached.
