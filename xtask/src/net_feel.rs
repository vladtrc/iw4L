use std::path::{Path, PathBuf};

use crate::perfetto_query::{
    Table, cell_f64, cell_i64, resolve_processor, resolve_trace, run_sql_file,
};

pub fn net_feel_gate(root: &Path, trace: Option<PathBuf>) -> bool {
    let path = match resolve_trace(root, trace.as_deref()) {
        Ok(p) => p,
        Err(e) => {
            println!("net-feel: {e}");
            return false;
        }
    };
    match load_and_analyze(root, &path) {
        Ok(report) => {
            report.print();
            report.ok
        }
        Err(e) => {
            println!("net-feel RED: {e}");
            false
        }
    }
}

struct FeelReport {
    ok: bool,
    lines: Vec<String>,
}

impl FeelReport {
    fn print(&self) {
        for line in &self.lines {
            println!("{line}");
        }
        if self.ok {
            println!("net-feel: GREEN");
        } else {
            println!("net-feel: RED");
        }
    }
}

struct FeelRow {
    client_clock_debt_ms: Option<f64>,
    ingress_queue_depth: Option<i64>,
    authority_command_time: Option<i64>,
    predicted_command_time: Option<i64>,
}

fn load_and_analyze(root: &Path, trace: &Path) -> Result<FeelReport, String> {
    let processor = resolve_processor(root)?;
    crate::perfetto_query::ensure_trace_health(root, &processor, trace)?;
    let table = run_sql_file(root, &processor, trace, "xtask/perfetto/feel.sql")?;
    Ok(analyze(&parse_feel(&table)))
}

fn parse_feel(table: &Table) -> Vec<FeelRow> {
    table
        .rows
        .iter()
        .map(|cells| FeelRow {
            client_clock_debt_ms: cell_f64(&table.headers, cells, "client_clock_debt_ms"),
            ingress_queue_depth: cell_i64(&table.headers, cells, "ingress_queue_depth"),
            authority_command_time: cell_i64(&table.headers, cells, "authority_command_time"),
            predicted_command_time: cell_i64(&table.headers, cells, "predicted_command_time"),
        })
        .collect()
}

fn analyze(rows: &[FeelRow]) -> FeelReport {
    let mut lines = Vec::new();
    let max_debt = rows
        .iter()
        .filter_map(|r| r.client_clock_debt_ms)
        .fold(None, |acc: Option<f64>, v| {
            Some(acc.map_or(v, |a| a.max(v)))
        });
    let max_queue = rows.iter().filter_map(|r| r.ingress_queue_depth).max();
    let sampled_q = rows
        .iter()
        .filter(|r| r.ingress_queue_depth.is_some())
        .count();
    let over_two = rows
        .iter()
        .filter(|r| r.ingress_queue_depth.is_some_and(|q| q > 2))
        .count();
    lines.push(format!("max_client_clock_debt_ms={max_debt:?}"));
    lines.push(format!("max_ingress_queue_depth={max_queue:?}"));
    lines.push(format!("ingress_queue_depth>2 rows={over_two}/{sampled_q}"));

    let mut ok = true;
    if let Some(debt) = max_debt
        && debt >= 50.0
    {
        lines.push(format!("RED: client_clock_debt_ms={debt} >= 50"));
        ok = false;
    }

    if let Some(q) = max_queue
        && q > 3
    {
        lines.push(format!("RED: clean localhost ingress_queue_depth={q} > 3"));
        ok = false;
    }

    let predicted_nulls = rows
        .iter()
        .filter(|r| r.predicted_command_time.is_none())
        .count();
    let auth_nulls = rows
        .iter()
        .filter(|r| r.authority_command_time.is_none())
        .count();
    let feel_rows = rows.len();
    if feel_rows == 0 {
        lines.push("RED: no feel events".into());
        ok = false;
    } else if predicted_nulls == feel_rows && auth_nulls == feel_rows {
        lines.push("RED: required domains absent (authority_* and predicted_* all NULL)".into());
        ok = false;
    }

    FeelReport { ok, lines }
}
