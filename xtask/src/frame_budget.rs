use std::path::PathBuf;

use crate::perfetto_query;

pub fn frame_budget(args: Vec<PathBuf>) -> bool {
    if let Some(sqlite) = args
        .iter()
        .find(|path| path.extension().and_then(|e| e.to_str()) == Some("sqlite"))
    {
        println!(
            "frame-budget refuses {} — observation truth is .pftrace (docs/PERF.md)",
            sqlite.display()
        );
        return false;
    }
    perfetto_query::bench_gate(args.into_iter().next())
}
