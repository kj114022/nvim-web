use std::collections::HashMap;

#[derive(Clone)]
#[allow(dead_code)]
pub struct Cell {
    pub ch: char,
    pub hl_id: Option<u32>,
    pub selected: bool,
}

impl Cell {
    pub const fn new() -> Self {
        Self {
            ch: ' ',
            hl_id: None,
            selected: false,
        }
    }
}

impl Default for Cell {
    fn default() -> Self {
        Self::new()
    }
}

/// Single grid instance with position info for multigrid support
#[derive(Clone)]
#[allow(dead_code)]
pub struct Grid {
    pub id: u32,
    pub rows: usize,
    pub cols: usize,
    pub cells: Vec<Cell>,
    pub cursor_row: usize,
    pub cursor_col: usize,
    pub is_focused: bool,
    // Position in screen coordinates (for floating windows)
    pub row_offset: i32,
    pub col_offset: i32,
    pub is_float: bool,
    pub is_visible: bool,
}

impl Grid {
    pub fn new(id: u32, rows: usize, cols: usize) -> Self {
        Self {
            id,
            rows,
            cols,
            cells: vec![Cell::new(); rows * cols],
            cursor_row: 0,
            cursor_col: 0,
            is_focused: true,
            row_offset: 0,
            col_offset: 0,
            is_float: false,
            is_visible: true,
        }
    }

    /// Set cell with highlight ID
    pub fn set_with_hl(&mut self, row: usize, col: usize, ch: char, hl_id: Option<u32>) {
        if row < self.rows && col < self.cols {
            let cell = &mut self.cells[row * self.cols + col];
            cell.ch = ch;
            cell.hl_id = hl_id;
        }
    }

    /// Clear grid
    pub fn clear(&mut self) {
        for cell in &mut self.cells {
            cell.ch = ' ';
            cell.hl_id = None;
        }
    }

    /// Resize grid, preserving content where possible
    pub fn resize(&mut self, new_rows: usize, new_cols: usize) {
        if new_rows == self.rows && new_cols == self.cols {
            return;
        }

        let mut new_cells = vec![Cell::new(); new_rows * new_cols];
        let copy_rows = self.rows.min(new_rows);
        let copy_cols = self.cols.min(new_cols);
        
        for row in 0..copy_rows {
            for col in 0..copy_cols {
                new_cells[row * new_cols + col] = self.cells[row * self.cols + col].clone();
            }
        }

        self.rows = new_rows;
        self.cols = new_cols;
        self.cells = new_cells;
        self.cursor_row = self.cursor_row.min(new_rows.saturating_sub(1));
        self.cursor_col = self.cursor_col.min(new_cols.saturating_sub(1));
    }

    /// Scroll a region of the grid
    /// rows > 0: scroll up (content moves up, new lines at bottom)
    /// rows < 0: scroll down (content moves down, new lines at top)
    pub fn scroll_region(&mut self, top: usize, bot: usize, left: usize, right: usize, rows: i64) {
        if rows == 0 { return; }
        
        let bot = bot.min(self.rows);
        let right = right.min(self.cols);
        
        if top >= bot || left >= right { return; }
        
        if rows > 0 {
            // Scroll up: copy from row+rows to row
            let scroll = rows as usize;
            for row in top..bot {
                let src_row = row + scroll;
                for col in left..right {
                    if src_row < bot {
                        // Copy cell from source row
                        let src_idx = src_row * self.cols + col;
                        let dst_idx = row * self.cols + col;
                        self.cells[dst_idx] = self.cells[src_idx].clone();
                    } else {
                        // Clear newly exposed rows at bottom
                        let idx = row * self.cols + col;
                        self.cells[idx] = Cell::new();
                    }
                }
            }
        } else {
            // Scroll down: copy from row-scroll to row (iterate in reverse)
            let scroll = (-rows) as usize;
            for row in (top..bot).rev() {
                for col in left..right {
                    if row >= top + scroll {
                        // Copy cell from source row
                        let src_row = row - scroll;
                        let src_idx = src_row * self.cols + col;
                        let dst_idx = row * self.cols + col;
                        self.cells[dst_idx] = self.cells[src_idx].clone();
                    } else {
                        // Clear newly exposed rows at top
                        let idx = row * self.cols + col;
                        self.cells[idx] = Cell::new();
                    }
                }
            }
        }
    }
}

/// Manages multiple grids with z-ordering
pub struct GridManager {
    grids: HashMap<u32, Grid>,
    order: Vec<u32>,  // Z-order: first = bottom, last = top
    active_grid: u32,
}

