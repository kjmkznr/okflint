# okflint

A small, **dependency-free** linter for [Open Knowledge Format (OKF) v0.2](https://github.com/GoogleCloudPlatform/knowledge-catalog) bundles.

OKF represents knowledge as a directory of Markdown files with YAML frontmatter.
`okflint` checks a bundle against the spec's rules and reports violations, with a
non-zero exit code on errors, so it can run in CI or a pre-commit hook.

Built with the Rust standard library only — no external crates, fast, portable.

## Install

```sh
cargo install --path .
```

## Usage

```sh
okflint [PATH]          # lint the bundle at PATH (default: current dir)
okflint --quiet .       # summary only
okflint --deny-warnings # also fail on warnings
```

Output is `path:line: severity [code] message`, e.g.:

```
ideas/0009-foo.md:1: error [OKF001] missing required frontmatter field `type`
journal/2026/06/2026-06-17.md:46: warning [OKF007] broken internal link: `/ideas/x.md`
```

## Checks

| Code   | Severity | Rule |
|--------|----------|------|
| OKF001 | error    | A non-reserved document is missing the required `type` field |
| OKF002 | error    | Bundle-root `index.md` frontmatter contains fields other than `okf_version`, or a subdirectory `index.md` contains any frontmatter |
| OKF003 | error    | Frontmatter opened with `---` but never closed |
| OKF004 | error    | A `log.md` date heading is not ISO 8601 `YYYY-MM-DD` |
| OKF005 | warning  | `log.md` entries are not ordered newest-first |
| OKF006 | warning  | A legacy `timestamp` field is not a valid ISO 8601 value (superseded by `generated.at` in v0.2, still accepted as a fallback) |
| OKF007 | warning  | Broken internal link (OKF consumers must tolerate these) |
| OKF008 | warning  | `okf_version` in the bundle-root `index.md` is not a `<major>.<minor>` version (e.g. `"0.2"`) |
| OKF009 | warning  | `status` is not one of `draft \| stable \| deprecated` |
| OKF010 | warning  | A v0.2 timestamp (`generated.at`, `verified[].at`, `stale_after`, `sources[].last_modified`, `usage_window.from/to`) is not an ISO 8601 datetime with an explicit UTC offset |
| OKF011 | warning  | An actor (`generated.by`, `verified[].by`, `sources[].author`) does not follow the `<producer>/<version>`, `human:<id>`, `process:<id>` convention |
| OKF012 | warning  | A v0.2 family is malformed: `generated.by`, `verified[].by/at`, `sources[].resource`, `parameters[].name`, `executor.resource`, `attester.resource`, or `usage_window.from/to` is missing |
| OKF013 | error    | `type: Attested Computation` is missing the required `runtime` field |
| OKF014 | warning  | An Attested Computation names no computation (neither an inline `# Computation` fence nor a `computation:` path) |
| OKF015 | warning  | A footnote `[^id]` has no matching `sources[].id` |

Errors map to OKF MUST rules; warnings to SHOULD / tolerated rules.

v0.1 bundles stay clean: a concept carrying just `type` (plus the legacy
`timestamp` and `# Citations` conventions) reports no new findings. The new
families (`sources`, `generated`, `verified`, `status`, `stale_after`,
`runtime`, `parameters`, `computation`, `executor`, `attester`) are all
optional and only checked when present — except `runtime`, which is required
for `type: Attested Computation`.

## `.okflintignore`

Bundles often contain non-concept Markdown (READMEs, specs). Since OKF treats
every non-reserved `.md` as a concept document requiring `type`, list such files
in a `.okflintignore` at the bundle root. Dot-directories (`.git`, `.claude`, …) are
always skipped.

The older name `.okfignore` is still read; if both files exist, their patterns
are combined.

```
# .okflintignore
README.md
CONTRIBUTING.md
docs/            # a directory and everything under it
*.draft.md       # suffix match
```

## Exit codes

| Code | Meaning |
|------|---------|
| 0 | No errors (warnings allowed unless `--deny-warnings`) |
| 1 | One or more errors (or warnings with `--deny-warnings`) |
| 2 | Usage error / could not read the bundle |

## Status

Targets OKF **v0.2**. See `tests/fixtures/valid-v2` for a bundle exercising
every v0.2 family and `tests/fixtures/invalid-v2` for each new finding.

## License

Apache-2.0
