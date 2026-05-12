# dotmgr

A TUI dotfile manager.

## Features

- Browse home directory and sync repo
- Toggle ignore files
- Sync (s/S) / Deploy (d/D) files
- Filter: show all or dotfiles only

## Controls

| Key   | Action        |
| ----- | ------------- |
| Tab   | Swap view     |
| ↑/↓   | Navigate      |
| s/S   | Sync          |
| d/D   | Deploy        |
| a     | Show all      |
| ^a    | Ignore all    |
| Space | Toggle ignore |
| q     | Quit          |

## Indicators

| Symbol | Meaning                |
| ------ | ---------------------- |
| ⊘      | Not synced (Home view) |
| ⚡     | Missing in home (Sync) |

## Config

`~/.config/dotmgr/config.toml`:

```toml
sync_dir = "/path/to/dotfiles"
synced_ignores = [".git", ".github", "README.md", "LICENSE"]
```

## Install

```sh
cargo install --git https://github.com/akirco/dotmgr

# or
#https://github.com/marcosnils/bin

bin install https://github.com/akirco/dotmgr
```
