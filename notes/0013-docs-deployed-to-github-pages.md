# 0013 — The documentation site deploys to GitHub Pages from CI

Decided alongside the Quarkdown site in `docs/`. Status: active.

## Context

The site in `docs/` is Quarkdown source, not HTML. Reading it on GitHub means
reading `.qd` markup with unrendered function calls, and building it requires
installing Quarkdown. Without somewhere to publish, the site is only useful to
someone who already has the toolchain and a checkout.

Committing the generated HTML into the repository was rejected: the output is
several megabytes per build, is regenerated wholesale each time, and would make
every documentation change an unreviewable diff.

## Decision

`.github/workflows/docs.yml` builds the site and publishes it to GitHub Pages
at <https://andreashdez.github.io/creature_life_cycle>, which is the URL
already set as `baseurl` in `docs/_setup.qd` for canonical links and
`sitemap.xml`.

The workflow is separate from `ci.yml` rather than a job inside it, because the
two have nothing in common: different triggers, different tooling, and a
documentation change should not wait on a Bevy compile.

Details worth recording:

- The Quarkdown version is pinned in an `env` block and the toolchain is cached
  on that version. The release archive bundles its own JRE, so no Java setup
  step is needed, but it is large enough to be worth caching.
- The build runs with `--strict`, so an unresolved function or a broken
  reference fails the workflow instead of silently producing a degraded page.
- Pull requests build the site but never deploy. Breakage is caught before
  merge; only `main` publishes.
- The build job resolves the output directory by finding `index.html` rather
  than hardcoding it, because Quarkdown names that directory after `.docname`.
- Deployments use a `pages` concurrency group with `cancel-in-progress: false`,
  so a queued run never publishes a site that a newer run has already replaced.

## Consequences

The documentation has a public home, and the rendered site is what people read
rather than the markup.

Publishing depends on a repository setting that does not live in the
repository: Pages must be configured with GitHub Actions as its source. That is
a one-time action, but it means a fresh fork or a restored repository will not
publish until someone flips it.

Pinning the Quarkdown version means the site does not change underneath the
project when a new release lands, at the cost of having to bump the pin
deliberately to pick up fixes.
