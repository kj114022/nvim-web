#[derive(Clone)]
pub struct Cell {
    pub ch: char,
    pub hl_id: Option<u32>,  // Highlight ID from grid_line
    pub selected: bool,
}

impl Cell {
    pub fn new() -> Self {
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

#[derive(Clone)]
pub struct Grid {
    pub rows: usize,
    pub cols: usize,
    pub cells: Vec<Cell>,
    pub cursor_row: usize,
    pub cursor_col: usize,
    pub is_focused: bool,
}

impl Grid {
    pub fn new(rows: usize, cols: usize) -> Self {
        Self {
            rows,
            cols,
            cells: vec![Cell::new(); rows * cols],
            cursor_row: 0,
            cursor_col: 0,
            is_focused: true,
        }
    }

    /// Set cell character (legacy, used by existing code)
    #[allow(dead_code)]
    pub fn set(&mut self, row: usize, col: usize, ch: char) {
        if row < self.rows && col < self.cols {
            self.cells[row * self.cols + col].ch = ch;
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

    /// Clear all selection
    #[allow(dead_code)]
    pub fn clear_selection(&mut self) {
        for cell in &mut self.cells {
            cell.selected = false;
        }
    }

    /// Set selection for a cell
    #[allow(dead_code)]
    pub fn set_selected(&mut self, row: usize, col: usize, selected: bool) {
        if row < self.rows && col < self.cols {
            self.cells[row * self.cols + col].selected = selected;
        }
    }

    /// Resize grid, preserving content where possible
    pub fn resize(&mut self, new_rows: usize, new_cols: usize) {
        if new_rows == self.rows && new_cols == self.cols {
            return;
        }

        let mut new_cells = vec![Cell::new(); new_rows * new_cols];

        // Copy existing content
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

        // Clamp cursor to new bounds
        self.cursor_row = self.cursor_row.min(new_rows.saturating_sub(1));
        self.cursor_col = self.cursor_col.min(new_cols.saturating_sub(1));
    }
}
