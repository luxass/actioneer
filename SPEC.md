# Actioneer Rewrite Specification

## 1. Core goal

Actioneer is rebuilt from first principles.

The rewrite is accepted only if the resulting implementation is simpler, easier
to read, and easier to reason about than the current implementation.

The implementation should be designed around command behavior, not around
preserving existing internal abstractions.

```text
audit  = scan refs -> apply policy -> report/fix violations
update = scan refs -> choose updates/fixes -> patch refs
```

Existing internal structs, modules, abstractions, and tests do not need to survive.
This document is intended to be the source of truth for the rewrite after the
current source and tests are removed.

## 2. Command specifications

The rewrite supports:

```text
actioneer
actioneer audit
actioneer audit --fix
actioneer update
actioneer version
```

Running `actioneer` with no subcommand intentionally runs the default update flow.
Every explicit subcommand has its own behavior and must not accidentally fall
through to update behavior.

The command entry points should read as straightforward pipelines. A contributor
should be able to open the command implementation and understand the command
without learning internal framework abstractions first.

### `actioneer`

Running `actioneer` with no subcommand runs the default update flow.

This is an intentional product behavior, not legacy compatibility.

Conceptual flow:

```text
load config
parse CLI overrides
scan action refs
filter refs
fetch required GitHub/cache data
choose update candidates
show interactive TUI selection unless --yes or --dry-run is used
patch selected refs unless --dry-run is used
print summary
exit
```

### `actioneer audit`

Purpose: check whether workflow action refs satisfy policy.

Expected shape:

```text
actioneer audit [OPTIONS] [INPUT]...
```

Important options:

```text
--fix
--offline
--no-cache
--filter OWNER/NAME      # repeatable
--exclude <PATTERN>
-r, --recursive
--mode tui|plain|json
```

Audit never uses the TUI. `--mode tui` and `--mode plain` both produce human text
for audit; `tui` only matters for interactive update selection.

Conceptual flow:

```text
load config
parse CLI overrides
scan action refs
filter refs
compute effective policy per ref
fetch required GitHub/cache data only when needed for policy checks
produce audit findings
if --fix: convert all fixable findings to patch edits and apply them without prompting
print human or JSON output
return success when no findings remain, otherwise failure
```

Acceptance criteria:

- reports policy violations
- fails when violations exist
- succeeds when all refs satisfy policy
- supports plain/tui text output
- supports redesigned JSON output
- supports offline/cache-only mode
- does not generate update candidates unless `--fix` requires a concrete fix
- does not depend on update UI or update output models

### `actioneer audit --fix`

Purpose: automatically fix policy violations where a safe fix exists.

`audit --fix` is explicitly mutating. It does not prompt, does not use the TUI,
and does not require `--yes`.

Conceptual behavior:

```text
branch/tag ref       -> pin to full SHA when target data is available
short-sha ref        -> pin to full SHA only when it uniquely matches known GitHub/cache data
SHA/comment mismatch -> fix SHA and/or version comment when target data is available
unfixable finding    -> report clearly and leave unchanged
```

Acceptance criteria:

- applies all available safe fixes without TUI selection
- does not require `--yes`
- pins insecure mutable refs to full SHAs by default
- respects config/rules for matching refs
- respects repeatable `--filter` / `--exclude`
- respects offline/cache-only mode
- supports `--dry-run`
- patches files safely
- reports what changed and what could not be fixed
- returns failure if findings remain after attempted fixes

### `actioneer update`

Purpose: update action refs to the newest allowed target according to config,
rules, update mode, pin style, and release age policy.

Expected shape:

```text
actioneer update [OPTIONS] [INPUT]...
```

Important options:

```text
--pin sha|tag
--update major|minor|patch
--min-release-age 30m|12h|7d
--skip-branches
--offline
--no-cache
--dry-run
-y, --yes
--filter OWNER/NAME      # repeatable
--exclude <PATTERN>
-r, --recursive
--mode tui|plain|json
```

