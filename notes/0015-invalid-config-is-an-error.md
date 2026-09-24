# 0015 — An invalid config is an error, and Save backs it up

Decided after a review of record [0004](0004-xdg-config-location.md). Status:
active. Supersedes the invalid-file fallback in 0004; the XDG location and the
creation of a missing file stand.

## Context

Record 0004 made an invalid `simulation.toml` fall back to the built-in
defaults, with a warning on stderr, and argued that "a typo costs the user one
run on defaults, not their setup". Neither half held up.

The CLI prints a board after that warning, and then a board every turn after
it, so the warning scrolls away and the exit code is `0`. A typo in the config
produced a complete, successful-looking run of a board nobody asked for.

The setup was not safe either. The GUI loaded the same defaults, and its
**Save setup** (record [0006](0006-gui-writes-the-shared-config.md)) wrote
them straight over the broken file, so the user's hand edit, the very thing
0004 meant to protect, was gone after one keypress. A packaged macOS app has no
visible stderr, so the GUI user never saw the warning at all.

## Decision

`load_configured_board` returns a `Result`. A missing file is still created
from the defaults, and a missing config directory still runs on them, but a
file that exists and cannot be read or parsed is an error.

The CLI prints the error and exits with status `1` before simulating anything.

The GUI still opens, because a window that refuses to start is a worse way to
say "your file has a typo". It runs the defaults and pins
"Config file is invalid; showing the defaults." to the status line until
another message replaces it. `save_configured_board` checks the file it is
about to replace and, if it does not parse, renames it to `simulation.toml.bak`
first. The status line names the backup.

## Consequences

A mistake in the config can no longer pass for a successful run, and a broken
file can no longer be destroyed by the GUI.

Only one backup is kept. Saving over a second invalid file replaces the first
backup, which held an older broken file. That was judged acceptable against
the cost of numbering backups.

Creature positions outside the board are still skipped with a warning rather
than rejected. They are a smaller mistake, since the rest of the board still
loads as written, and changing that is a separate decision.
