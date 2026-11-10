use super::*;

pub(super) fn prepare(
    program: &sim::script::Program,
    sources: &Sources,
    xanims: &asset_anim::XAnimCatalog,
) -> Result<Arc<BTreeMap<String, sim::script::ScriptAnimTree>>, InstallRefusal> {
    let mut trees = BTreeMap::new();
    for (name, references) in program.animation_references() {
        let path = format!("animtrees/{name}.atr");
        let source = sources.0.config(&path).ok_or_else(|| {
            InstallRefusal::new(format!("Missing campaign animation tree {path}"))
        })?;
        let tree = asset_anim::compile_script_animtree(source.as_bytes(), references).map_err(
            |error| InstallRefusal::new(format!("Campaign animation tree {path}: {error}")),
        )?;
        let mut missing = Vec::new();
        let definition = tree
            .to_runtime_definition(|_, clip| {
                let result = xanims.clip(asset_core::FamilyId::IW4, clip);
                if result.is_none() {
                    missing.push(clip.to_owned());
                }
                result
            })
            .map_err(|error| {
                InstallRefusal::new(format!("Campaign animation tree {path}: {error}"))
            })?;
        if !missing.is_empty() {
            return Err(InstallRefusal::new(format!(
                "Campaign animation tree {path} has missing or undecodable clips: {}",
                missing.join(" ")
            )));
        }
        let names = tree.nodes().iter().map(|node| node.name.clone()).collect();
        let product = sim::script::ScriptAnimTree::new(
            names,
            definition,
            tree.nodes().iter().map(|node| node.flags).collect(),
        )
        .map_err(InstallRefusal::new)?;
        diag::info!(
            Sim,
            "campaign animation tree: name={name} nodes={} leaves={} bound={}",
            tree.node_count(),
            tree.leaf_count(),
            tree.leaf_count()
        );
        trees.insert(name, product);
    }
    Ok(Arc::new(trees))
}