Conceptual flow:

```text
load config
parse CLI overrides
scan action refs
filter refs
compute effective update config per ref
fetch required GitHub/cache data
choose update candidates
apply min-release-age and update-mode constraints
if --dry-run: print planned candidates and exit without writing
if --yes: select all candidates
if --dry-run and --yes are both set: --dry-run wins and no files are written
if mode is plain/json and --yes/--dry-run is not set: fail early before GitHub/cache fetching when possible
if mode is tui and --yes/--dry-run is not set: show simplified TUI selection
convert selected candidates to patch edits
patch files safely
print summary
exit
```

Acceptance criteria:

- pin style defaults to SHA
- mutable refs are pinned to full SHAs by default
- existing pinned SHAs can be updated when newer allowed versions exist
- version comments are preserved/updated for human readability
- update mode `major|minor|patch` is respected
- min age is respected globally and through matching rules
- downgrades are avoided
- branch refs are skipped only when effective config says to skip branches
- supports dry-run, yes mode, TUI selection, JSON output, no-cache mode, and offline/cache-only mode
- `plain` and `json` modes never open a TUI and require `--yes` or `--dry-run` for update
- supports repeatable `--filter OWNER/NAME` and `--exclude` before fetching GitHub/cache data
- filtered refs still use their effective config/rules normally
- does not produce audit output wrappers

### `actioneer version`

Purpose: print version information.

Expected shape:

```text
actioneer version
```

Acceptance criteria:

- prints the actioneer version
- does not scan workflows
- does not read GitHub/cache data
- returns success

## 3. Default security policy

By default, all external GitHub Actions references should be pinned to full SHAs.

These are audit failures unless explicitly allowed by policy/rules:

```yaml
- uses: owner/repo@main
- uses: owner/repo@v4
- uses: owner/repo@v4.2.0
- uses: owner/repo@abc1234
```

These represent:

```text
branch-like mutable ref
mutable version tag
mutable exact tag
short SHA
```

This is secure by default:

```yaml
- uses: owner/repo@0123456789abcdef0123456789abcdef01234567
```

Version comments may be used to preserve human readability:

```yaml
- uses: actions/checkout@0123456789abcdef0123456789abcdef01234567 # v4.2.2
```

## 4. Pinning and update strategy

The rewrite must preserve configurable update strategy.

### Pin style

`actioneer update` supports:

```text
--pin sha
--pin tag
```

Default:

```text
--pin sha
```

Behavior:

- `--pin sha` rewrites selected refs to full commit SHAs.
- `--pin tag` rewrites selected refs to version tags.
- SHA pinning remains the default and recommended secure behavior.
- Config/rules may override pinning behavior for matching actions.

Config equivalent:

```toml
pin = "sha" # or "tag"
```

### Update mode

`actioneer update` supports:

```text
--update major
--update minor
--update patch
```

Default:

```text
--update major
```

Behavior:

- `major`: allow newest available version.
- `minor`: stay within current major version.
- `patch`: stay within current major/minor version.
- No downgrades.
- If current semantic version cannot be determined, behavior must be defined and tested.

Config equivalent:

```toml
update = "major" # "minor" | "patch"
```

### Branch handling

Preserve:

```text
--skip-branches
```

Behavior:

- Branch-like refs are audit failures by default.
- Update normally fixes branch-like refs by pinning them to an allowed target.
- `--skip-branches` prevents update suggestions for branch-like refs.
- `--skip-branches` must not hide audit failures.

Config equivalent:

```toml
skip_branches = true
```

### Release age

Preserve:

```text
--min-release-age 30m
--min-release-age 12h
--min-release-age 7d
```

Default:

```text
10h
```

Behavior:

- Update ignores candidate tags newer than the configured age.
- Rules may override min-release-age per action.
- Offline mode must use cached release age data and fail clearly when required data is missing.

Config equivalent:

```toml
min_release_age = "7d"
```

## 5. Config file support

The rewrite supports `.actioneer.toml`.

