//! One reading of an environment switch, for every switch a run is alternated
//! across.
//!
//! Absent, empty, `0`, `false` and `off` are off; anything else is on. The
//! rule lives here once so that two switches cannot disagree about what `=0`
//! means: a pair of runs across a switch that reads `=0` as on is two runs of
//! the same arm, and nothing in the report says so.

/// Whether `name` asks for the behaviour behind it.
pub fn on(name: &str) -> bool {
    match std::env::var(name) {
        Ok(value) => {
            let value = value.trim();
            !(value.is_empty()
                || value == "0"
                || value.eq_ignore_ascii_case("false")
                || value.eq_ignore_ascii_case("off"))
        }
        Err(_) => false,
    }
}
