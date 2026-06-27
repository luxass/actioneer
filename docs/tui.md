# TUI — living spec

Interactive terminal UI for the `update` command, built on [ratatui](https://ratatui.rs) + [crossterm](https://github.com/crossterm-rs/crossterm).

## Architecture

```
src/tui/
├── mod.rs        public entry-point; terminal lifecycle + event loop
├── app.rs        App state, scan worker, selection navigation
├── selection.rs  SelectableUpdate rows from ScanReport
├── event.rs      EventHandler (background thread, channel-based)
├── theme.rs      semantic colour palette + Style constructors
└── ui.rs         update screen rendering
```

### Entry points

```rust
actioneer::tui::run_app(ActioneerConfig) -> Result<TuiOutcome, TuiError>
```

`actioneer update` (and bare `actioneer`) launches the TUI unless `--mode plain` or
`--mode json` is passed.

`actioneer audit` and `actioneer version` always use plain stdout.

### Scan flow

1. TUI renders immediately with a spinner while `scan_workspace` runs on a background thread.
2. When the scan completes, planned updates appear in an interactive table.
3. User selects rows (Space / `a`) and presses Enter to apply — the TUI closes and a coloured summary is printed to stdout (plain text when piped).

## Colour palette

Semantic roles in `theme.rs` — tuned for dark terminals, inspired by modern CLIs (cargo, gh):

| Role | Use |
|------|-----|
| Brand (violet) | `actioneer` header |
| Accent (sky) | subcommand name, info icons, spinner |
| Workflow (cyan) | workflow file column |
| Action (amber) | action reference column |
| From (muted gray) | current pin |
| To (green) | target pin |
| Key (soft blue) | footer bindings |
| Success / warn / error | status lines and checkmarks |

Selected table rows get a subtle blue-gray background band (buffer patch in `ui.rs`).

Post-TUI apply output uses the same semantics via `src/ansi.rs` when stdout is a TTY.

## Interactive selection

When planned updates exist:

| Key | Action |
|-----|--------|
| `↑` / `k` | Move cursor up |
| `↓` / `j` | Move cursor down |
| `Space` | Toggle `[x]` on current row |
| `a` | Select all |
| `n` | Deselect all |
| `Enter` | Apply selected updates |
| `q` / `Esc` | Quit |

Rows start unselected; use Space or `a` to choose updates. Footer shows `N selected`.

## Terminal safety

- Raw mode + alternate screen entered before the loop.
- Panic hook restores the terminal on crash.

## Future work

- [ ] Scrollbar for long tables
- [ ] `?` help overlay
- [ ] Mouse support