Config may live at:

```text
.actioneer.toml
.github/actioneer.toml
```

Acceptance criteria:

- config is loaded automatically when running inside a repository
- CLI flags override config values
- config format is documented
- config parse errors are clear and include file path
- if both config files exist, precedence is deterministic and documented

Precedence:

```text
defaults
-> .actioneer.toml globals
-> .actioneer.toml rules in order
-> .github/actioneer.toml globals
-> .github/actioneer.toml rules in order
-> CLI overrides
```

`.github/actioneer.toml` overrides root `.actioneer.toml` for global config
values. Rules are ordered conditional overrides. Root config rules run before
`.github` config rules.

Config supports these top-level keys:

```toml
# never perform network requests; use cache only
offline = false

# bypass cache reads/writes and always use fresh network data
no_cache = false

# default output/interaction mode
mode = "tui" # "tui" | "plain" | "json"

# pin style for update/fix operations and audit policy
pin = "sha" # "sha" | "tag"

# allowed update range
update = "major" # "major" | "minor" | "patch"

# minimum age for candidate releases/tags
min_release_age = "10h"

# do not create update candidates for branch-like refs
skip_branches = false

# pin also defines the audit policy for matching refs:
# sha => full SHA required; tag => version tags are allowed
# branch-like refs and short SHAs remain findings by default
```

These keys are also the configurable values that rules may override unless the
field is inherently global. `offline`, `no_cache`, and `mode` are global execution
settings and are not rule-overridable.

## 6. Config rules

Rules are ordered conditional overrides for the normal configurable values.

Rules are configured as an array:

```toml
min_release_age = "7d"
pin = "sha"
update = "major"

[[rules]]
name = "my actions update immediately"
when = 'ActionRepoOwner == "luxass"'
min_release_age = "0h"

[[rules]]
name = "internal actions may use version tags"
when = 'ActionRepoOwner == "my-org"'
pin = "tag"
```

### Rule fields

A rule has:

- `name`: optional human label, recommended for diagnostics
- `when`: required condition expression
- any subset of normal configurable values, such as:
  - `min_release_age`
  - `pin`
  - `update`
  - `skip_branches`
  - future config fields

`offline`, `no_cache`, and `mode` are not valid rule fields.

Rules apply only to action references whose `when` expression evaluates to true.

`pin` defines both update output and audit policy for matching refs:

- `pin = "sha"` means full SHA pins are required.
- `pin = "tag"` means version tag refs are policy-compliant for matching refs.

Branch-like refs and short SHAs remain findings by default. SHA/comment mismatch
findings are never suppressed by `pin = "tag"`.

### Effective config

For each discovered action reference, actioneer computes an effective config:

```text
built-in defaults
-> .actioneer.toml globals
-> .actioneer.toml matching rules in file order
-> .github/actioneer.toml globals
-> .github/actioneer.toml matching rules in file order
-> CLI overrides
```

When multiple rules match, later matching rules override earlier matching rules.
This makes broad rules first and specific exceptions later the recommended style.

### Example result

Given:

```toml
min_release_age = "7d"

[[rules]]
name = "luxass defaults"
when = 'ActionRepoOwner == "luxass"'
min_release_age = "1d"

[[rules]]
name = "fast specific action"
when = 'ActionRepo == "luxass/foo"'
min_release_age = "0h"
```

For:

```yaml
- uses: luxass/foo@v1
```

The effective value is:

```text
min_release_age = 0h
```

because both rules match and the later, more specific rule overrides the earlier
broad rule.

### Rule metadata

Rule conditions should be able to reference simple action metadata:

```text
ActionRepoOwner
ActionRepoName
ActionRepo
ActionPath
WorkflowFile
CurrentRef
CurrentRefKind
```

Where:

```text
ActionRepo = "owner/name"
CurrentRefKind = full_sha | short_sha | version_tag | branch_or_tag
```

### Rule condition language

The initial condition language should be intentionally small.

Supported operators/functions:

