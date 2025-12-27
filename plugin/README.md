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

## Commands

| Command | Description |
|---------|-------------|
| `:E @local/path` | Open file from server filesystem |
| `:E @browser/path` | Open file from browser OPFS |
| `:E @ssh/user@host/path` | Open file from SSH remote |
| `:VfsStatus` | Show current buffer's VFS backend |

## File Browsing

Use Neovim's built-in netrw:

```vim
:Ex              " Open file explorer
:Ex ~/projects   " Browse specific directory
:Vex             " Vertical split explorer
```

Or install a file explorer plugin:

- [oil.nvim](https://github.com/stevearc/oil.nvim) - Edit filesystem like a buffer
- [nvim-tree](https://github.com/nvim-tree/nvim-tree.lua) - Tree explorer
- [telescope.nvim](https://github.com/nvim-telescope/telescope.nvim) - Fuzzy finder

## Git

Use shell commands:

```vim
:!git status
:!git diff
:!git add %
:!git commit -m "message"
```

Or install git plugins:

- [fugitive.vim](https://github.com/tpope/vim-fugitive) - Git wrapper
- [gitsigns.nvim](https://github.com/lewis6991/gitsigns.nvim) - Git signs in gutter
- [lazygit.nvim](https://github.com/kdheepak/lazygit.nvim) - LazyGit integration

## License

MIT
