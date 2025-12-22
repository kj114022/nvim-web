# The Great WebSocket Reconnection Debug Saga

> A memoir of the multi-session debugging journey that fixed browser refresh failures in nvim-web.

## The Problem

Users reported that the Neovim web UI would:
1. Work perfectly on first page load
2. Show blank screen on browser refresh (F5)
3. Require full restart of the host to work again

The browser agent could open new tabs successfully, but manual refresh always failed.

## Timeline of Investigation

### Phase 1: Initial Symptoms
- Host logs showed: `HOST: SENDING X bytes TO WS` - data was flowing
- Browser console showed: `WS OPEN` - connection established
- But canvas remained blank - no rendering

### Phase 2: The Grok Analysis
We consulted Grok's analysis (`grok.md`) which identified potential issues with `grid_line` parsing. However, our implementation was already correct per modern Neovim linegrid protocol.

### Phase 3: The False Leads
Several red herrings consumed debugging time:
- Canvas sizing issues (fixed but not the root cause)
- Grid parsing bugs (already correct)
- Browser caching (hard refresh didn't help)

### Phase 4: The Breakthrough
Host logs revealed the critical clue:
```
HOST: Channel send failed!
```

This only appeared on reconnection, not first load.

### Phase 5: Root Cause Analysis

The architecture had a fatal flaw:

```
serve() -> accept connection -> bridge()
                                   |
                                   +-> Creates channels (to_ws_tx, to_ws_rx)
                                   +-> Spawns Neovim reader thread
                                   +-> Thread sends to to_ws_tx
                                   |
                              (browser refreshes)
                                   |
                              bridge() exits
                                   |
                              CHANNELS DROPPED
                                   |
                              BUT THREAD STILL RUNNING
                                   |
                              Thread tries to send to dead channel
                                   |
                              "Channel send failed!"
```

On reconnection:
- New `bridge()` call created NEW channels
- NEW Neovim reader thread spawned
- But OLD thread still running with DEAD channel
- OLD thread's sends all failed
- NEW thread couldn't receive what OLD thread was reading

## The Fix

Refactored `ws.rs` with persistent architecture:

```rust
pub fn serve(nvim: &mut Nvim) -> Result<()> {
    // Attach UI ONCE
    crate::rpc::attach_ui(&mut nvim.stdin)?;
    
    // Create PERSISTENT channel
    let (nvim_tx, nvim_rx) = mpsc::channel::<Vec<u8>>();
    let nvim_rx = Arc::new(Mutex::new(nvim_rx));
    
    // Spawn reader ONCE - lives across all connections
    thread::spawn(move || {
        loop {
            let msg = crate::rpc::read_message(&mut nvim_stdout);
            let _ = nvim_tx.send(bytes); // Never fails - channel lives
        }
    });
    
    // Accept connections in loop
    loop {
        let websocket = accept(stream);
        
        // Drain stale messages
        while nvim_rx.try_recv().is_ok() {}
        
        // Force full redraw
        crate::rpc::send_notification("nvim_ui_try_resize", ...);
        
        // Run bridge with shared channel
        bridge(nvim, &mut websocket, nvim_rx.clone());
    }
}
```

Key changes:
1. **Neovim reader thread spawned in `serve()`** - persists across connections
2. **Channel wrapped in `Arc<Mutex<>>`** - shared across bridge calls
3. **Drain stale messages** - new client gets fresh state
4. **Force redraw** - ensures UI syncs on reconnect

## Verification

Browser automation test confirmed:
- First load: Working
- After refresh: Working

## Commits

1. `f72832e` - Cleanup: remove orphaned files, restore index.html
2. `9920aa5` - Phase 9: mouse support, canvas clear fix
3. `c66c2a8` - Phase 9: complete UI polish
4. `1399f2b` - Fix: initial viewport resize
5. `07d19e6` - **THE FIX**: refactor bridge for stable reconnection

## Lessons Learned

1. **Threads outlive function scope** - spawned threads don't die when the function returns
2. **Channels die with their owners** - must Arc/Mutex for cross-scope sharing
3. **Test reconnection explicitly** - first connection success hides reconnection bugs
4. **Trust user reports** - browser agent tests masked the real behavior

## Files Modified

- `host/src/ws.rs` - Major refactor (112 insertions, 97 deletions)

## Test Coverage Added

- `host/tests/ws_reconnection.rs` - Connection, reconnection, and stress tests

---

*Documented: 2024-12-23*
*nvim-web project for GSoC Neovim Browser Integration*