```text
==
!=
&&
||
starts_with(value, prefix)
```

Supported literals:

```text
"string"
true
false
```

Parentheses may be supported for grouping. If parentheses are not implemented in
the first version, precedence must be documented and tested.

Examples:

```text
ActionRepoOwner == "luxass"
ActionRepo != "actions/checkout"
starts_with(WorkflowFile, ".github/workflows/")
ActionRepoOwner == "luxass" || ActionRepoOwner == "my-org"
ActionRepoOwner == "luxass" && CurrentRefKind != "full_sha"
```

Do not add a general-purpose scripting language unless there is a concrete need.

Acceptance criteria:

- rules are evaluated deterministically
- rule precedence is documented as ordered, last matching override wins
- rule evaluation errors are clear and include the rule name when available
- policy/rule evaluation is isolated from scanning, GitHub fetching, and patching
- rules do not make command flow hard to read

## 7. Offline and cache modes

The rewrite supports offline/cache-only operation:

```text
actioneer audit --offline
actioneer audit --fix --offline
actioneer update --offline
```

Offline mode applies to every command that would otherwise need GitHub data. This
includes update: update can work offline when the required tag/release data is in
cache.

Also configurable:

```toml
offline = true
```

Offline mode:

- never performs network requests
- uses cached GitHub data only
- fails clearly when required cache data is missing
- reports local-only audit findings normally, but fails if required cache data for SHA/comment verification is missing
- works for audit
- works for audit `--fix`
- works for update when required cache data exists

The rewrite also keeps no-cache behavior:

```text
--no-cache
```

No-cache mode:

- bypasses cache reads and writes
- performs fresh network requests when GitHub data is needed
- conflicts with `--offline`; using both is a CLI/config error after config and CLI overrides are resolved

## 8. JSON output redesign

JSON output is redesigned and documented.

Acceptance criteria:

- JSON is stable and command-shaped, not internal-struct-shaped.
- JSON is useful for CI and machines.
- JSON should not expose internal implementation structs directly.
- JSON output uses `--mode json`.
- Human diagnostics in JSON mode go to stderr.

### Shared JSON fields

JSON IDs are used only to correlate related objects inside the same command
result, such as an `audit --fix` fix entry pointing back to its finding. IDs must
be deterministic within one command run. They do not need to be stable across
versions.

Every command JSON document includes:

```json
{
  "schema_version": 1,
  "command": "audit",
  "ok": false
}
```

### Audit JSON

Required shape:

```json
{
  "schema_version": 1,
  "command": "audit",
  "ok": false,
  "summary": {
    "references": 3,
    "findings": 1,
    "fixable": 1
  },
  "findings": [
    {
      "id": "finding-1",
      "kind": "mutable_ref",
      "severity": "error",
      "file": ".github/workflows/ci.yml",
      "line": 12,
      "action": {
        "owner": "actions",
        "name": "checkout",
        "repo": "actions/checkout",
        "path": "",
        "ref": "v4"
      },
      "message": "Action is pinned to a mutable tag",
      "recommendation": "Pin to a full SHA",
      "fixable": true,
      "expected_sha": null
    }
  ]
}
```

### Update JSON

Required shape:

```json
{
  "schema_version": 1,
  "command": "update",
  "ok": true,
  "summary": {
    "references": 3,
    "candidates": 1,
    "selected": 1,
    "applied": 1
  },
  "candidates": [
    {
      "id": "update-1",
      "kind": "version_update",
      "file": ".github/workflows/ci.yml",
      "line": 12,
      "action": {
        "owner": "actions",
        "name": "checkout",
        "repo": "actions/checkout",
        "path": "",
        "current_ref": "v4"
      },
      "target": {
        "ref": "0123456789abcdef0123456789abcdef01234567",
        "version": "v4.2.2",
        "sha": "0123456789abcdef0123456789abcdef01234567",
        "pin": "sha"
      },
      "reason": "newer_version_available",
      "notes": ["mutable_ref"],
      "selected": true,
      "applied": true
    }
  ]
}
```

