# AGENTS.md

`actioneer` is a Zig CLI for scanning GitHub Actions workflow YAML, reporting update candidates, and rewriting selected references.

## Stack

- Zig
- `zli` for the CLI
- bundled `tree-sitter-yaml`

## Commands

- `just build`
- `just test`
- `just fmt`
- `just run -- <args>`
- `just validate -- <args>`

## Layout

- `src/main.zig`: process entrypoint
- `src/cli/`: commands, flags, UI, prompts
- `src/app/`: application services
- `src/core/`: scanning, parsing, Git/GitHub helpers, rewriting
- `src/syntax/`: YAML and GitHub Actions syntax helpers
- `vendor/`: vendored dependencies

## Current CLI

- Root command logic lives in `src/cli/cmd/update.zig`.
- `validate` and `version` are subcommands.
- Shared flags and positional parsing live in `src/cli/options.zig`.

## Rules

- Prefer small, targeted changes.
- Preserve existing CLI behavior unless the task explicitly requires a change.
- Read the relevant command path before editing behavior.
- Keep user-facing output consistent with `src/cli/ui.zig`.
- Reuse `src/core/types.zig` types when possible.
- Do not modify `vendor/` unless the task requires it.

## Verification

- Run `zig fmt build.zig src` after Zig edits.
- Run `zig build test` after behavioral changes when feasible.
- If CLI behavior changed, run the relevant path with `zig build run -- ...`.
- If you cannot run verification, say what was not run and why.
