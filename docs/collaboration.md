# Real-Time Collaboration

nvim-web supports real-time collaborative editing using CRDTs (Conflict-free Replicated Data Types).

## How It Works

```
  ┌─────────────┐         ┌─────────────┐         ┌─────────────┐
  │   Client A  │         │   Server    │         │   Client B  │
  │  (Browser)  │         │ (nvim-web)  │         │  (Browser)  │
  └──────┬──────┘         └──────┬──────┘         └──────┬──────┘
         │                       │                       │
         │◄──── SyncStep1 ──────►│◄──── SyncStep1 ──────►│
         │     (state vector)    │     (state vector)    │
         │                       │                       │
         │◄──── SyncStep2 ──────►│◄──── SyncStep2 ──────►│
         │    (missing updates)  │    (missing updates)  │
         │                       │                       │
         │                       │                       │
         │──── Update ──────────►│                       │
         │    (local edit)       │──── Update ──────────►│
         │                       │   (broadcast)         │
         │                       │                       │
         │                       │◄──── Update ──────────│
         │◄──── Update ──────────│    (remote edit)      │
         │   (broadcast)         │                       │
         │                       │                       │
```

## CRDT Model

We use [y-crdt](https://github.com/y-crdt/y-crdt) (Yjs Rust port) for:

- **Text operations** - Character-level insertions/deletions
- **Automatic merging** - No conflicts, deterministic resolution
- **Offline support** - Changes sync when reconnected

## API

### Buffer CRDT

```rust
use nvim_web_host::crdt::BufferCrdt;

// Create CRDT for buffer
let mut crdt = BufferCrdt::new(buffer_id);

// Set initial content
crdt.set_content("hello world");

// Apply Neovim buffer change
let update = crdt.apply_nvim_delta(
    start_line,
    end_line,
    new_lines
);

// Sync to other clients
broadcast(update);
```

### Sync Protocol

```rust
use nvim_web_host::crdt::{CrdtSync, SyncMessage};

let sync = CrdtSync::new(buffer_id);

// Initial sync (new client)
let msg = CrdtSync::create_sync_step1(&crdt);
send_to_server(msg);

// Handle incoming message
match sync.handle_message(incoming, &mut crdt)? {
    Some(response) => send_to_client(response),
    None => {}
}
```

## Message Types

| Type | Direction | Description |
|------|-----------|-------------|
| `sync1` | Client → Server | Client's state vector |
| `sync2` | Server → Client | Missing updates |
| `update` | Bidirectional | Incremental change |
| `awareness` | Bidirectional | Cursor/presence info |

## Awareness Protocol

Track remote cursors and presence:

```json
{
  "type": "awareness",
  "data": {
    "user_id": "alice",
    "cursor": {
      "line": 10,
      "column": 5
    },
    "selection": {
      "start": {"line": 10, "column": 0},
      "end": {"line": 10, "column": 15}
    },
    "color": "#ff6b6b"
  }
}
```

## Integration with Neovim

### Buffer Attachment

The host attaches to Neovim buffers to receive changes:

```lua
vim.api.nvim_buf_attach(bufnr, false, {
    on_lines = function(_, bufnr, _, start, old_end, new_end, ...)
        -- Convert to CRDT delta
        local lines = vim.api.nvim_buf_get_lines(bufnr, start, new_end, false)
        nvim_web.apply_delta(bufnr, start, old_end, lines)
    end
})
```

### Applying Remote Changes

```lua
-- Received from other client
local function apply_remote_change(bufnr, start_line, end_line, new_lines)
    -- Temporarily disable on_lines callback
    vim.api.nvim_buf_set_lines(bufnr, start_line, end_line, false, new_lines)
end
```

## Performance

- **Incremental sync** - Only changed portions transmitted
- **Binary encoding** - Efficient wire format (y-sync protocol)
- **Batched updates** - Multiple operations combined

## Limitations

- **Large files** - Memory scales with document size
- **High concurrency** - Many simultaneous editors increase merge overhead
- **Undo/redo** - Per-client undo history (not shared)

## Configuration

```toml
[collaboration]
enabled = true
# Maximum buffer size for CRDT (bytes)
max_buffer_size = 10485760  # 10MB
# Sync interval (ms)
sync_interval = 100
# Awareness broadcast interval (ms)
awareness_interval = 1000
```

## Troubleshooting

### Sync Issues

1. Check network connectivity
2. Verify buffer IDs match
3. Check for version mismatches

### Performance

1. Reduce number of concurrent editors
2. Split large files
3. Increase sync interval