### Audit fix JSON

`audit --fix --mode json` uses the audit shape and additionally includes fixes.
`ok` is `true` only when no findings remain after fixes. `ok` is `false` when
unfixable findings remain or a fix fails.

```json
{
  "fixes": [
    {
      "finding_id": "finding-1",
      "file": ".github/workflows/ci.yml",
      "line": 12,
      "applied": true,
      "new_ref": "0123456789abcdef0123456789abcdef01234567",
      "new_version_comment": "v4.2.2"
    }
  ]
}
```

Exact field additions are allowed, but these required fields must exist.

## 9. Interactive UI

A terminal user interface is required for interactive update selection.

The rewrite should use `ratatui` + `crossterm` for the TUI. This keeps the project
on a mature Rust terminal stack and avoids spending rewrite effort on evaluating
UI libraries unless a concrete limitation is discovered.

The interactive UI survives, but is simplified.

Acceptance criteria:

- TUI-based interactive selection exists for update flows
- audit and `audit --fix` never use the TUI
- TUI uses `ratatui` + `crossterm` unless there is a documented reason to change
- UI is understandable and reliable
- UI does not require complex internal UI-specific models
- UI shows:
  - action name
  - file/line
  - current ref
  - target ref
  - reason/security note
- UI allows selecting updates/fixes
- UI can be less fancy than the current implementation

## 10. Source layout

The rewrite should keep a conventional Rust entry layout:

```text
src/main.rs
src/lib.rs
src/cmd/
```

Acceptance criteria:

- `src/main.rs` is a thin binary entry point that parses CLI args and dispatches commands.
- `src/lib.rs` exposes library modules used by tests and the binary.
- `src/cmd/` contains command entry points and command orchestration, such as audit/update/version.
- `src/cmd/` must not become a dumping ground for domain logic.
- Config loading, rule evaluation, workflow discovery, GitHub/cache access, patching, TUI rendering, and reusable JSON shaping should live outside `src/cmd/` unless the code is trivial and command-specific.
- Command files should read like linear pipelines that call into small, clearly named modules.
- The rest of the module layout is up to the implementation.
- Module layout should serve readability and command flow, not mirror the current code.

## 11. Simple architecture

The rewrite should use simple, command-shaped data structures.

Likely concepts:

```text
DiscoveredActionRef
GitHubTag
AuditFinding
UpdateCandidate
PatchEdit
Rule
Policy
```

Avoid:

```text
central god models
Option-heavy structs
shared structs where one command ignores half the fields
generic “assessment” layers
vague functions like resolve/assess unless truly precise
```

Command independence:

```text
audit does not generate update candidates unless --fix is used
update does not generate audit output wrappers
patching only receives patch edits
JSON structs are created at command boundaries
```

A reader should be able to open `audit::run` or `update::run` and understand the
command without first learning a web of internal abstractions.

## 12. Required behavior and edge cases

The rewrite should preserve useful product behavior and important edge cases, but
it should not preserve the old CLI shape just because it exists today.

### Target CLI design

The CLI should be designed intentionally around the new product model. A flag is
accepted only if it is clear, command-shaped, and useful in the new design.

Do not keep legacy flags only for compatibility.

Target commands:

```text
actioneer
actioneer audit
actioneer audit --fix
actioneer update
actioneer version
```

Target shared CLI flags:

```text
INPUT...                  # files/directories to scan
-r, --recursive           # recursive directory scanning
--filter OWNER/NAME       # include exact action repository; repeatable
--exclude <PATTERN>       # exclude matching action names
--offline                 # cache-only, no network
--no-cache                # bypass cache and force fresh network data
--mode tui|plain|json     # output/interaction mode; can also be configured
```

Target update/fix CLI flags:

```text
--dry-run                 # preview without writing
-y, --yes                 # apply without interactive selection
--pin sha|tag             # target pin style
--update major|minor|patch
--skip-branches
--min-release-age 30m|12h|7d
```

