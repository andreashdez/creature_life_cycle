# 0004 — The config lives in the XDG config home and is created on demand

Decided in `a1421f4`, extended in `15ffa6b`. Status: active.

## Context

`simulation.toml` was read from the current working directory. That is fine
when running `cargo run` from a checkout and wrong everywhere else: a packaged
build has no checkout beside it, and the file a user edits would depend on
which directory they happened to launch from. It also meant the repository
carried a live config file that was easy to modify and accidentally commit.

## Decision

The active config is read from and written to:

```text
${XDG_CONFIG_HOME:-$HOME/.config}/creature_life_cycle/simulation.toml
```

The repository's copy was renamed to `simulation.example.toml` and demoted to
documentation. If the config file is missing, the program creates it from the
built-in standard board and default probabilities, including any parent
directories (`15ffa6b`). If it exists but is invalid, the program falls back to
the same built-in defaults and leaves the file untouched.

## Consequences

The CLI, the GUI, and a packaged build all agree on one location, which is what
lets the GUI write a setup the CLI will then read (record
[0006](0006-gui-writes-the-shared-config.md)).

A first run now has a side effect — it writes a file — which is why the tests
point `XDG_CONFIG_HOME` at a throwaway directory under `target/` rather than
touching the developer's real config.

Not overwriting an invalid file is deliberate. A typo costs the user one run on
defaults, not their setup. The cost is that a broken file stays broken until it
is fixed by hand, and the only signal is the warning printed at startup.
