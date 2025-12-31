# nvim-web.nvim

Minimal VFS helpers for nvim-web browser integration.

## Installation

### lazy.nvim

```lua
{
  "your-username/nvim-web.nvim",
  config = function()
    require("nvim-web").setup()
  end,
}
```

### Manual

```lua
vim.opt.runtimepath:append("/path/to/nvim-web/plugin")
require("nvim-web").setup()
```

## VFS Commands

| Command | Description |
|---------|-------------|
| `:E @local/path` | Open file from server filesystem |
| `:E @browser/path` | Open file from browser OPFS |
| `:E @ssh/user@host/path` | Open file from SSH remote |
| `:VfsStatus` | Show current buffer's VFS backend |

## Git Commands

Built-in Git integration (no external plugins required):

| Command | Description |
|---------|-------------|
| `:Git [args]` | Run any git command |
| `:G [args]` | Short alias for :Git |
| `:Gstatus` | Git status |
| `:Gdiff [file]` | Git diff (current file or all) |
| `:Glog [n]` | Git log (default 20 commits) |
| `:Gblame` | Git blame current file |
| `:Gadd [file]` | Stage file (current if no arg) |
| `:Gcommit [msg]` | Commit (inline message optional) |
| `:Gpush` | Push to remote |
| `:Gpull` | Pull from remote |

### Features

- **Tab completion** for subcommands and file paths
- **Syntax highlighting** for diff, log, blame output
- **Output buffer** with `q` to close for long output
- **Error handling** with exit code display

## File Browsing

Use Neovim's built-in netrw:

```vim
:Ex              " Open file explorer
:Ex ~/projects   " Browse specific directory
:Vex             " Vertical split explorer
```

Or install a file explorer plugin:

- [oil.nvim](https://github.com/stevearc/oil.nvim)
- [nvim-tree](https://github.com/nvim-tree/nvim-tree.lua)
- [telescope.nvim](https://github.com/nvim-telescope/telescope.nvim)

## License

MIT
