use std::sync::{Arc, Mutex, Condvar};
use std::sync::mpsc::Sender;
use std::collections::HashMap;
use std::io::Write;
use anyhow::{Result, Context};
use rmpv::Value;
use lazy_static::lazy_static;

/// Pending RPC response
struct Pending {
    result: Mutex<Option<Result<Value, Value>>>,
    cvar: Condvar,
}

/// RPC state for tracking pending requests
struct RpcState {
    next_id: Mutex<u64>,
    pending: Mutex<HashMap<u64, Arc<Pending>>>,
}

lazy_static! {
    static ref RPC_STATE: RpcState = RpcState {
        next_id: Mutex::new(100), // Start at 100 to avoid conflicts with other RPCs
        pending: Mutex::new(HashMap::new()),
    };
}

/// Make a blocking RPC call to Neovim and wait for response
pub fn rpc_call(
    nvim_stdin: &mut impl Write,
    method: &str,
    params: Vec<Value>,
) -> Result<Value> {
    // Generate request ID and register pending response
    let (id, pending) = {
        let mut next_id_guard = RPC_STATE.next_id.lock().unwrap();
        let id = *next_id_guard;
        *next_id_guard += 1;

        let pending = Arc::new(Pending {
            result: Mutex::new(None),
            cvar: Condvar::new(),
        });

        RPC_STATE.pending.lock().unwrap().insert(id, pending.clone());
        (id, pending)
    };

    // Send request
    let msg = Value::Array(vec![
        Value::Integer(0.into()),  // request type
        Value::Integer(id.into()), // message ID
        Value::String(method.into()),
        Value::Array(params),
    ]);

    rmpv::encode::write_value(nvim_stdin, &msg)?;
    nvim_stdin.flush()?;

    eprintln!("RPC call: {} (id={})", method, id);

    // Block until response arrives
    let result = {
        let mut guard = pending.result.lock().unwrap();
        while guard.is_none() {
            guard = pending.cvar.wait(guard).unwrap();
        }
        guard.take().unwrap()
    };

    // Cleanup
    RPC_STATE.pending.lock().unwrap().remove(&id);

    // Return result or error
    match result {
        Ok(v) => {
            eprintln!("RPC response: {} (id={}) -> success", method, id);
            Ok(v)
        }
        Err(e) => {
            eprintln!("RPC response: {} (id={}) -> error: {:?}", method, id, e);
            anyhow::bail!("RPC error for {}: {:?}", method, e)
        }
    }
}

/// Make a blocking filesystem RPC call via channel to browser and wait for response
/// 
/// Uses the same request ID registry and condvar blocking as rpc_call.
/// Sends via channel instead of direct write for clean ownership separation.
pub fn fs_rpc_call_via_channel(
    ws_tx: &Sender<Vec<u8>>,
    op_type: &str,
    params: Vec<Value>,
) -> Result<Value> {
    // Generate request ID and register pending response
    let (id, pending) = {
        let mut next_id_guard = RPC_STATE.next_id.lock().unwrap();
        let id = *next_id_guard;
        *next_id_guard += 1;

        let pending = Arc::new(Pending {
            result: Mutex::new(None),
            cvar: Condvar::new(),
        });

        RPC_STATE.pending.lock().unwrap().insert(id, pending.clone());
        (id, pending)
    };

    // Build FS request message
    // Format: [2, id, [op_type, ns, path, data?]]
    let msg = Value::Array(vec![
        Value::Integer(2.into()),  // FS request type (distinguish from RPC)
        Value::Integer(id.into()), // message ID
        Value::Array(params),      // [op_type, ns, path, data?]
    ]);

    // Encode to bytes and send via channel
    let mut buf = Vec::new();
    rmpv::encode::write_value(&mut buf, &msg)?;
    ws_tx.send(buf).context("Failed to send FS request to WS thread")?;

    eprintln!("FS call: {} (id={})", op_type, id);

    // Block until response arrives
    let result = {
        let mut guard = pending.result.lock().unwrap();
        while guard.is_none() {
            guard = pending.cvar.wait(guard).unwrap();
        }
        guard.take().unwrap()
    };

    // Cleanup
    RPC_STATE.pending.lock().unwrap().remove(&id);

    // Return result or error
    match result {
        Ok(v) => {
            eprintln!("FS response: {} (id={}) -> success", op_type, id);
            Ok(v)
        }
        Err(e) => {
            eprintln!("FS response: {} (id={}) -> error: {:?}", op_type, id, e);
            anyhow::bail!("FS error for {}: {:?}", op_type, e)
        }
    }
}

/// Handle RPC response message from Neovim
pub fn handle_rpc_response(msg: &Value) -> Result<()> {
    if let Value::Array(arr) = msg {
        if arr.len() >= 4 {
            if let Value::Integer(msgid_val) = &arr[1] {
                let msgid = msgid_val.as_u64().context("Invalid message ID")?;
                let error = arr[2].clone();
                let result = arr[3].clone();

                // Find pending request and complete it
                if let Some(pending) = RPC_STATE.pending.lock().unwrap().get(&msgid).cloned() {
                    let mut guard = pending.result.lock().unwrap();
                    *guard = if error.is_nil() {
                        Some(Ok(result))
                    } else {
                        Some(Err(error))
                    };
                    pending.cvar.notify_one();
                    eprintln!("Completed RPC response for msgid={}", msgid);
                }
            }
        }
    }
    Ok(())
}

/// Handle FS response message from Browser
/// 
/// Format: { ok: true, result: ..., id: 123 } or { ok: false, error: "...", id: 123 }
pub fn handle_fs_response(msg: &Value) -> Result<()> {
    // Parse msgpack encoded FS response
    // Expected format from browser: [3, id, ok, result/error]
    if let Value::Array(arr) = msg {
        if arr.len() >= 4 {
            if let Value::Integer(msgid_val) = &arr[1] {
                let msgid = msgid_val.as_u64().context("Invalid message ID")?;
                let ok = if let Value::Boolean(b) = &arr[2] { *b } else { false };
                let data = arr[3].clone();

                // Find pending request and complete it
                if let Some(pending) = RPC_STATE.pending.lock().unwrap().get(&msgid).cloned() {
                    let mut guard = pending.result.lock().unwrap();
                    *guard = if ok {
                        Some(Ok(data))
                    } else {
                        Some(Err(data))
                    };
                    pending.cvar.notify_one();
                    eprintln!("Completed FS response for msgid={}", msgid);
                }
            }
        }
    }
    Ok(())
}
