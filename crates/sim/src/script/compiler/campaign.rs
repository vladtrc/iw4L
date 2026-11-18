use super::{Callee, Catalog, Fault, Op, SourceResolver, developer_statement_ranges, link, parse};
use std::collections::{BTreeMap, BTreeSet};

pub(in crate::script) fn unresolved(
    resolver: &impl SourceResolver,
    roots: &[&str],
    catalog: &Catalog,
) -> Result<BTreeMap<String, BTreeSet<String>>, Fault> {
    let parsed = parse(resolver, roots, catalog)?;
    let resolved: Vec<_> = parsed
        .tables
        .pending
        .iter()
        .map(|pending| link(pending, &parsed.names, &parsed.imports, catalog))
        .collect();
    let mut active = BTreeSet::new();
    for function in &parsed.functions {
        let elided = developer_statement_ranges(function, &parsed.tables, &resolved);
        for (pc, (_, op)) in function.code.iter().enumerate() {
            if elided.iter().any(|range| range.contains(&pc)) {
                continue;
            }
            match op {
                Op::Call(Callee::Unlinked(id), ..)
                | Op::Spawn(Callee::Unlinked(id), ..)
                | Op::FunctionRef(id) => {
                    active.insert(*id);
                }
                _ => {}
            }
        }
    }
    let mut failures: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for id in active {
        if let Err(message) = &resolved[id as usize] {
            failures
                .entry(message.clone())
                .or_default()
                .insert(parsed.tables.pending[id as usize].module.clone());
        }
    }
    Ok(failures)
}
