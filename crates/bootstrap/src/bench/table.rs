//! Plain-text column layout. The bench report is read in a terminal and kept in
//! a file that gets diffed against the last run, so columns line up and nothing
//! wraps.

use std::time::Duration;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Align {
    Left,
    Right,
}

pub(crate) struct Table {
    align: Vec<Align>,
    headers: Vec<String>,
    rows: Vec<Vec<String>>,
    indent: usize,
}

impl Table {
    pub(crate) fn new(indent: usize, columns: &[(&str, Align)]) -> Self {
        Self {
            align: columns.iter().map(|(_, align)| *align).collect(),
            headers: columns.iter().map(|(name, _)| (*name).to_owned()).collect(),
            rows: Vec::new(),
            indent,
        }
    }

    pub(crate) fn row<const N: usize>(&mut self, cells: [String; N]) {
        self.rows.push(cells.into_iter().collect());
    }

    /// A row for something that was asked for and did not answer. Every cell
    /// but the name reads MISS, because a metric nobody sampled is not a zero
    /// and a table that prints it as one is worse than no table.
    pub(crate) fn miss(&mut self, name: &str) {
        let mut row = vec![name.to_owned()];
        row.extend(std::iter::repeat_n(
            "MISS".to_owned(),
            self.headers.len().saturating_sub(1),
        ));
        self.rows.push(row);
    }

    pub(crate) fn render(&self, out: &mut Vec<String>) {
        let mut widths: Vec<usize> = self
            .headers
            .iter()
            .map(|name| name.chars().count())
            .collect();
        for row in &self.rows {
            for (index, cell) in row.iter().enumerate() {
                let width = cell.chars().count();
                if width > widths[index] {
                    widths[index] = width;
                }
            }
        }
        out.push(self.line(&self.headers, &widths));
        for row in &self.rows {
            out.push(self.line(row, &widths));
        }
    }

    fn line(&self, cells: &[String], widths: &[usize]) -> String {
        let mut line = " ".repeat(self.indent);
        for (index, cell) in cells.iter().enumerate() {
            if index > 0 {
                line.push_str("  ");
            }
            let pad = widths[index].saturating_sub(cell.chars().count());
            match self.align.get(index).copied().unwrap_or(Align::Left) {
                Align::Left if index + 1 == cells.len() => line.push_str(cell),
                Align::Left => {
                    line.push_str(cell);
                    line.push_str(&" ".repeat(pad));
                }
                Align::Right => {
                    line.push_str(&" ".repeat(pad));
                    line.push_str(cell);
                }
            }
        }
        line.trim_end().to_owned()
    }
}

/// Nanoseconds as milliseconds, the unit every frame number in the report uses.
pub(crate) fn ms(ns: u64) -> String {
    format!("{:.2}", ns as f64 / 1e6)
}

pub(crate) fn secs(duration: Duration) -> String {
    format!("{:.3}s", duration.as_secs_f64())
}

pub(crate) fn percent(part: u64, whole: u64) -> String {
    if whole == 0 {
        return "—".to_owned();
    }
    format!("{:.1}", part as f64 * 100.0 / whole as f64)
}

/// Bytes at the scale a reader can hold: a screenshot in KiB, a zone in MiB.
pub(crate) fn bytes(bytes: u64) -> String {
    if bytes < 1024 * 1024 {
        format!("{} KiB", bytes / 1024)
    } else {
        mib(bytes)
    }
}

pub(crate) fn mib(bytes: u64) -> String {
    let mib = bytes as f64 / (1024.0 * 1024.0);
    if mib >= 1024.0 {
        format!("{:.2}GiB", mib / 1024.0)
    } else {
        format!("{mib:.0}MiB")
    }
}

/// `done/total`, `done`, or nothing — a stage that counted no items says so by
/// staying silent rather than printing a zero it never measured.
pub(crate) fn items(done: u64, total: u64) -> String {
    match (done, total) {
        (0, 0) => String::new(),
        (done, 0) => done.to_string(),
        (done, total) => format!("{done}/{total}"),
    }
}
