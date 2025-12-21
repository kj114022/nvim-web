use std::sync::{Arc, Mutex, Condvar};
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
