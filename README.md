# actioneer

`actioneer` audits GitHub Actions workflow references and proposes or applies
updates. It discovers workflow YAML, resolves remote actions through GitHub,
checks pin quality, and plans semver-aware upgrades. Interactive update mode uses
a terminal UI; plain text and JSON modes are available for automation.

## Build

The rewrite targets Rust 1.95 or newer.

```sh
cargo build --locked
cargo test --locked
```

Run the development build with `cargo run --`:

```sh
cargo run -- --help
cargo run -- version
```

## Usage

| Invocation | Behavior |
|------------|----------|
| `actioneer` or `actioneer update` | Scan `.github/workflows/` and open the update TUI |
| `actioneer audit` | Print audit issues; exit 1 when issues are found |
| `actioneer update --mode plain` | Print the proposed update table |
| `actioneer update --mode json` | Print proposed updates as JSON |
| `actioneer update --apply` | Apply every proposed update |
| `actioneer update --dry-run` | Preview apply results without writing files |
| `actioneer version` | Print the installed version |

Pass a workflow file or a directory as `PATH`. Directory discovery is flat and
includes files ending in `.yml` or `.yaml`.

```sh
actioneer testdata/workflows/advanced.yml
actioneer audit .github/workflows/ci.yml
actioneer update .github/workflows --mode plain
```

Global options may appear with the bare command or a subcommand:

- `--pin sha|tag` chooses full-SHA pins with tag comments or tag pins.
- `--update major|minor|patch` controls the allowed semver bump.
- `--skip-branches` prevents branch-pinned references from being processed.
- `--min-release-age 7d` rejects releases newer than the given duration.
- `--offline` allows cache reads but performs no network requests.
- `--no-cache` bypasses cache reads and writes.
- `--mode plain|json` disables the TUI and selects an output format.
- `--apply` writes planned updates; `--dry-run` previews those writes.

`--offline` and `--no-cache` are mutually exclusive.

## Configuration

An optional `.github/actioneer.toml` supplies defaults. Command-line options
override values loaded from the file.

```toml
pin = "sha"
update = "minor"
skip_branches = false
min-release-age = "7d"
mode = "plain"
```

With SHA pinning, an updated reference is written as a full commit SHA with a
human-readable tag comment, for example:

```yaml
uses: actions/checkout@9c091bb21b7c1c1d1991bb908d89e4e9dddfe3e0 # v7.0.0
```

## Development

Run all repository checks with:

```sh
just ci
```

The living design specifications are in `docs/`. Contributors and coding agents
should also read `AGENTS.md`, which records the rewrite architecture, deferred
features, and repository conventions.
