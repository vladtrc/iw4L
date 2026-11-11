use super::{Catalog, Fault, SourceResolver, link, parse};
use std::collections::{BTreeMap, BTreeSet};

pub(in crate::script) fn unresolved(
    resolver: &impl SourceResolver,
    roots: &[&str],
    catalog: &Catalog,
) -> Result<BTreeMap<String, BTreeSet<String>>, Fault> {
    let parsed = parse(resolver, roots, catalog)?;
    let mut failures: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for pending in &parsed.tables.pending {
        if let Err(message) = link(pending, &parsed.names, &parsed.imports, catalog) {
            failures
                .entry(message)
                .or_default()
                .insert(pending.module.clone());
        }
    }
    Ok(failures)
}
