# 0011 — Bevy sits behind an optional, default-on `gui` feature

Decided in `5f85ae0`. Status: active.

## Context

Adopting Bevy (record [0010](0010-macroquad-to-bevy.md)) added a game engine to
the dependency tree of a project whose library and CLI are a few thousand lines
of arithmetic. Everything paid for it: a clean `cargo test`, a `cargo check` in
an editor, and a CI run all compiled the engine, even though nothing outside
`src/bin/gui.rs` refers to it.

## Decision

Bevy is an optional dependency behind a `gui` feature, which is on by default
so that a plain `cargo build` still produces the GUI:

```toml
[features]
default = ["gui"]
gui = ["dep:bevy"]
```

The `gui` binary declares `required-features = ["gui"]`, so asking for it
without the feature reports that cleanly instead of failing to compile.
Building with `--no-default-features` leaves the library, the CLI, the benches,
and the tests untouched and pulls 49 crates instead of the full tree.

## Consequences

The library, CLI, and tests can be built and checked quickly, which matters
most in the edit-compile loop and in CI.

The gate has to be maintained: no code outside the GUI binary may reference
Bevy, and nothing in the library may be gated behind `gui`, or the
no-default-features build breaks. Because that is easy to violate by accident
and invisible in a normal build, CI runs
`cargo check --locked --no-default-features --all-targets` as a dedicated step
(record [0012](0012-github-actions-ci.md)).

Defaulting the feature *on* was chosen over off: the common case is someone who
wants to watch the simulation, and making them discover a feature flag first is
a worse first impression than a slow first build.