Notes:

- `--offline` is the target cache-only behavior.
- `--no-cache` is kept as the explicit way to bypass cache reads/writes and force fresh network data.
- `--offline` and `--no-cache` conflict and should produce a clear CLI/config error.
- `--mode tui|plain|json` is the target output/interaction mode flag and may also be set in config.
- Default mode is `tui`.
- `tui` enables interactive update selection.
- `plain` is raw text output without TUI interaction or decorative styling.
- `json` is machine output.
- `actioneer` with no subcommand runs the default update flow.
- `--dry-run` applies to mutating flows: update and `audit --fix`. It does not apply to plain `audit`.
- `--filter OWNER/NAME` supports multiple occurrences.
- `--filter OWNER/NAME` works for both audit and update and is applied before GitHub/cache fetching.
- Filtering does not bypass rules; remaining refs still compute effective config from matching rules.

Shared behavior to preserve:

- non-recursive scans default to `.github` when no input is given
- recursive scans default to `.` when no input is given
- explicit inputs may be files or directories
- filters match exact `OWNER/NAME`
- multiple filters are allowed and include refs matching any listed `OWNER/NAME`
- excludes match the discovered full action name by containment
- JSON output writes machine output to stdout and human diagnostics to stderr
- empty scans return success with a helpful human message

### Workflow discovery edge cases

Preserve support for:

- workflow YAML files
- composite action YAML files
- step-level `uses:`
- reusable workflow job-level `uses:`
- action paths, such as `owner/repo/path@ref`
- quoted and unquoted YAML scalar values
- version comments next to refs, such as `# v4.2.0`
- file, line, and edit location tracking for patching

Ignore:

- local actions, such as `./action`
- parent-relative local actions, such as `../action`
- docker references
- non-action `uses:`-like text inside unrelated strings
- non-YAML files when scanning directories
- non-composite action YAML files where appropriate

### GitHub and cache edge cases

Preserve support for:

- GitHub API pagination
- ignoring non-version tags
- authenticated requests
- `GITHUB_TOKEN`
- `gh auth token` fallback when available
- cache reads/writes unless disabled or offline/cache policy prevents it
- clear GitHub HTTP/rate-limit/request error reporting
- release date lookup for `--min-release-age`
- annotated and lightweight tags when resolving release dates

### Update edge cases

Preserve behavior for:

- detecting available version upgrades
- SHA pin output and tag pin output
- SHA pinning as the default
- branch-like refs as update candidates unless `--skip-branches` is set
- SHA/comment mismatches as fixable update candidates
- SHA-like refs with version comments as SHA refs, even when the SHA text is typoed
- using version comments to determine the current version for SHA-pinned refs
- avoiding downgrades
- skipping refs already current for the requested pin style
- multiple repos in one workflow file
- dry-run preview
- interactive selection when a TTY is available
- non-interactive apply with `--yes`
- min-release-age fallback to the newest older allowed tag

### Patch safety edge cases

Preserve behavior for:

- replacing only the scanned ref span
- erroring if the scanned target is no longer present
- multiple updates in one file
- updates across multiple files
- safely sorting interleaved file edits
- preserving quoted refs
- preserving CRLF line endings
- writing or updating version comments when needed
- avoiding version comments when the new ref already equals the version tag
- preserving non-version user comments as safely as the current behavior allows

### Audit edge cases

Preserve behavior for:

- success when all refs are securely pinned
- success on empty scans
- failure on branch-like mutable refs
- failure on mutable version tags, including `v4` and `v4.2.0`
- failure on short SHA refs
- failure on SHA/comment mismatches
- reporting file, line, action name, current ref, and finding kind in human output
- reporting expected SHA for SHA/comment mismatches when known
- respecting `--filter OWNER/NAME`
- respecting global `--exclude`
- success when filters exclude insecure refs
- failure when filters include insecure refs

### Exit codes

Preserve predictable exit behavior:

