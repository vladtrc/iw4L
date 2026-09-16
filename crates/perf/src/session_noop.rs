use std::path::PathBuf;

const ENV: &str = "IW4L_PERF";

#[derive(Clone, Debug)]
pub struct RunMetadata {
    pub zone: Option<String>,
    pub role: String,
    pub focus: Option<String>,
}

pub fn enabled() -> bool {
    crate::switch::on(ENV)
}

pub fn start(_metadata: RunMetadata) -> Result<Option<PathBuf>, String> {
    if enabled() {
        Err(
            "IW4L_PERF requested, but native Perfetto recording is unavailable in this build"
                .to_owned(),
        )
    } else {
        Ok(None)
    }
}

pub fn flush() -> Result<Option<PathBuf>, String> {
    Ok(None)
}
