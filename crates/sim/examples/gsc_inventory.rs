use sim::gsc_ir::{Catalog, FileSources, Iw4Startup, NativeRegistry, Program};
fn main() {
    let output = std::path::Path::new("context/artifacts/2026-09-25-mp-gsc-semantic-port/12-NATIVE-ABI-AND-LEDGER-PART");
    let sources = FileSources("context/externals/gsc/iw4".into());
    for mode in ["dm", "dd", "dom"] {
        let startup = Iw4Startup::new(&sources, mode, "mp_overgrown");
        let roots: Vec<&str> = startup.roots.iter().map(String::as_str).collect();
        let program = match Program::load(&sources, &roots, &Catalog::iw4()) {
            Ok(program) => program,
            Err(fault) => { println!("LINK {mode}: {fault}"); continue; }
        };
        let entries: Vec<&str> = startup.entries.iter().map(String::as_str).collect();
        let ledger = program.native_ledger(&NativeRegistry::default(), &entries);
        let mut text = String::from("namespace\tname\towner\tsites\treachable_sites\tbound\tfirst_site\n");
        for row in &ledger {
            let site = row.first_site.as_ref().map_or(String::new(), |l| format!("{}:{} {}", l.module, l.line, l.function));
            text += &format!("{:?}\t{}\t{:?}\t{}\t{}\t{}\t{}\n", row.builtin.namespace, row.builtin.name, row.builtin.owner, row.sites, row.reachable_sites, row.bound, site);
        }
        std::fs::write(output.join(format!("ledger-{mode}.tsv")), text).unwrap();
        println!("LEDGER {mode}: modules={} functions={} natives={} reachable={} bound={} unresolved_names=0",
            program.modules().len(), program.function_count(), ledger.len(),
            ledger.iter().filter(|r| r.reachable_sites > 0).count(), ledger.iter().filter(|r| r.bound).count());
    }
}
