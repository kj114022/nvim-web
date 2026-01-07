#![allow(clippy::too_many_lines)]
//! Neovim redraw event handling for multigrid support
//! Processes `grid_line`, `grid_cursor_goto`, `grid_clear`, `grid_resize`,
//! `win_pos`, `win_float_pos`, `win_hide`, `win_close` events

use crate::grid::GridManager;
use crate::highlight::{HighlightMap, HighlightAttr};

/// Apply redraw events to grids (multigrid support)
pub fn apply_redraw(grids: &mut GridManager, highlights: &mut HighlightMap, msg: &rmpv::Value) {
    if let rmpv::Value::Array(arr) = msg {
        // Message format: [2, "redraw", [[event, ...args]...]]
        if arr.len() >= 3 {
            if let rmpv::Value::Array(events) = &arr[2] {
                for event in events {
                    if let rmpv::Value::Array(ev) = event {
                        if ev.is_empty() { continue; }
                        
                        if let rmpv::Value::String(name) = &ev[0] {
                            match name.as_str() {
                                Some("hl_attr_define") => {
                                    for call in &ev[1..] {
                                        if let rmpv::Value::Array(args) = call {
                                            if args.len() >= 2 {
                                                if let rmpv::Value::Integer(id) = &args[0] {
                                                    let hl_id = id.as_u64().unwrap_or(0) as u32;
                                                    let attr = parse_hl_attr(&args[1]);
                                                    highlights.define(hl_id, attr);
                                                }
                                            }
                                        }
                                    }
                                }
                                Some("grid_line") => {
                                    for call in &ev[1..] {
                                        if let rmpv::Value::Array(args) = call {
                                            if args.len() >= 4 {
                                                if let (
                                                    rmpv::Value::Integer(grid_id),
                                                    rmpv::Value::Integer(row),
                                                    rmpv::Value::Integer(col_start),
                                                    rmpv::Value::Array(cells)
                                                ) = (&args[0], &args[1], &args[2], &args[3]) {
                                                    let gid = grid_id.as_u64().unwrap_or(1) as u32;
                                                    let row = row.as_u64().unwrap_or(0) as usize;
                                                    let mut col = col_start.as_u64().unwrap_or(0) as usize;
                                                    let mut last_hl_id: Option<u32> = None;
                                                    let the_grid = grids.get_or_create(gid, 24, 80);
                                                    
                                                    for cell in cells {
                                                        if let rmpv::Value::Array(cell_data) = cell {
                                                            if cell_data.is_empty() { continue; }
                                                            
                                                            let text = if let rmpv::Value::String(s) = &cell_data[0] {
                                                                s.as_str().unwrap_or("")
                                                            } else {
                                                                ""
                                                            };
                                                            
                                                            let hl_id = if cell_data.len() >= 2 {
                                                                if let rmpv::Value::Integer(h) = &cell_data[1] {
                                                                    let id = Some(h.as_u64().unwrap_or(0) as u32);
                                                                    last_hl_id = id;
                                                                    id
                                                                } else {
                                                                    last_hl_id
                                                                }
                                                            } else {
                                                                last_hl_id
                                                            };
                                                            
                                                            let repeat = if cell_data.len() >= 3 {
                                                                if let rmpv::Value::Integer(r) = &cell_data[2] {
                                                                    r.as_u64().unwrap_or(1) as usize
                                                                } else {
                                                                    1
                                                                }
                                                            } else {
                                                                1
                                                            };
                                                            
                                                            let ch = if text.is_empty() { ' ' } else {
                                                                text.chars().next().unwrap_or(' ')
                                                            };
                                                            
                                                            for _ in 0..repeat {
                                                                if col < the_grid.cols {
                                                                    the_grid.set_with_hl(row, col, ch, hl_id);
                                                                    col += 1;
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                                Some("grid_cursor_goto") => {
                                    for call in &ev[1..] {
                                        if let rmpv::Value::Array(args) = call {
                                            if args.len() >= 3 {
                                                if let (
                                                    rmpv::Value::Integer(grid_id),
                                                    rmpv::Value::Integer(row),
                                                    rmpv::Value::Integer(col)
                                                ) = (&args[0], &args[1], &args[2]) {
                                                    let gid = grid_id.as_u64().unwrap_or(1) as u32;
                                                    let r = row.as_u64().unwrap_or(0) as usize;
                                                    let c = col.as_u64().unwrap_or(0) as usize;
                                                    grids.set_cursor(gid, r, c);
                                                }
                                            }
                                        }
                                    }
                                }
                                Some("grid_clear") => {
                                    for call in &ev[1..] {
                                        if let rmpv::Value::Array(args) = call {
                                            if !args.is_empty() {
                                                if let rmpv::Value::Integer(grid_id) = &args[0] {
                                                    let gid = grid_id.as_u64().unwrap_or(1) as u32;
                                                    grids.clear_grid(gid);
                                                }
                                            }
                                        }
                                    }
                                }
                                Some("grid_resize") => {
                                    for call in &ev[1..] {
                                        if let rmpv::Value::Array(args) = call {
                                            if args.len() >= 3 {
                                                if let (
                                                    rmpv::Value::Integer(grid_id),
                                                    rmpv::Value::Integer(width),
                                                    rmpv::Value::Integer(height)
                                                ) = (&args[0], &args[1], &args[2]) {
                                                    let gid = grid_id.as_u64().unwrap_or(1) as u32;
                                                    let new_cols = width.as_u64().unwrap_or(80) as usize;
                                                    let new_rows = height.as_u64().unwrap_or(24) as usize;
                                                    grids.resize_grid(gid, new_rows, new_cols);
                                                }
                                            }
                                        }
                                    }
                                }
                                Some("win_pos") => {
                                    for call in &ev[1..] {
                                        if let rmpv::Value::Array(args) = call {
                                            if args.len() >= 4 {
                                                if let (
                                                    rmpv::Value::Integer(grid_id),
                                                    _,
                                                    rmpv::Value::Integer(row),
                                                    rmpv::Value::Integer(col)
                                                ) = (&args[0], &args[1], &args[2], &args[3]) {
                                                    let gid = grid_id.as_u64().unwrap_or(1) as u32;
                                                    let r = row.as_i64().unwrap_or(0) as i32;
                                                    let c = col.as_i64().unwrap_or(0) as i32;
                                                    grids.set_win_pos(gid, r, c);
                                                }
                                            }
                                        }
                                    }
                                }
                                Some("win_float_pos") => {
                                    for call in &ev[1..] {
                                        if let rmpv::Value::Array(args) = call {
                                            if args.len() >= 6 {
                                                if let rmpv::Value::Integer(grid_id) = &args[0] {
                                                    let gid = grid_id.as_u64().unwrap_or(1) as u32;
                                                    let r = if let rmpv::Value::Integer(v) = &args[4] {
                                                        v.as_i64().unwrap_or(0) as i32
                                                    } else if let rmpv::Value::F64(v) = &args[4] {
                                                        *v as i32
                                                    } else { 0 };
                                                    let c = if let rmpv::Value::Integer(v) = &args[5] {
                                                        v.as_i64().unwrap_or(0) as i32
                                                    } else if let rmpv::Value::F64(v) = &args[5] {
                                                        *v as i32
                                                    } else { 0 };
                                                    grids.set_float_pos(gid, r, c);
                                                }
                                            }
                                        }
                                    }
                                }
                                Some("win_hide") => {
                                    for call in &ev[1..] {
                                        if let rmpv::Value::Array(args) = call {
                                            if !args.is_empty() {
                                                if let rmpv::Value::Integer(grid_id) = &args[0] {
                                                    let gid = grid_id.as_u64().unwrap_or(1) as u32;
                                                    grids.hide_grid(gid);
                                                }
                                            }
                                        }
                                    }
                                }
                                Some("win_close") => {
                                    for call in &ev[1..] {
                                        if let rmpv::Value::Array(args) = call {
                                            if !args.is_empty() {
                                                if let rmpv::Value::Integer(grid_id) = &args[0] {
                                                    let gid = grid_id.as_u64().unwrap_or(1) as u32;
                                                    grids.close_grid(gid);
                                                }
                                            }
                                        }
                                    }
                                }
                                Some("grid_scroll") => {
                                    // grid_scroll: [grid, top, bot, left, right, rows, cols]
                                    // Scrolls a region of the grid
                                    for call in &ev[1..] {
                                        if let rmpv::Value::Array(args) = call {
                                            if args.len() >= 6 {
                                                if let (
                                                    rmpv::Value::Integer(grid_id),
                                                    rmpv::Value::Integer(top),
                                                    rmpv::Value::Integer(bot),
                                                    rmpv::Value::Integer(left),
                                                    rmpv::Value::Integer(right),
                                                    rmpv::Value::Integer(rows),
                                                ) = (&args[0], &args[1], &args[2], &args[3], &args[4], &args[5]) {
                                                    let gid = grid_id.as_u64().unwrap_or(1) as u32;
                                                    let top = top.as_u64().unwrap_or(0) as usize;
                                                    let bot = bot.as_u64().unwrap_or(0) as usize;
                                                    let left = left.as_u64().unwrap_or(0) as usize;
                                                    let right = right.as_u64().unwrap_or(0) as usize;
                                                    let rows_scroll = rows.as_i64().unwrap_or(0);
                                                    grids.scroll_region(gid, top, bot, left, right, rows_scroll);
                                                }
                                            }
                                        }
                                    }
                                }
                                Some("msg_set_pos") => {
                                    for call in &ev[1..] {
                                        if let rmpv::Value::Array(args) = call {
                                            if args.len() >= 4 {
                                                // [grid, row, scrolled, sep_char]
                                                if let (
                                                    rmpv::Value::Integer(grid_id),
                                                    rmpv::Value::Integer(row),
                                                    _, // scrolled
                                                    _  // sep_char
                                                ) = (&args[0], &args[1], &args[2], &args[3]) {
                                                    let gid = grid_id.as_u64().unwrap_or(0) as u32;
                                                    let r = row.as_i64().unwrap_or(0) as i32;
                                                    // Message grid is always full width, col 0
                                                    grids.set_win_pos(gid, r, 0);
                                                }
                                            }
                                        }
                                    }
                                }
                                Some("default_colors_set") => {
                                    // default_colors_set: [fg, bg, sp, cterm_fg, cterm_bg]
                                    for call in &ev[1..] {
                                        if let rmpv::Value::Array(args) = call {
                                            if args.len() >= 2 {
                                                let fg = if let rmpv::Value::Integer(v) = &args[0] {
                                                    Some(v.as_u64().unwrap_or(0xCCCCCC) as u32)
                                                } else { None };
                                                let bg = if let rmpv::Value::Integer(v) = &args[1] {
                                                    Some(v.as_u64().unwrap_or(0x1a1a1a) as u32)
                                                } else { None };
                                                highlights.set_default_colors(fg, bg);
                                            }
                                        }
                                    }
                                }
                                Some("flush") => {
                                    // flush event signals end of redraw batch
                                    // No action needed - rendering happens on each redraw event
                                }
                                Some("mode_change") => {
                                    // mode_change: [mode_name, mode_idx]
                                    // Tracks current Neovim mode for cursor rendering
                                    for call in &ev[1..] {
                                        if let rmpv::Value::Array(args) = call {
                                            if !args.is_empty() {
                                                if let rmpv::Value::String(mode_name) = &args[0] {
                                                    if let Some(mode) = mode_name.as_str() {
                                                        grids.set_mode(mode);
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                                Some("mode_info_set") => {
                                    // mode_info_set: [cursor_style_enabled, mode_info]
                                    // Contains cursor style info per mode - ignored for now
                                }
                                Some("win_viewport") => {
                                    // win_viewport: [grid, win, topline, botline, curline, curcol, line_count, scroll_delta]
                                    // Used for smooth scrolling and viewport tracking - ignored for now
                                }
                                unknown => {
                                    web_sys::console::warn_1(&format!("Unhandled event: {unknown:?}").into());
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// Parse highlight attributes from msgpack map
pub fn parse_hl_attr(value: &rmpv::Value) -> HighlightAttr {
    let mut attr = HighlightAttr::default();
    
    if let rmpv::Value::Map(map) = value {
        for (key, val) in map {
            if let rmpv::Value::String(k) = key {
                match k.as_str() {
                    Some("foreground") => {
                        if let rmpv::Value::Integer(i) = val {
                            attr.fg = Some(i.as_u64().unwrap_or(0) as u32);
                        }
                    }
                    Some("background") => {
                        if let rmpv::Value::Integer(i) = val {
                            attr.bg = Some(i.as_u64().unwrap_or(0) as u32);
                        }
                    }
                    Some("bold") => {
                        attr.bold = matches!(val, rmpv::Value::Boolean(true));
                    }
                    Some("italic") => {
                        attr.italic = matches!(val, rmpv::Value::Boolean(true));
                    }
                    Some("underline") => {
                        attr.underline = matches!(val, rmpv::Value::Boolean(true));
                    }
                    _ => {}
                }
            }
        }
    }
    
    attr
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_parse_hl_attr_empty() {
        let value = rmpv::Value::Map(vec![]);
        let attr = parse_hl_attr(&value);
        assert!(attr.fg.is_none());
        assert!(attr.bg.is_none());
        assert!(!attr.bold);
    }
    
    #[test]
    fn test_parse_hl_attr_foreground() {
        let value = rmpv::Value::Map(vec![
            (rmpv::Value::String("foreground".into()), rmpv::Value::Integer(0xFF0000.into())),
        ]);
        let attr = parse_hl_attr(&value);
        assert_eq!(attr.fg, Some(0xFF0000));
    }
}
