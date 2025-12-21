use std::io::Write;
use anyhow::Result;
use rmpv::Value;

// ... existing functions ...

/// Buffer operations for VFS management

/// Create a new buffer
pub fn create_buffer(stdin: &mut impl Write, listed: bool, scratch: bool) -> Result<()> {
    let msg = Value::Array(vec![
        Value::Integer(0.into()),  // request
        Value::Integer(10.into()), // request id
        Value::String("nvim_create_buf".into()),
        Value::Array(vec![
            Value::Boolean(listed),
            Value::Boolean(scratch),
        ]),
    ]);
    rmpv::encode::write_value(stdin, &msg)?;
    stdin.flush()?;
    Ok(())
}

/// Set buffer name
pub fn set_buffer_name(stdin: &mut impl Write, bufnr: u32, name: &str) -> Result<()> {
    let msg = Value::Array(vec![
        Value::Integer(0.into()),
        Value::Integer(11.into()),
        Value::String("nvim_buf_set_name".into()),
        Value::Array(vec![
            Value::Integer(bufnr.into()),
            Value::String(name.into()),
        ]),
    ]);
    rmpv::encode::write_value(stdin, &msg)?;
    stdin.flush()?;
    Ok(())
}

/// Set buffer option
pub fn set_buffer_option(stdin: &mut impl Write, bufnr: u32, name: &str, value: Value) -> Result<()> {
    let msg = Value::Array(vec![
        Value::Integer(0.into()),
        Value::Integer(12.into()),
        Value::String("nvim_buf_set_option".into()),
        Value::Array(vec![
            Value::Integer(bufnr.into()),
            Value::String(name.into()),
            value,
        ]),
    ]);
    rmpv::encode::write_value(stdin, &msg)?;
    stdin.flush()?;
    Ok(())
}

/// Set buffer lines
pub fn set_buffer_lines(stdin: &mut impl Write, bufnr: u32, start: i64, end: i64, lines: Vec<String>) -> Result<()> {
    let line_values: Vec<Value> = lines.into_iter()
        .map(|s| Value::String(s.into()))
        .collect();
    
    let msg = Value::Array(vec![
        Value::Integer(0.into()),
        Value::Integer(13.into()),
        Value::String("nvim_buf_set_lines".into()),
        Value::Array(vec![
            Value::Integer(bufnr.into()),
            Value::Integer(start.into()),
            Value::Integer(end.into()),
            Value::Boolean(false), // strict_indexing
            Value::Array(line_values),
        ]),
    ]);
    rmpv::encode::write_value(stdin, &msg)?;
    stdin.flush()?;
    Ok(())
}

/// Get buffer lines
pub fn get_buffer_lines(stdin: &mut impl Write, bufnr: u32, start: i64, end: i64) -> Result<()> {
    let msg = Value::Array(vec![
        Value::Integer(0.into()),
        Value::Integer(14.into()),
        Value::String("nvim_buf_get_lines".into()),
        Value::Array(vec![
            Value::Integer(bufnr.into()),
            Value::Integer(start.into()),
            Value::Integer(end.into()),
            Value::Boolean(false),
        ]),
    ]);
    rmpv::encode::write_value(stdin, &msg)?;
    stdin.flush()?;
    Ok(())
}

/// Set current buffer
pub fn set_current_buffer(stdin: &mut impl Write, bufnr: u32) -> Result<()> {
    let msg = Value::Array(vec![
        Value::Integer(0.into()),
        Value::Integer(15.into()),
        Value::String("nvim_set_current_buf".into()),
        Value::Array(vec![Value::Integer(bufnr.into())]),
    ]);
    rmpv::encode::write_value(stdin, &msg)?;
    stdin.flush()?;
    Ok(())
}

/// Execute Vim command
pub fn exec_command(stdin: &mut impl Write, cmd: &str) -> Result<()> {
    let msg = Value::Array(vec![
        Value::Integer(0.into()),
        Value::Integer(16.into()),
        Value::String("nvim_command".into()),
        Value::Array(vec![Value::String(cmd.into())]),
    ]);
    rmpv::encode::write_value(stdin, &msg)?;
    stdin.flush()?;
    Ok(())
}
