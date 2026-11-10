pub use d3d9_sm3::{PASS_FRAGMENT_ENTRY, PASS_VERTEX_ENTRY};
pub type ValidatedPassWgsl = d3d9_sm3::PassWgsl;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WgslValidationError {
    Parse(String),
    Validation(String),
}
impl WgslValidationError {
    pub fn census_key(&self) -> String {
        match self {
            Self::Parse(_) => "Wgsl/WgslParse".to_owned(),
            Self::Validation(_) => "Wgsl/WgslValidation".to_owned(),
        }
    }
}
pub(crate) fn validate_wgsl(source: &str) -> Result<(), WgslValidationError> {
    let module = naga::front::wgsl::parse_str(source)
        .map_err(|error| WgslValidationError::Parse(error.emit_to_string(source)))?;
    naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::all(),
    )
    .validate(&module)
    .map_err(|error| WgslValidationError::Validation(error.to_string()))?;
    Ok(())
}
