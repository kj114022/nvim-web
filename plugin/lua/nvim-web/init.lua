-- nvim-web.nvim - Minimal VFS helpers for nvim-web browser integration
-- 
-- This plugin provides VFS path shortcuts and status commands.
-- For file browsing, use Neovim's built-in :Ex or plugins like oil.nvim

local M = {}

-- Git subcommands for completion
local git_subcommands = {
  "add", "blame", "branch", "checkout", "cherry-pick", "clone", "commit",
  "diff", "fetch", "init", "log", "merge", "pull", "push", "rebase",
  "remote", "reset", "restore", "revert", "rm", "show", "stash", "status",
  "switch", "tag"
}

-- Run git command and display output in a scratch buffer
local function run_git(args, opts)
  opts = opts or {}
  local cmd = "git " .. args
  
  -- Run command and capture output
  local output = vim.fn.systemlist(cmd)
  local exit_code = vim.v.shell_error
  
  -- For simple commands with no output, just show status
  if #output == 0 or (#output == 1 and output[1] == "") then
    if exit_code == 0 then
      vim.notify("git " .. args, vim.log.levels.INFO)
    else
      vim.notify("git " .. args .. " (exit " .. exit_code .. ")", vim.log.levels.WARN)
    end
    return
  end
  
  -- For commands with output, show in buffer or floating window
  if opts.float or (args:match("^diff") or args:match("^log") or args:match("^show") or args:match("^blame")) then
    -- Create a scratch buffer for output
    local buf = vim.api.nvim_create_buf(false, true)
    vim.api.nvim_buf_set_lines(buf, 0, -1, false, output)
    vim.api.nvim_buf_set_option(buf, "buftype", "nofile")
    vim.api.nvim_buf_set_option(buf, "bufhidden", "wipe")
    vim.api.nvim_buf_set_option(buf, "swapfile", false)
    vim.api.nvim_buf_set_option(buf, "modifiable", false)
    
    -- Set filetype for syntax highlighting
    if args:match("^diff") then
      vim.api.nvim_buf_set_option(buf, "filetype", "diff")
    elseif args:match("^log") then
      vim.api.nvim_buf_set_option(buf, "filetype", "git")
    elseif args:match("^blame") then
      vim.api.nvim_buf_set_option(buf, "filetype", "fugitiveblame")
    end
    
    -- Open in split
    vim.cmd("vsplit")
    vim.api.nvim_win_set_buf(0, buf)
    vim.api.nvim_buf_set_name(buf, "git://" .. args:gsub("%s+", "/"))
    
    -- Map q to close
    vim.keymap.set("n", "q", ":close<CR>", { buffer = buf, silent = true })
  else
    -- For short output, just print it
    for _, line in ipairs(output) do
      print(line)
    end
    if exit_code ~= 0 then
      vim.notify("git exited with code " .. exit_code, vim.log.levels.WARN)
    end
  end
end

-- Git completion function
local function git_complete(arg_lead, cmd_line, cursor_pos)
  local args = vim.split(cmd_line, "%s+")
  
  -- If we're completing the first argument (subcommand)
  if #args <= 2 then
    local matches = {}
    for _, sub in ipairs(git_subcommands) do
      if sub:find("^" .. arg_lead) then
        table.insert(matches, sub)
      end
    end
    return matches
  end
  
  -- For file arguments, use file completion
  return vim.fn.getcompletion(arg_lead, "file")
end

