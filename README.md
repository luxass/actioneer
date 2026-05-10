# actioneer

`actioneer` scans GitHub Actions workflows, finds outdated `uses:` references, and can rewrite them to newer versions or pinned SHAs.

## Install

macOS and Linux:

```sh
curl -fsSL https://raw.githubusercontent.com/luxass/actioneer/main/install.sh | sh
```

The installer downloads the correct release for your platform and installs `actioneer` into `~/.local/bin` by default.

You can override the target directory with `ACTIONEER_INSTALL_DIR`, and pin a specific release with `ACTIONEER_VERSION`.

Windows can use the release archives from [GitHub Releases](https://github.com/luxass/actioneer/releases).

## Quick Start

```sh
actioneer --dry-run
actioneer --yes
actioneer validate
```

By default, `actioneer` scans `.github`. Use `--recursive` to scan from the current directory, or pass a file or directory explicitly.

## What It Does

- Finds GitHub Actions references in workflow YAML.
- Resolves newer tags through the GitHub API.
- Rewrites references either as SHAs or preserved tags.
- Detects SHA/comment mismatches before you trust pinned actions.
- Supports interactive use, CI validation, and JSON output.

## Notes

- `--style sha` is the default.
- `validate` exits non-zero on SHA/comment mismatches.
- Interactive selection requires a TTY.
- Set `GITHUB_TOKEN` if you want higher GitHub API rate limits.

## Build From Source

If you want to build it yourself, use Zig `0.16.0` or newer and run `zig build`.

## 📄 License

Published under [MIT License](./LICENSE).
