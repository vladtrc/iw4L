extern crate alloc;

use alloc::string::String;

pub const KEY_UNBOUND: &str = "KEY_UNBOUND";

pub fn replace_directive(src: &str, mut resolve: impl FnMut(&str) -> String) -> String {
    let mut out = String::new();
    let mut rest = src;
    while let Some(start) = rest.find("[{") {
        let after_open = &rest[start + 2..];
        let Some(end) = after_open.find("}]") else {
            out.push_str(rest);
            return out;
        };
        out.push_str(&rest[..start]);
        let directive = &after_open[..end];
        if directive.is_empty() {
            out.push_str("[{}]");
        } else {
            out.push_str(&resolve(directive));
        }
        rest = &after_open[end + 2..];
    }
    out.push_str(rest);
    out
}

pub fn unbound_directive(unbound_localized: &str, directive: &str) -> String {
    alloc::format!("{unbound_localized}({directive})")
}

pub fn default_mp_key_binding(command: &str) -> Option<&'static str> {
    match command {
        "+activate" => Some("F"),

        "+gostand" => Some("SPACE"),
        _ => None,
    }
}

#[must_use]
pub fn hudelem_default_text_scale(elem_font_scale: f32) -> f32 {
    crate::hudelem_text_scale(0, elem_font_scale)
}