function M.setup()
  -- :E [path] - Open file with VFS backend shortcut
  -- If no path given, trigger browser file picker
  vim.api.nvim_create_user_command("E", function(args)
    local path = args.args
    
    -- If no path given, trigger browser file picker
    if path == "" then
      vim.rpcnotify(1, 'open_file_picker')
      vim.notify("Opening file picker...", vim.log.levels.INFO)
      return
    end
    
    -- Expand @backend/ shortcuts to vfs:// URIs
    if path:match("^@local/") then
      path = "vfs://local/" .. path:sub(8)
    elseif path:match("^@browser/") then
      path = "vfs://browser/" .. path:sub(10)
    elseif path:match("^@ssh/") then
      path = "vfs://ssh/" .. path:sub(6)
    elseif path:match("^@github/") then
      path = "vfs://github/" .. path:sub(9)
    end
    
    vim.cmd("edit " .. vim.fn.fnameescape(path))
  end, { nargs = "?", complete = "file", desc = "Open file (no args = file picker)" })

  -- :Edit - Explicit file picker trigger (alias)
  vim.api.nvim_create_user_command("Edit", function()
    vim.rpcnotify(1, 'open_file_picker')
    vim.notify("Opening file picker...", vim.log.levels.INFO)
  end, { desc = "Open browser file picker" })

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

  ---------------------------------------------------------------------------
  -- Git Commands
  ---------------------------------------------------------------------------
  
  -- :Git [args] - Run any git command
  vim.api.nvim_create_user_command("Git", function(args)
    if args.args == "" then
      run_git("status")
    else
      run_git(args.args)
    end
  end, { 
    nargs = "*", 
    complete = git_complete,
    desc = "Run git command" 
  })
  
  -- :G - Short alias for :Git
  vim.api.nvim_create_user_command("G", function(args)
    if args.args == "" then
      run_git("status")
    else
      run_git(args.args)
    end
  end, { 
    nargs = "*", 
    complete = git_complete,
    desc = "Run git command (alias for :Git)" 
  })
  
  -- Convenient shortcuts
  vim.api.nvim_create_user_command("Gstatus", function() run_git("status") end, 
    { desc = "Git status" })
  vim.api.nvim_create_user_command("Gdiff", function(args)
    run_git("diff " .. (args.args or ""))
  end, { nargs = "*", desc = "Git diff" })
  vim.api.nvim_create_user_command("Glog", function(args)
    local count = args.args ~= "" and args.args or "20"
    run_git("log --oneline -n " .. count)
  end, { nargs = "?", desc = "Git log (default 20 commits)" })
  vim.api.nvim_create_user_command("Gblame", function()
    local file = vim.fn.expand("%")
    if file ~= "" then
      run_git("blame " .. vim.fn.shellescape(file))
    else
      vim.notify("No file to blame", vim.log.levels.WARN)
    end
  end, { desc = "Git blame current file" })
  vim.api.nvim_create_user_command("Gadd", function(args)
    if args.args ~= "" then
      run_git("add " .. args.args)
    else
      local file = vim.fn.expand("%")
      if file ~= "" then
        run_git("add " .. vim.fn.shellescape(file))
      else
        run_git("add -A")
      end
    end
  end, { nargs = "*", complete = "file", desc = "Git add (current file if no args)" })
  vim.api.nvim_create_user_command("Gcommit", function(args)
    if args.args ~= "" then
      run_git('commit -m ' .. vim.fn.shellescape(args.args))
    else
      run_git("commit")
    end
  end, { nargs = "*", desc = "Git commit (with message if provided)" })
  vim.api.nvim_create_user_command("Gpush", function() run_git("push") end, 
    { desc = "Git push" })
  vim.api.nvim_create_user_command("Gpull", function() run_git("pull") end, 
    { desc = "Git pull" })

  ---------------------------------------------------------------------------
  -- VFS Autocommands
  ---------------------------------------------------------------------------
  
  local vfs_group = vim.api.nvim_create_augroup("NvimWebVfs", { clear = true })

  -- BufReadCmd: Called when Neovim opens a buffer with a vfs:// name
  vim.api.nvim_create_autocmd("BufReadCmd", {
    group = vfs_group,
    pattern = "vfs://*",
    callback = function(args)
      local uri = args.file
      vim.api.nvim_buf_set_option(args.buf, 'buftype', 'acwrite')
      
      -- Request file content from Host via RPC
      local ok, result = pcall(vim.rpcrequest, 1, 'vfs_read', uri)
      if ok and type(result) == 'table' then
        -- result is array of lines
        vim.api.nvim_buf_set_lines(args.buf, 0, -1, false, result)
        vim.api.nvim_buf_set_option(args.buf, 'modified', false)
      else
        local err = ok and "No content" or tostring(result)
        vim.notify("VFS read failed: " .. err, vim.log.levels.ERROR)
      end
    end,
    desc = "Read vfs:// files through Host RPC",
  })

  -- BufWriteCmd: Called when Neovim saves a buffer with a vfs:// name
  vim.api.nvim_create_autocmd("BufWriteCmd", {
    group = vfs_group,
    pattern = "vfs://*",
    callback = function(args)
      local uri = args.file
      local lines = vim.api.nvim_buf_get_lines(args.buf, 0, -1, false)
      
      -- Send file content to Host via RPC
      local ok, result = pcall(vim.rpcrequest, 1, 'vfs_write', uri, lines)
      if ok and result then
        vim.api.nvim_buf_set_option(args.buf, 'modified', false)
        vim.notify("VFS: Saved " .. uri, vim.log.levels.INFO)
      else
        local err = ok and "Write failed" or tostring(result)
        vim.notify("VFS write failed: " .. err, vim.log.levels.ERROR)
      end
    end,
    desc = "Write vfs:// files through Host RPC",
  })

  ---------------------------------------------------------------------------
  -- Large File Fast Mode (Performance Optimization)
  ---------------------------------------------------------------------------
  
  local large_file_group = vim.api.nvim_create_augroup("NvimWebLargeFile", { clear = true })
  
  -- Threshold for "large file" mode (1MB)
  local LARGE_FILE_THRESHOLD = 1024 * 1024
  
  -- Disable heavy features for large files
  vim.api.nvim_create_autocmd("BufReadPre", {
    group = large_file_group,
    callback = function(args)
      local file = args.file
      local ok, stats = pcall(vim.loop.fs_stat, file)
      
      if ok and stats and stats.size > LARGE_FILE_THRESHOLD then
        -- Mark buffer as large file
        vim.b[args.buf].large_file = true
        
        -- Disable treesitter
        vim.treesitter.stop(args.buf)
        
        -- Disable syntax highlighting
        vim.cmd("syntax off")
        
        -- Disable other expensive plugins
        vim.opt_local.foldmethod = "manual"
        vim.opt_local.spell = false
        vim.opt_local.swapfile = false
        vim.opt_local.undofile = false
        vim.opt_local.breakindent = false
        vim.opt_local.colorcolumn = ""
        vim.opt_local.list = false
        
        -- Notify user
        local size_mb = string.format("%.1fMB", stats.size / 1024 / 1024)
        vim.notify("Large file (" .. size_mb .. "): Fast mode enabled", vim.log.levels.INFO)
      end
    end,
    desc = "Enable fast mode for large files",
  })

  ---------------------------------------------------------------------------
  -- Netrw and Clipboard Configuration
  ---------------------------------------------------------------------------
  
  -- Configure netrw for minimal file browsing
  vim.g.netrw_banner = 0      -- Hide banner
  vim.g.netrw_liststyle = 3   -- Tree style
  vim.g.netrw_winsize = 25    -- 25% width for splits

  -- Configure clipboard provider
  -- This delegates clipboard operations to the host via RPC
  _G.NvimWebClipboardCopy = function(lines, regtype)
    vim.rpcnotify(1, 'clipboard_write', lines, regtype)
  end

  _G.NvimWebClipboardPaste = function()
    -- This sends a request to host, which asks browser, and returns result
    local ok, result = pcall(vim.rpcrequest, 1, 'clipboard_read', '')
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

  ---------------------------------------------------------------------------
  -- Browser Integration Commands (Phase 12.3)
  ---------------------------------------------------------------------------

  -- :WebShare - Copy session URL to clipboard for sharing
  vim.api.nvim_create_user_command("WebShare", function(args)
    local viewer_mode = args.bang and "?viewer=1" or ""
    -- Request session URL from host
    local ok, session_id = pcall(vim.rpcrequest, 1, 'get_session_id')
    if ok and session_id then
      local url = "https://nvim-web.app/?session=" .. session_id .. viewer_mode
      -- Copy to clipboard via browser
      vim.fn.setreg('+', url)
      if args.bang then
        vim.notify("Read-only link copied: " .. url, vim.log.levels.INFO)
      else
        vim.notify("Session link copied: " .. url, vim.log.levels.INFO)
      end
    else
      vim.notify("Could not get session ID", vim.log.levels.WARN)
    end
  end, { bang = true, desc = "Copy session share URL (! for read-only)" })

  -- :WebNotify - Send notification to browser
  vim.api.nvim_create_user_command("WebNotify", function(args)
    local message = args.args
    local level = args.bang and "warn" or "info"
    vim.rpcnotify(1, 'browser_notify', { message = message, level = level })
  end, { nargs = 1, bang = true, desc = "Show browser notification (! for warning)" })

  -- :WebPrint - Trigger browser print dialog
  vim.api.nvim_create_user_command("WebPrint", function()
    vim.rpcnotify(1, 'browser_print')
    vim.notify("Print dialog triggered", vim.log.levels.INFO)
  end, { desc = "Open browser print dialog" })

  -- :WebFullscreen - Toggle browser fullscreen
  vim.api.nvim_create_user_command("WebFullscreen", function()
    vim.rpcnotify(1, 'browser_fullscreen')
  end, { desc = "Toggle browser fullscreen" })

  -- :WebViewers - Show connected viewers (for collaboration)
  vim.api.nvim_create_user_command("WebViewers", function()
    local ok, viewers = pcall(vim.rpcrequest, 1, 'get_viewers')
    if ok and type(viewers) == 'table' then
      if #viewers == 0 then
        vim.notify("No viewers connected", vim.log.levels.INFO)
      else
        vim.notify("Connected viewers: " .. #viewers, vim.log.levels.INFO)
        for i, v in ipairs(viewers) do
          print(i .. ". " .. (v.name or v.id or "anonymous"))
        end
      end
    else
      vim.notify("Could not get viewer list", vim.log.levels.WARN)
    end
  end, { desc = "List connected session viewers" })

end

-- Status function for statusline integration
-- Usage: set statusline+=%{nvim_web#status()}
function M.status()
  local name = vim.api.nvim_buf_get_name(0)
  if name:match("^vfs://") then
    local backend = name:match("^vfs://([^/]+)")
    return "[" .. (backend or "vfs"):upper() .. "]"
  end
  return ""
end

-- Expose status for vimscript: nvim_web#status()
_G.nvim_web = M

return M


