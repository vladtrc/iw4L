use frame::ControllerIcon;

pub(crate) const SIZE: u32 = 64;

pub(crate) fn rasterize(icon: ControllerIcon) -> Vec<u8> {
    let svg = match icon {
        ControllerIcon::A => include_str!("../icons/xbox/a.svg"),
        ControllerIcon::B => include_str!("../icons/xbox/b.svg"),
        ControllerIcon::X => include_str!("../icons/xbox/x.svg"),
        ControllerIcon::Y => include_str!("../icons/xbox/y.svg"),
        ControllerIcon::DpadUp => include_str!("../icons/xbox/dpad_up.svg"),
        ControllerIcon::DpadDown => include_str!("../icons/xbox/dpad_down.svg"),
        ControllerIcon::DpadLeft => include_str!("../icons/xbox/dpad_left.svg"),
        ControllerIcon::DpadRight => include_str!("../icons/xbox/dpad_right.svg"),
    };
    let tree = resvg::usvg::Tree::from_str(svg, &resvg::usvg::Options::default())
        .expect("embedded controller SVG");
    let mut pixmap = resvg::tiny_skia::Pixmap::new(SIZE, SIZE).expect("controller icon pixmap");
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(
            SIZE as f32 / tree.size().width(),
            SIZE as f32 / tree.size().height(),
        ),
        &mut pixmap.as_mut(),
    );
    let mut rgba = pixmap.take();
    for pixel in rgba.as_chunks_mut::<4>().0.iter_mut() {
        let alpha = u32::from(pixel[3]);
        if let Some(alpha) = std::num::NonZeroU32::new(alpha) {
            for channel in &mut pixel[..3] {
                *channel =
                    ((u32::from(*channel) * 255 + alpha.get() / 2) / alpha.get()).min(255) as u8;
            }
        }
    }
    rgba
}
