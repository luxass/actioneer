# TUI — living spec

Interactive terminal UI for the `update` command, built on [ratatui](https://ratatui.rs) + [crossterm](https://github.com/crossterm-rs/crossterm).

## Architecture

```
src/tui/
├── mod.rs        public entry-point; terminal lifecycle + event loop
├── app.rs        App state, scan worker, selection navigation
├── selection.rs  SelectableUpdate rows from ScanReport
├── event.rs      EventHandler (background thread, channel-based)
├── theme.rs      colour palette + Style constructors
└── ui.rs         update screen rendering
```

### Entry points

```rust
actioneer::tui::run_app(ActioneerConfig) -> Result<(), TuiError>
```

`actioneer update` (and bare `actioneer`) launches the TUI unless `--mode plain` or
`--mode json` is passed.

`actioneer audit` and `actioneer version` always use plain stdout.

### Scan flow

1. TUI renders immediately with a spinner while `scan_workspace` runs on a background thread.
2. When the scan completes, planned updates appear in an interactive table.
3. User selects rows, confirms — file patching is not yet implemented.

## Interactive selection

When planned updates exist:

| Key | Action |
|-----|--------|
| `↑` / `k` | Move cursor up |
| `↓` / `j` | Move cursor down |
| `Space` | Toggle `[x]` on current row |
| `a` | Select all |
| `n` | Deselect all |
| `Enter` | Open confirm screen |
| `q` / `Esc` | Quit (select view) |

### Confirm screen

| Key | Action |
|-----|--------|
| `Enter` | Confirm selection (apply stub — patching not implemented) |
| `Esc` | Back to selection |
| `q` | Quit |

All rows are selected by default. Footer shows `N selected`.

## Terminal safety

- Raw mode + alternate screen entered before the loop.
- Panic hook restores the terminal on crash.

## Future work

- [ ] File patching / apply selected updates
- [ ] Scrollbar for long tables
- [ ] `?` help overlay
- [ ] Mouse support
