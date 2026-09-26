#[must_use]
pub fn copycat_weapnext_bind_active(pm_type: i32, in_killcam: bool) -> bool {
    in_killcam || pm_type > 7
}
