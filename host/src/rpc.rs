use std::io::{Read, Write};
use anyhow::Result;
use rmpv::Value;

pub fn attach_ui(stdin: &mut impl Write) -> Result<()> {
    let msg = Value::Array(vec![
        Value::Integer(0.into()),                 // message type (0 = request)
        Value::Integer(0.into()),                 // request id
        Value::String("nvim_ui_attach".into()),   // method
        Value::Array(vec![
            Value::Integer(80.into()),            // width
            Value::Integer(24.into()),            // height
            Value::Map(vec![                      // opts - enable linegrid extension
                (Value::String("ext_linegrid".into()), Value::Boolean(true)),
            ]),
        ]),
    ]);

    rmpv::encode::write_value(stdin, &msg)?;
    stdin.flush()?;
    println!("Sent nvim_ui_attach request");
    Ok(())
}

pub fn read_message(stdout: &mut impl Read) -> Result<Value> {
    Ok(rmpv::decode::read_value(stdout)?)
}

pub fn is_redraw(msg: &Value) -> bool {
    if let Value::Array(arr) = msg {
        if arr.len() >= 3 {
            if let Value::Integer(msg_type) = &arr[0] {
                if msg_type.as_u64() == Some(2) {
                    if let Value::String(method) = &arr[1] {
                        return method.as_str() == Some("redraw");
                    }
                }
            }
        }
    }
    false
}

pub fn send_input(stdin: &mut impl Write, keys: &str) -> Result<()> {
    let msg = Value::Array(vec![
        Value::Integer(0.into()),
        Value::Integer(1.into()),  // request id
        Value::String("nvim_input".into()),
        Value::Array(vec![Value::String(keys.into())]),
    ]);
    rmpv::encode::write_value(stdin, &msg)?;
    stdin.flush()?;
    Ok(())
}

pub fn send_resize(stdin: &mut impl Write, rows: u64, cols: u64) -> Result<()> {
    let msg = Value::Array(vec![
        Value::Integer(0.into()),
        Value::Integer(2.into()),  // request id
        Value::String("nvim_ui_try_resize".into()),
        Value::Array(vec![
            Value::Integer(cols.into()),
            Value::Integer(rows.into()),
        ]),
    ]);
    rmpv::encode::write_value(stdin, &msg)?;
    stdin.flush()?;
    Ok(())
}

/// Send a notification to Neovim (no response expected)
pub fn send_notification(stdin: &mut impl Write, method: &str, params: Vec<Value>) -> Result<()> {
    let msg = Value::Array(vec![
        Value::Integer(2.into()),  // Notification type
        Value::String(method.into()),
        Value::Array(params),
    ]);
    rmpv::encode::write_value(stdin, &msg)?;
    stdin.flush()?;
    Ok(())
}

pub fn read_loop(stdout: &mut impl Read) -> Result<()> {
    println!("Starting read loop...");
    loop {
        let value = read_message(stdout)?;
        handle_msg(value);
    }
}

fn handle_msg(msg: Value) {
    if let Value::Array(arr) = &msg {
        // Neovim notifications are [2, method, params]
        if arr.len() >= 3 {
            if let Value::Integer(msg_type) = &arr[0] {
                let msg_type_val = msg_type.as_u64();
                
                // Check for notification (type 2)
                if msg_type_val == Some(2) {
                    if let Value::String(method) = &arr[1] {
                        if method.as_str() == Some("redraw") {
                            println!("REDRAW: {:?}", arr[2]);
                        }
                    }
                }
                // Also check for responses (type 1)
                else if msg_type_val == Some(1) {
                    println!("RESPONSE: {:?}", msg);
                }
            }
        }
    }
}
