#[derive(Clone)]
pub struct Cell {
    pub ch: char,
}

#[derive(Clone)]
pub struct Grid {
    pub rows: usize,
    pub cols: usize,
    pub cells: Vec<Cell>,
    pub cursor_row: usize,
    pub cursor_col: usize,
}

impl Grid {
    pub fn new(rows: usize, cols: usize) -> Self {
        Self {
            rows,
            cols,
            cells: vec![Cell { ch: ' ' }; rows * cols],
            cursor_row: 0,
            cursor_col: 0,
        }
    }

    pub fn set(&mut self, row: usize, col: usize, ch: char) {
        self.cells[row * self.cols + col].ch = ch;
    }
}
