-- nvim-web.nvim - Neovim plugin for nvim-web browser integration

local M = {}

-- Configuration
M.config = {
  explorer_width = 30,
  explorer_position = "left",
  default_backend = "local",
}

-- Explorer state
M.state = {
  explorer_open = false,
  explorer_buf = nil,
  explorer_win = nil,
  current_path = "/",
  current_backend = "local",
}

-- Session state
M.session_state = {
  buf = nil,
  win = nil,
}

-- Connection state
M.conn_state = {
  buf = nil,
  win = nil,
  connections = {},
}

function M.setup(opts)
  M.config = vim.tbl_deep_extend("force", M.config, opts or {})
  M.create_commands()
end

function M.create_commands()
  -- File Explorer
  vim.api.nvim_create_user_command("NvimWebExplorer", function(args)
    local backend = args.args ~= "" and args.args or M.config.default_backend
    M.toggle_explorer(backend)
  end, { nargs = "?", complete = function() return {"local", "ssh", "browser"} end })
  
  -- Session commands
  vim.api.nvim_create_user_command("NvimWebSessions", function()
    M.show_sessions()
  end, {})
  
  vim.api.nvim_create_user_command("NvimWebSessionNew", function()
    M.create_session()
  end, {})
  
  vim.api.nvim_create_user_command("NvimWebSessionShare", function()
    M.share_session()
  end, {})
  
  -- SSH commands
  vim.api.nvim_create_user_command("NvimWebSSH", function(args)
    M.mount_ssh(args.args)
  end, { nargs = 1 })
  
  vim.api.nvim_create_user_command("NvimWebConnections", function()
    M.show_connections()
  end, {})
  
  -- Remote commands
  vim.api.nvim_create_user_command("NvimWebConnect", function(args)
    M.connect_remote(args.args)
  end, { nargs = 1 })
end

-- Explorer functions
function M.toggle_explorer(backend)
  if M.state.explorer_open then
    M.close_explorer()
  else
    M.open_explorer(backend)
  end
end

function M.open_explorer(backend)
  backend = backend or M.config.default_backend
  M.state.current_backend = backend
  M.state.current_path = "/"
  
  local buf = vim.api.nvim_create_buf(false, true)
  vim.api.nvim_buf_set_option(buf, "buftype", "nofile")
  vim.api.nvim_buf_set_option(buf, "bufhidden", "wipe")
  vim.api.nvim_buf_set_name(buf, "nvim-web://explorer/" .. backend)
  
  vim.cmd(M.config.explorer_position == "left" and "topleft vsplit" or "botright vsplit")
  vim.cmd("vertical resize " .. M.config.explorer_width)
  local win = vim.api.nvim_get_current_win()
  vim.api.nvim_win_set_buf(win, buf)
  
  M.state.explorer_buf = buf
  M.state.explorer_win = win
  M.state.explorer_open = true
  
  vim.api.nvim_buf_set_option(buf, "modifiable", false)
  vim.api.nvim_win_set_option(win, "number", false)
  vim.api.nvim_win_set_option(win, "relativenumber", false)
  vim.api.nvim_win_set_option(win, "signcolumn", "no")
  vim.api.nvim_win_set_option(win, "cursorline", true)
  
  M.setup_explorer_keymaps(buf)
  M.refresh_explorer()
end

function M.close_explorer()
  if M.state.explorer_win and vim.api.nvim_win_is_valid(M.state.explorer_win) then
    vim.api.nvim_win_close(M.state.explorer_win, true)
  end
  M.state.explorer_open = false
  M.state.explorer_buf = nil
  M.state.explorer_win = nil
end