impl GridManager {
    pub fn new() -> Self {
        // Create default grid 1 (main buffer)
        let mut grids = HashMap::new();
        grids.insert(1, Grid::new(1, 24, 80));
        
        Self {
            grids,
            order: vec![1],
            active_grid: 1,
        }
    }

    /// Get or create grid
    pub fn get_or_create(&mut self, grid_id: u32, rows: usize, cols: usize) -> &mut Grid {
        if let std::collections::hash_map::Entry::Vacant(e) = self.grids.entry(grid_id) {
            e.insert(Grid::new(grid_id, rows, cols));
            self.order.push(grid_id);
        }
        self.grids.get_mut(&grid_id).unwrap()
    }

    /// Get grid immutably
    pub fn get(&self, grid_id: u32) -> Option<&Grid> {
        self.grids.get(&grid_id)
    }

    /// Get grid mutably
    #[allow(dead_code)]
    pub fn get_mut(&mut self, grid_id: u32) -> Option<&mut Grid> {
        self.grids.get_mut(&grid_id)
    }

    /// Resize grid
    pub fn resize_grid(&mut self, grid_id: u32, rows: usize, cols: usize) {
        if let Some(grid) = self.grids.get_mut(&grid_id) {
            grid.resize(rows, cols);
        } else {
            self.grids.insert(grid_id, Grid::new(grid_id, rows, cols));
            if !self.order.contains(&grid_id) {
                self.order.push(grid_id);
            }
        }
    }

    /// Clear grid
    pub fn clear_grid(&mut self, grid_id: u32) {
        if let Some(grid) = self.grids.get_mut(&grid_id) {
            grid.clear();
        }
    }

    /// Position floating window
    pub fn set_float_pos(&mut self, grid_id: u32, row: i32, col: i32) {
        if let Some(grid) = self.grids.get_mut(&grid_id) {
            grid.row_offset = row;
            grid.col_offset = col;
            grid.is_float = true;
            grid.is_visible = true;
            
            // Move to top of z-order
            self.order.retain(|&id| id != grid_id);
            self.order.push(grid_id);
        }
    }

    /// Position embedded window
    pub fn set_win_pos(&mut self, grid_id: u32, row: i32, col: i32) {
        // Create grid if it doesn't exist
        let grid = self.grids.entry(grid_id).or_insert_with(|| {
            if !self.order.contains(&grid_id) {
                // Defer adding to order until we know dimensions
            }
            Grid::new(grid_id, 24, 80)  // Default size, will be resized later
        });
        if !self.order.contains(&grid_id) {
            self.order.push(grid_id);
        }
        grid.row_offset = row;
        grid.col_offset = col;
        grid.is_float = false;
        grid.is_visible = true;
    }

    /// Hide grid
    pub fn hide_grid(&mut self, grid_id: u32) {
        if let Some(grid) = self.grids.get_mut(&grid_id) {
            grid.is_visible = false;
        }
    }

    /// Close/remove grid
    pub fn close_grid(&mut self, grid_id: u32) {
        self.grids.remove(&grid_id);
        self.order.retain(|&id| id != grid_id);
    }

    /// Set cursor position
    pub fn set_cursor(&mut self, grid_id: u32, row: usize, col: usize) {
        self.active_grid = grid_id;
        if let Some(grid) = self.grids.get_mut(&grid_id) {
            grid.cursor_row = row;
            grid.cursor_col = col;
        }
    }

    /// Get grids in z-order (bottom to top)
    pub fn grids_in_order(&self) -> impl Iterator<Item = &Grid> {
        self.order.iter()
            .filter_map(|id| self.grids.get(id))
            .filter(|g| g.is_visible)
    }

    /// Get active grid ID
    pub const fn active_grid_id(&self) -> u32 {
        self.active_grid
    }

    /// Get main grid (grid 1) for resize calculations
    #[allow(dead_code)]
    pub fn main_grid(&self) -> Option<&Grid> {
        self.grids.get(&1)
    }

    /// Get main grid mutably
    #[allow(dead_code)]
    pub fn main_grid_mut(&mut self) -> Option<&mut Grid> {
        self.grids.get_mut(&1)
    }

    /// Scroll a region within a grid
    pub fn scroll_region(&mut self, grid_id: u32, top: usize, bot: usize, left: usize, right: usize, rows: i64) {
        if let Some(grid) = self.grids.get_mut(&grid_id) {
            grid.scroll_region(top, bot, left, right, rows);
        }
    }
}

impl Default for GridManager {
    fn default() -> Self {
        Self::new()
    }
}
