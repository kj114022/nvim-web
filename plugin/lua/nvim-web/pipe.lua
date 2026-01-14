-- nvim-web pipe.lua
-- Universal tool pipe wrapper for CLI tools (LLMs, formatters, etc.)

local M = {}

--- Execute a CLI tool with input
--- @param command string The command to execute (e.g., "claude", "gemini-cli")
--- @param args table Array of arguments
--- @param input? string Input to pipe to stdin
--- @param callback? function Callback(err, result) for async execution
--- @return table|nil Result {stdout, stderr, exit_code} for sync, nil for async
function M.exec(command, args, input, callback)
  args = args or {}
  input = input or ""

  -- Build RPC call
  local params = { command, args, input }

  if callback then
    -- Async execution
    vim.rpc_request_async(0, "tool_exec", params, function(err, result)
      if err then
        callback(err, nil)
      else
        callback(nil, result)
      end
    end)
    return nil
  else
    -- Sync execution
    local err, result = vim.fn.rpcrequest(0, "tool_exec", unpack(params))
    if err and err ~= vim.NIL then
      error("tool_exec failed: " .. tostring(err))
    end
    return result
  end
end

--- Execute tool with visual selection as input
--- @param command string The command to execute
--- @param args? table Array of arguments
function M.exec_visual(command, args)
  args = args or {}

  -- Get visual selection
  local start_pos = vim.fn.getpos("'<")
  local end_pos = vim.fn.getpos("'>")
  local lines = vim.fn.getline(start_pos[2], end_pos[2])

  if type(lines) == "string" then
    lines = { lines }
  end

  local input = table.concat(lines, "\n")

  -- Execute and show result in floating window
  M.exec(command, args, input, function(err, result)
    if err then
      vim.notify("Tool error: " .. tostring(err), vim.log.levels.ERROR)
      return
    end

    if result and result.stdout then
      -- Show in floating window
      M.show_output(result.stdout)
    end
  end)
end

--- Show output in floating window
--- @param content string Content to display
function M.show_output(content)
  local lines = vim.split(content, "\n")
  local buf = vim.api.nvim_create_buf(false, true)
  vim.api.nvim_buf_set_lines(buf, 0, -1, false, lines)

  local width = math.min(80, vim.o.columns - 4)
  local height = math.min(#lines, vim.o.lines - 4)

  vim.api.nvim_open_win(buf, true, {
    relative = "editor",
    width = width,
    height = height,
    row = (vim.o.lines - height) / 2,
    col = (vim.o.columns - width) / 2,
    style = "minimal",
    border = "rounded",
  })

  -- Close on q or Escape
  vim.keymap.set("n", "q", "<cmd>close<cr>", { buffer = buf })
  vim.keymap.set("n", "<Esc>", "<cmd>close<cr>", { buffer = buf })
end

--- Setup keymaps and commands
function M.setup()
  -- Commands
  vim.api.nvim_create_user_command("ToolExec", function(opts)
    local args = vim.split(opts.args, " ")
    local cmd = table.remove(args, 1)
    if opts.range > 0 then
      local lines = vim.fn.getline(opts.line1, opts.line2)
      if type(lines) == "string" then lines = { lines } end
      local input = table.concat(lines, "\n")
      M.exec(cmd, args, input, function(err, result)
        if err then
          vim.notify("Error: " .. tostring(err), vim.log.levels.ERROR)
        elseif result then
          M.show_output(result.stdout or "")
        end
      end)
    else
      M.exec(cmd, args, "", function(err, result)
        if err then
          vim.notify("Error: " .. tostring(err), vim.log.levels.ERROR)
        elseif result then
          M.show_output(result.stdout or "")
        end
      end)
    end
  end, { nargs = "+", range = true, desc = "Execute CLI tool" })

  -- Example keymaps (users can override)
  -- <leader>tc - Claude on selection
  vim.keymap.set("v", "<leader>tc", function()
    M.exec_visual("claude", { "-p", "explain this code" })
  end, { desc = "Claude explain selection" })

  -- <leader>tf - Format with prettier
  vim.keymap.set("v", "<leader>tf", function()
    M.exec_visual("prettier", { "--stdin-filepath", vim.fn.expand("%") })
  end, { desc = "Format selection with prettier" })
end

return M
