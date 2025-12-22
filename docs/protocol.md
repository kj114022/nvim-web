# nvim-web Protocol

This document defines the WebSocket protocol used between the nvim-web host (native Neovim process) and the browser UI.

The protocol transports:
- Neovim redraw events
- UI input events
- RPC request/response messages
- VFS request/response messages

This protocol is:
- Transported over a single WebSocket connection
- Encoded using MessagePack
- Request/response oriented where applicable

This protocol does not:
- Reimplement Neovim semantics
- Define editor behavior
- Guarantee backward compatibility across major versions

## Transport

- Transport: WebSocket
- Connection model: single persistent connection
- Direction: bidirectional
- Ordering: messages are processed in receive order

## Encoding

- All messages are encoded using MessagePack
- Message framing is handled by the WebSocket layer
- Binary payloads (e.g. file contents) are sent as MessagePack binary objects

## Message Envelope

All protocol messages are MessagePack-encoded arrays with a type identifier as the first element:

```
[type, ...payload]
```

Where:
- `type` (integer): message type identifier
  - `0`: RPC request
  - `1`: RPC response  
  - `2`: FS request
  - `3`: FS response
  - Redraw events: raw Neovim notification arrays
- `payload`: message-specific data (varies by type)

The presence of a message `id` in requests indicates a request/response pair. Messages without `id` are fire-and-forget.

## Message Categories

### Redraw

Direction: Host → UI  
Source: Neovim `redraw` notifications

Payload:
- Raw redraw event arrays as produced by Neovim
- The UI is responsible for interpretation and rendering

Format:
```
[2, "redraw", [event_arrays...]]
```

### Input

Direction: UI → Host

Purpose:
- Forward user input (keys, mouse, resize) to Neovim

Payload:
- Input events encoded in Neovim-compatible form

Format:
```
[method_name, ...args]
```

Examples:
- `["nvim_input", "<key>"]`
- `["nvim_ui_try_resize", cols, rows]`

### RPC Responses

Direction: Bidirectional  
Request/response: Yes (`id` required)

Used for:
- Synchronous Neovim RPC calls
- Blocking host-side operations

Semantics:
- Exactly one response per request
- Responses are matched using `id`

Format:
```
Request:  [0, id, method, params]
Response: [1, id, error, result]
```

### VFS Operations

Direction: Bidirectional  
Request/response: Yes (`id` required)

Operations:
- `fs_read`: Read file contents
- `fs_write`: Write file contents
- `fs_stat`: Get file metadata
- `fs_list`: List directory contents

Semantics:
- Host initiates requests to browser
- UI (browser) responds with success or error
- Errors do not close the WebSocket connection

Format:
```
Request:  [2, id, [operation, namespace, path, data?]]
Response: [3, id, ok, result]
```

Where:
- `operation`: string operation type (e.g., "fs_read")
- `namespace`: OPFS namespace identifier
- `path`: file path within namespace
- `data`: binary data for write operations
- `ok`: boolean success indicator
- `result`: operation result or error message

## Error Handling

- Errors are returned as responses with `ok: false`
- Errors are surfaced to Neovim using `nvim_err_writeln`
- Errors must not:
  - terminate the WebSocket connection
  - poison global state
  - leave partial VFS state

The host is responsible for preserving editor invariants on failure.

Error flow:
1. Backend operation fails
2. Error returned to VFS handler
3. Error logged to stderr
4. Error notified to Neovim (visible to user)
5. Connection remains open for subsequent operations

## Versioning

- The protocol is versioned implicitly with the nvim-web host
- No backward compatibility is guaranteed across major versions
- Breaking changes must be documented

## Non-Goals

- Supporting arbitrary third-party clients
- Stabilizing a public network API
- Replacing Neovim's internal RPC protocol
- Protocol-level backwards compatibility guarantees
