use d3d9_sm3::pass_fragment_alpha_test_entry;
pub use d3d9_sm3::{PASS_FRAGMENT_ENTRY, PASS_VERTEX_ENTRY, PassWgsl};
use d3d9_state::AlphaTest;

pub const MATERIAL_ALPHA_TESTS: [AlphaTest; 4] = [
    AlphaTest::from_raw(5, 0),
    AlphaTest::from_raw(2, 128),
    AlphaTest::from_raw(7, 128),
    AlphaTest::from_raw(7, 255),
];

pub fn alpha_test_fragment_entry(alpha_test: Option<AlphaTest>) -> String {
    match alpha_test {
        None => String::from(PASS_FRAGMENT_ENTRY),
        Some(test) => pass_fragment_alpha_test_entry(
            MATERIAL_ALPHA_TESTS
                .iter()
                .position(|entry| *entry == test)
                .expect("uncompiled material alpha-test state"),
        ),
    }
}

pub type ValidatedPassWgsl = PassWgsl;
