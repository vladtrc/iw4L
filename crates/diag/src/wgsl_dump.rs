pub fn dump_pass_wgsl(vertex_name: &str, pixel_name: &str, source: &str) {
    let Ok(want) = std::env::var("IW4L_WGSL_DUMP") else {
        return;
    };
    if !vertex_name.contains(&want) && !pixel_name.contains(&want) {
        return;
    }
    let sanitize = |name: &str| {
        name.chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
            .collect::<String>()
    };
    let directory = std::path::Path::new("iw4l-artifacts/wgsl");
    if let Err(error) = std::fs::create_dir_all(directory) {
        crate::warn!(
            World,
            "IW4L_WGSL_DUMP: cannot create {directory:?}: {error}"
        );
        return;
    }
    let path = directory.join(format!(
        "{}__{}.wgsl",
        sanitize(vertex_name),
        sanitize(pixel_name)
    ));
    match std::fs::write(&path, source) {
        Ok(()) => crate::warn!(
            World,
            "IW4L_WGSL_DUMP: wrote {} bytes of {vertex_name} / {pixel_name} to {path:?}",
            source.len()
        ),
        Err(error) => crate::warn!(World, "IW4L_WGSL_DUMP: cannot write {path:?}: {error}"),
    }
}
