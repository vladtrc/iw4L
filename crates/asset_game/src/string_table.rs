#[derive(Clone, Debug, Default)]
pub struct CapturedStringTable {
    pub name: String,
    pub columns: usize,
    pub rows: usize,
    pub cells: Vec<String>,
}

impl CapturedStringTable {
    #[must_use]
    pub fn cell(&self, row: i32, col: i32) -> &str {
        if row < 0 || col < 0 {
            return "";
        }
        let row = row as usize;
        let col = col as usize;
        if col >= self.columns || row >= self.rows {
            return "";
        }
        self.cells
            .get(row.saturating_mul(self.columns).saturating_add(col))
            .map(String::as_str)
            .unwrap_or("")
    }

    #[must_use]
    pub fn lookup_col(&self, key: &str, col: i32) -> &str {
        match self.lookup_row(key) {
            Some(row) => self.cell(row, col),
            None => "",
        }
    }

    #[must_use]
    pub fn lookup_row(&self, key: &str) -> Option<i32> {
        self.lookup_row_in_col(0, key)
    }

    #[must_use]
    pub fn lookup_row_in_col(&self, col: i32, key: &str) -> Option<i32> {
        for row in 0..self.rows {
            if self.cell(row as i32, col).eq_ignore_ascii_case(key) {
                return Some(row as i32);
            }
        }
        None
    }
}
