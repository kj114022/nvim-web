# nvim-web.nvim

Neovim plugin for nvim-web browser integration.

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

### packer.nvim

```lua
use {
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
| `:NvimWebExplorer [backend]` | Toggle file explorer (local/ssh/browser) |
| `:NvimWebSessions` | Open session manager |
| `:NvimWebSessionNew` | Create new session |
| `:NvimWebSessionShare` | Copy share link |
| `:NvimWebSSH user@host` | Mount SSH filesystem |
| `:NvimWebConnections` | SSH connection manager |
| `:NvimWebConnect url` | Connect to remote host |

## File Explorer Keymaps

| Key | Action |
|-----|--------|
| `Enter` / `l` | Open file/directory |
| `h` / `-` | Go to parent directory |
| `a` | Create new file/directory |
| `d` | Delete |
| `r` | Rename |
| `y` | Copy path |
| `R` | Refresh |
| `1` / `2` / `3` | Switch to local/ssh/browser backend |
| `q` / `Esc` | Close explorer |

## Session Manager Keymaps

| Key | Action |
|-----|--------|
| `n` | New session |
| `s` | Share session |
| `q` / `Esc` | Close |

## SSH Connection Keymaps

| Key | Action |
|-----|--------|
| `a` | Add connection |
| `c` | Connect |
| `d` | Disconnect |
| `x` | Remove |
| `Enter` | Browse files |
| `q` / `Esc` | Close |

## Configuration

```lua
require("nvim-web").setup({
  explorer_width = 30,       -- Explorer sidebar width
  explorer_position = "left", -- "left" or "right"
  default_backend = "local",  -- "local", "ssh", or "browser"
})
```

## License

MIT
