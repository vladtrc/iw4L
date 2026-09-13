pub const EXPLODER_TARGETNAME: &str = "exploder";

pub const EXPLODERCHUNK_TARGETNAME: &str = "exploderchunk";

pub const EXPLODERCHUNK_VISIBLE_TARGETNAME: &str = "exploderchunk visible";

pub const FX_MODEL: &str = "fx";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExploderActivateAction {
    BrushShow,

    BrushThrow,

    BrushDelete,
}

pub fn effective_script_exploder<'a>(prefab: &'a str, exploder: &'a str) -> &'a str {
    if !prefab.is_empty() { prefab } else { exploder }
}

pub fn setup_exploders_hides(model: &str, targetname: &str, script_exploder: &str) -> bool {
    if script_exploder.is_empty() {
        return false;
    }
    if model == FX_MODEL && targetname != EXPLODERCHUNK_TARGETNAME {
        return true;
    }
    targetname == EXPLODER_TARGETNAME || targetname == EXPLODERCHUNK_TARGETNAME
}

pub fn setup_exploders_notsolid(targetname: &str, script_exploder: &str) -> bool {
    if script_exploder.is_empty() {
        return false;
    }
    targetname == EXPLODER_TARGETNAME || targetname == EXPLODERCHUNK_TARGETNAME
}

pub fn exploder_type(targetname: &str) -> &'static str {
    match targetname {
        EXPLODER_TARGETNAME => EXPLODER_TARGETNAME,
        EXPLODERCHUNK_TARGETNAME => EXPLODERCHUNK_TARGETNAME,
        EXPLODERCHUNK_VISIBLE_TARGETNAME => EXPLODERCHUNK_VISIBLE_TARGETNAME,
        _ => "normal",
    }
}

pub fn exploder_activate_action(targetname: &str) -> ExploderActivateAction {
    match exploder_type(targetname) {
        EXPLODER_TARGETNAME => ExploderActivateAction::BrushShow,
        EXPLODERCHUNK_TARGETNAME | EXPLODERCHUNK_VISIBLE_TARGETNAME => {
            ExploderActivateAction::BrushThrow
        }
        _ => ExploderActivateAction::BrushDelete,
    }
}
