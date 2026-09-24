# 0016 — The CLI loads any config file with `--config`

Decided alongside record [0015](0015-invalid-config-is-an-error.md). Status:
active.

## Context

The CLI only read the XDG `simulation.toml`
(record [0004](0004-xdg-config-location.md)). Comparing two setups meant
copying files over that one path, and the only way to run a file kept
elsewhere was to point `XDG_CONFIG_HOME` at a directory laid out to match,
which is what the end-to-end tests had to do.

## Decision

`--config <path>` loads the named file instead. It goes through the library's
`load_board_from`, which shares its parsing with `load_configured_board` but
treats a missing file as an error rather than creating it: the user named this
file, so a typo in its path should fail, not quietly write a default board
somewhere unexpected. The XDG file is neither read nor created when the flag is
given.

The GUI does not take the flag. It has no command line to speak of, and its
**Save setup** writes to the XDG file, so a GUI that loaded from elsewhere
would save somewhere other than where it read.

## Consequences

Setups can live in a directory of their own and be run side by side with
`--seed` for comparison.

The CLI and the GUI can now look at different files. A setup run with
`--config` has to be copied to the XDG path before the GUI will open it.
