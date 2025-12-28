-- nvim-web.nvim - Minimal VFS helpers for nvim-web browser integration
-- 
-- This plugin provides VFS path shortcuts and status commands.
-- For file browsing, use Neovim's built-in :Ex or plugins like oil.nvim
-- For git, use :!git commands or fugitive.vim

local M = {}

function M.setup()
  -- VFS path shortcut command: :E @local/path, :E @browser/path, :E @ssh/path
  vim.api.nvim_create_user_command("E", function(args)
    local path = args.args
    
    -- Expand @backend/ shortcuts to vfs:// URIs
    if path:match("^@local/") then
      path = "vfs://local/" .. path:sub(8)
    elseif path:match("^@browser/") then
      path = "vfs://browser/" .. path:sub(10)
    elseif path:match("^@ssh/") then
      path = "vfs://ssh/" .. path:sub(6)
    end
    
    vim.cmd("edit " .. vim.fn.fnameescape(path))
  end, { nargs = 1, complete = "file", desc = "Open file with VFS backend shortcut" })

  -- Show current buffer's VFS backend and path
  vim.api.nvim_create_user_command("VfsStatus", function()
    local name = vim.api.nvim_buf_get_name(0)
    local backend = "local"
    local path = name
    
    if name:match("^vfs://browser/") then
      backend = "browser"
      path = name:sub(15)
    elseif name:match("^vfs://ssh/") then
      backend = "ssh"
      path = name:sub(11)
    elseif name:match("^vfs://local/") then
      backend = "local"
      path = name:sub(13)
    end
    
    vim.notify(string.format("[%s] %s", backend:upper(), path), vim.log.levels.INFO)
  end, { desc = "Show current VFS backend and path" })

  -- Configure netrw for minimal file browsing
  vim.g.netrw_banner = 0      -- Hide banner
  vim.g.netrw_liststyle = 3   -- Tree style
  vim.g.netrw_winsize = 25    -- 25% width for splits

  -- Configure clipboard provider
  -- This delegates clipboard operations to the host via RPC
  _G.NvimWebClipboardCopy = function(lines, regtype)
    vim.rpcnotify(0, 'clipboard_write', lines, regtype)
  end

  _G.NvimWebClipboardPaste = function()
    -- This sends a request to host, which asks browser, and returns result
    local ok, result = pcall(vim.rpcrequest, 0, 'clipboard_read', '')
    if ok then
      return result
    else
      return { '', 'v' }
    end
  end

  vim.g.clipboard = {
    name = 'nvim-web',
    copy = {
      ['+'] = 'v:lua.NvimWebClipboardCopy',
      ['*'] = 'v:lua.NvimWebClipboardCopy',
    },
    paste = {
      ['+'] = 'v:lua.NvimWebClipboardPaste',
      ['*'] = 'v:lua.NvimWebClipboardPaste',
    },
    cache_enabled = 1,
  }
end

return M