- successful update/audit/version returns success
- audit returns failure when findings exist
- canceled interactive selection returns success without changes
- interrupted interactive selection returns failure
- GitHub lookup errors return failure
- patch/write errors return failure
- empty scans return success

## 13. Behavior fixture workspace

The rewrite includes a dedicated fixture workspace separate from `.github/workflows`.

Location:

```text
testdata/workflows/
```

This directory contains realistic workflow YAML examples used for:

- e2e tests
- manual CLI verification
- documenting expected behavior

Required structure:

```text
testdata/workflows/
  audit/
    secure-full-sha/
      .github/workflows/ci.yml
    mutable-branch/
      .github/workflows/ci.yml
    mutable-tag/
      .github/workflows/ci.yml
    short-sha/
      .github/workflows/ci.yml
    sha-comment-mismatch/
      .github/workflows/ci.yml

  update/
    tag-to-sha/
      .github/workflows/ci.yml
    branch-to-sha/
      .github/workflows/ci.yml
    sha-version-bump/
      .github/workflows/ci.yml
    tag-pin-style/
      .github/workflows/ci.yml
    min-release-age/
      .github/workflows/ci.yml

  config/
    root-config/
      .actioneer.toml
      .github/workflows/ci.yml
    github-config/
      .github/actioneer.toml
      .github/workflows/ci.yml
    rules-order/
      .actioneer.toml
      .github/workflows/ci.yml
    owner-specific-rule/
      .actioneer.toml
      .github/workflows/ci.yml

  discovery/
    reusable-workflow/
      .github/workflows/ci.yml
    composite-action/
      action.yml
    ignores-local-and-docker/
      .github/workflows/ci.yml

  offline/
    cache-hit/
      .github/workflows/ci.yml
    cache-miss/
      .github/workflows/ci.yml
```

The exact file contents can evolve, but each fixture directory should have a
clear expected behavior documented either in the test name or a small README.

The actual project `.github/workflows` directory must not be used as test input.

## 14. Fixture safety

Tests must never patch source fixture files in-place.

Acceptance criteria:

- mutating tests copy fixtures to temporary directories before running commands
- source fixtures under `testdata/workflows` remain unchanged after tests
- CI/test checks protect fixture integrity
- an optional local pre-push hook may be provided, but correctness must not depend on it

A possible optional hook/check:

```sh
git diff --exit-code -- testdata/workflows
```

Tests/CI are the source of truth, not local hooks.

## 15. Test coverage

The rewrite must include behavior-focused tests that run the CLI against fixture
workspaces.

Tests should cover:

- audit plain/tui text behavior where important
- audit JSON behavior
- update JSON behavior
- `audit --fix`
- update patching
- files unchanged when no fix should apply
- offline mode
- missing cache in offline mode
- no-cache mode
- `--offline` plus `--no-cache` conflict handling
- config file loading from root
- config file loading from `.github/actioneer.toml`
- config precedence
- default `min_release_age = "10h"`
- rule-based min-release-age override
- owner-specific rules
- ordered rules where later matching rules override earlier matching rules
- rule condition operators: `==`, `!=`, `starts_with`, `&&`, `||`
- default SHA pinning policy
- plain/json update requiring `--yes` or `--dry-run`
- audit never opening a TUI
- audit --fix applying all fixable findings without prompting
- audit --fix failing when unfixable findings remain
- filter behavior before GitHub/cache fetching
- filters plus rules applying together
- configurable `mode`
- `pin = "tag"` allowing version tag refs as policy-compliant without allowing branch-like refs, short SHAs, or SHA/comment mismatch
- interactive selection logic at a simplified/unit level

The rewrite is accepted when:

```sh
cargo test
cargo test --features e2e --test e2e
```

pass, and new feature tests are included.

## 16. Deletion-friendly rewrite

Existing internal abstractions may be deleted instead of migrated.

The preferred result is:

```text
less code
fewer concepts
clearer command flow
boring data structures
```

Not a renamed version of the current design.