function M.setup_explorer_keymaps(buf)
  local opts = { buffer = buf, silent = true }
  vim.keymap.set("n", "<CR>", function() M.explorer_action("open") end, opts)
  vim.keymap.set("n", "l", function() M.explorer_action("open") end, opts)
  vim.keymap.set("n", "h", function() M.explorer_action("parent") end, opts)
  vim.keymap.set("n", "-", function() M.explorer_action("parent") end, opts)
  vim.keymap.set("n", "a", function() M.explorer_action("create") end, opts)
  vim.keymap.set("n", "d", function() M.explorer_action("delete") end, opts)
  vim.keymap.set("n", "r", function() M.explorer_action("rename") end, opts)
  vim.keymap.set("n", "y", function() M.explorer_action("copy_path") end, opts)
  vim.keymap.set("n", "R", function() M.refresh_explorer() end, opts)
  vim.keymap.set("n", "q", function() M.close_explorer() end, opts)
  vim.keymap.set("n", "<Esc>", function() M.close_explorer() end, opts)
  vim.keymap.set("n", "1", function() M.switch_backend("local") end, opts)
  vim.keymap.set("n", "2", function() M.switch_backend("ssh") end, opts)
  vim.keymap.set("n", "3", function() M.switch_backend("browser") end, opts)
end

function M.refresh_explorer()
  if not M.state.explorer_buf then return end
  
  local lines = {
    " nvim-web Explorer",
    string.format(" [%s] %s", M.state.current_backend:upper(), M.state.current_path),
    string.rep("-", M.config.explorer_width - 2),
    "",
    " Press 'R' to refresh",
    " Press '1/2/3' for local/ssh/browser",
    " Press 'q' to close",
  }
  
  vim.api.nvim_buf_set_option(M.state.explorer_buf, "modifiable", true)
  vim.api.nvim_buf_set_lines(M.state.explorer_buf, 0, -1, false, lines)
  vim.api.nvim_buf_set_option(M.state.explorer_buf, "modifiable", false)
end

function M.explorer_action(action)
  if action == "parent" then
    local parent = vim.fn.fnamemodify(M.state.current_path, ":h")
    if parent ~= M.state.current_path then
      M.state.current_path = parent
      M.refresh_explorer()
    end
  elseif action == "copy_path" then
    local vfs_path = string.format("vfs://%s%s", M.state.current_backend, M.state.current_path)
    vim.fn.setreg("+", vfs_path)
    vim.notify("Copied: " .. vfs_path, vim.log.levels.INFO)
  end
end

function M.switch_backend(backend)
  M.state.current_backend = backend
  M.state.current_path = "/"
  M.refresh_explorer()
end

-- Session functions
function M.show_sessions()
  local width, height = 60, 15
  local row = math.floor((vim.o.lines - height) / 2)
  local col = math.floor((vim.o.columns - width) / 2)
  
  local buf = vim.api.nvim_create_buf(false, true)
  vim.api.nvim_buf_set_option(buf, "buftype", "nofile")
  vim.api.nvim_buf_set_option(buf, "bufhidden", "wipe")
  
  local win = vim.api.nvim_open_win(buf, true, {
    relative = "editor", width = width, height = height,
    row = row, col = col, style = "minimal", border = "rounded",
    title = " Session Manager ", title_pos = "center",
  })
  
  M.session_state.buf = buf
  M.session_state.win = win
  
  local lines = {
    " nvim-web Sessions",
    string.rep("-", 58),
    "",
    " n - New session   s - Share   q - Close",
    "",
    " Open localhost:8080?session=new for new session",
  }
  
  vim.api.nvim_buf_set_lines(buf, 0, -1, false, lines)
  vim.api.nvim_buf_set_option(buf, "modifiable", false)
  
  local opts = { buffer = buf, silent = true }
  vim.keymap.set("n", "q", function() vim.api.nvim_win_close(win, true) end, opts)
  vim.keymap.set("n", "<Esc>", function() vim.api.nvim_win_close(win, true) end, opts)
  vim.keymap.set("n", "n", function() M.create_session() end, opts)
  vim.keymap.set("n", "s", function() M.share_session() end, opts)
end

function M.create_session()
  vim.notify("New session: open localhost:8080?session=new", vim.log.levels.INFO)
end

function M.share_session()
  local url = "http://localhost:8080?session=<session_id>"
  vim.fn.setreg("+", url)
  vim.notify("Share link copied: " .. url, vim.log.levels.INFO)
end

-- SSH/Connection functions
function M.mount_ssh(uri)
  local name = uri:match("@(.+)") or uri
  name = name:gsub("[:%.]", "_")
  
  M.conn_state.connections[name] = { uri = uri, connected = true }
  vim.notify("SSH: Added '" .. name .. "' (" .. uri .. ")", vim.log.levels.INFO)
