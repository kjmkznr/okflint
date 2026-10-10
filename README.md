# okflint

A small, **dependency-free** linter for [Open Knowledge Format (OKF) v0.1](https://github.com/GoogleCloudPlatform/knowledge-catalog) bundles.

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
| OKF002 | error    | `index.md` must not contain frontmatter |
| OKF003 | error    | Frontmatter opened with `---` but never closed |
| OKF004 | error    | A `log.md` date heading is not ISO 8601 `YYYY-MM-DD` |
| OKF005 | warning  | `log.md` entries are not ordered newest-first |
| OKF006 | warning  | A `timestamp` field is not a valid ISO 8601 value |
| OKF007 | warning  | Broken internal link (OKF consumers must tolerate these) |

Errors map to OKF MUST rules; warnings to SHOULD / tolerated rules.

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

Targets OKF **v0.1 (Draft)**. The spec is young; checks will track it as it
evolves.

## License

Apache-2.0
