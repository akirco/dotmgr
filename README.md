# Dotfile Manager

A TUI tool to manage dotfiles with sync functionality.

## Features

- Browse dotfiles in home directory
- Ignore/unignore files and directories
- Sync dotfiles to a dedicated directory (default: `~/.dotfiles`)
- Filter view (all files vs dotfiles only, pending sync items)

## Controls

| Key     | Action                   |
| ------- | ------------------------ |
| ↑/k     | Move up                  |
| ↓/j     | Move down                |
| Enter   | Open directory           |
| Esc     | Go back                  |
| Space/i | Toggle ignore            |
| p       | Toggle pending view      |
| s       | Sync to sync_dir         |
| a       | Toggle all/dotfiles view |
| r       | Refresh                  |
| q       | Quit                     |

## Config

Sync directory can be configured in `~/.config/dotmgr/config.toml`:

```toml
sync_dir = "/home/user/.dotfiles"
```

## Install

```sh
cargo install --git https://github.com/akirco/dotmgr

or
# https://github.com/marcosnils/bin
bin install https://github.com/akirco/dotmgr
```