end

function M.show_connections()
  local width, height = 65, 18
  local row = math.floor((vim.o.lines - height) / 2)
  local col = math.floor((vim.o.columns - width) / 2)
  
  local buf = vim.api.nvim_create_buf(false, true)
  vim.api.nvim_buf_set_option(buf, "buftype", "nofile")
  vim.api.nvim_buf_set_option(buf, "bufhidden", "wipe")
  
  local win = vim.api.nvim_open_win(buf, true, {
    relative = "editor", width = width, height = height,
    row = row, col = col, style = "minimal", border = "rounded",
    title = " SSH Connections ", title_pos = "center",
  })
  
  M.conn_state.buf = buf
  M.conn_state.win = win
  
  local lines = {
    " SSH Connections",
    string.rep("-", 63),
    "",
    " Name                 URI                           Status",
    string.rep("-", 63),
  }
  
  local count = 0
  for name, conn in pairs(M.conn_state.connections) do
    count = count + 1
    local status = conn.connected and "[connected]" or "[offline]"
    table.insert(lines, string.format(" %-20s %-30s %s", name, conn.uri, status))
  end
  
  if count == 0 then
    table.insert(lines, " (No connections)")
    table.insert(lines, " Press 'a' to add SSH connection")
  end
  
  table.insert(lines, "")
  table.insert(lines, string.rep("-", 63))
  table.insert(lines, " a - Add   c - Connect   d - Disconnect   x - Remove   q - Close")
  
  vim.api.nvim_buf_set_lines(buf, 0, -1, false, lines)
  vim.api.nvim_buf_set_option(buf, "modifiable", false)
  vim.api.nvim_win_set_option(win, "cursorline", true)
  
  local opts = { buffer = buf, silent = true }
  vim.keymap.set("n", "q", function() vim.api.nvim_win_close(win, true) end, opts)
  vim.keymap.set("n", "<Esc>", function() vim.api.nvim_win_close(win, true) end, opts)
  vim.keymap.set("n", "a", function()
    vim.ui.input({ prompt = "SSH URI (user@host): " }, function(uri)
      if uri then M.mount_ssh(uri) end
    end)
  end, opts)
end

-- Remote connection state
M.remote_state = {
  current_url = "ws://127.0.0.1:9001",
  saved_connections = {
    { name = "local", url = "ws://127.0.0.1:9001" },
  },
}

function M.connect_remote(target)
  local url = target
  
  -- Check if target is a saved connection name
  for _, conn in ipairs(M.remote_state.saved_connections) do
    if conn.name == target then
      url = conn.url
      break
    end
  end
  
  -- Validate URL format
  if not url:match("^wss?://") then
    vim.notify("Remote: Invalid URL (must start with ws:// or wss://)", vim.log.levels.ERROR)
    return
  end
  
  M.remote_state.current_url = url
  
  -- Display connection instructions
  local lines = {
    "",
    "  Remote Connection",
    "  " .. string.rep("-", 50),
    "",
    "  Target: " .. url,
    "",
    "  To connect, open your browser to:",
    "",
    "    http://localhost:8080",
    "",
    "  Then in browser console (F12), run:",
    "",
    "    localStorage.setItem('nvim_bridge_url', '" .. url .. "')",
    "    location.reload()",
    "",
    "  " .. string.rep("-", 50),
    "",
  }
  
  vim.notify(table.concat(lines, "\n"), vim.log.levels.INFO)
end

-- Add saved connection
function M.add_remote_connection(name, url)
  table.insert(M.remote_state.saved_connections, { name = name, url = url })
  vim.notify("Remote: Saved connection '" .. name .. "' -> " .. url, vim.log.levels.INFO)
end

-- List saved connections
function M.list_remote_connections()
  local lines = { " Saved Remote Connections:", "" }
  for _, conn in ipairs(M.remote_state.saved_connections) do
    table.insert(lines, string.format("   %-15s %s", conn.name, conn.url))
  end
  vim.notify(table.concat(lines, "\n"), vim.log.levels.INFO)
end

return M

