# 0017 — The library returns typed errors and notices, and never prints

Decided after a review of the library's public API. Status: active.

## Context

The library printed to stderr in six places. `Board::add_aphid` and
`Board::add_ladybug` printed when a position was outside the board, and
`load_configured_board` printed when the config directory was missing, when it
created a default file, and when that creation failed. The front ends had no
say in where those messages went. The GUI, whose packaged macOS app has no
visible stderr, could not show them, and a test could not check them without
running a binary and reading its stderr.

Every fallible function also returned `Result<_, String>`. A caller could print
the message, but could not tell "the file does not exist" from "the file is not
valid TOML" from "the directory could not be created" without matching on
English text.

## Decision

Nothing in `src/lib.rs` prints. Failures are errors, and events worth reporting
on a load that succeeded are notices, and both are returned to the caller.

- `InvalidConfig` says why config contents do not describe a board: a TOML or
  schema error, an empty board, or a probability out of range (with its label
  and value).
- `ConfigError` says why a config file could not be used. It has one variant
  per failure (`NoConfigHome`, `NotFound`, `Read`, `Invalid`, `CreateDir`,
  `Write`, `Backup`), and each carries the path and the underlying `io::Error`
  or `InvalidConfig`.
- `ConfigNotice` covers a load that succeeded but has something to report: no
  config directory, a default file created or not created, or a starting
  creature left out because it is outside the board. `parse_simulation_config`
  returns the notices, and the load functions return them in a `LoadedBoard`
  next to the board.

`add_aphid` and `add_ladybug` already returned `None` for a position outside
the board, so they simply stopped printing.

The error types are written by hand rather than with `thiserror`. There are
three small enums, and the dependency list is kept short on purpose. Each
`Display` message already includes the inner error, so `source()` is left
empty. Otherwise a caller that walks the error chain would print the inner
error twice.

## Consequences

The CLI and the GUI both print notices to stderr, with the same wording as
before, so the output did not change. The seeded snapshot tests, which require
empty stderr, pass unchanged. The one visible difference is that a missing
config now prints one line, "Created default config at …", instead of two.

A front end can now choose where each message goes. The GUI could, for
example, show skipped creatures in its status line. Tests can check notices
and error variants directly, without running a binary.

Code that matched on message text has to match on variants instead. Inside
this repository that was only the library's own tests.
