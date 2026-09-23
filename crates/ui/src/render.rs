use crate::class_icons::rgba_ui_image;
use crate::game_text_font;
use crate::model::{Content, Enabled, Rect640, Screen, ScreenCmd, Widget, WidgetAnimation};
use crate::nav::{Focusable, UI_PASS_FOCUS, spawn_selection_bar};
use crate::retail_menu::{MenuFrontend, MenuImageCache};
use assets::{LocalizeCatalog, MenuCatalog};
use bevy::prelude::*;

#[derive(Component, Clone, Debug)]
pub(crate) struct PaintedWidget {
    pub id: String,
}

#[derive(Component, Clone, Debug)]
pub(crate) struct WidgetBehavior {
    pub on_activate: Vec<ScreenCmd>,
    pub on_focus: Vec<ScreenCmd>,
}

#[derive(Component, Clone, Debug)]
pub(crate) struct WidgetControl(pub Content);

#[derive(Component, Clone, Debug)]
pub(crate) struct WidgetHelp(pub String);

#[derive(Component)]
pub(crate) struct FocusHelpText;

#[derive(Component, Clone, Copy, Debug)]
pub(crate) struct SliderControl {
    pub key: crate::SettingKey,
    pub min: f32,
    pub max: f32,
}

#[derive(Component)]
pub(crate) struct RetailScrollX {
    origin_px: f32,
    distance_px: f32,
    period_seconds: f32,
}

#[derive(Component)]
pub(crate) struct RetailPulseAlpha {
    rgb: [f32; 3],
    radians_per_second: f32,
}

const UNSUPPORTED_MARK: Color = Color::srgba(0.85, 0.15, 0.55, 0.55);

pub(crate) fn spawn_screen(
    parent: &mut ChildSpawnerCommands,
    loc: &LocalizeCatalog,
    catalog: &MenuCatalog,
    cache: &MenuImageCache,
    frontend: &MenuFrontend,
    screen: &Screen,
    font: &Handle<Font>,
    win_w: f32,
    win_h: f32,
    interactive: bool,
) -> bool {
    let mut used_oxanium = false;
    for widget in &screen.widgets {
        if spawn_widget(
            parent,
            loc,
            catalog,
            cache,
            frontend,
            widget,
            font,
            win_w,
            win_h,
            interactive,
        ) {
            used_oxanium = true;
        }
    }
    used_oxanium
}

fn spawn_widget(
    parent: &mut ChildSpawnerCommands,
    loc: &LocalizeCatalog,
    catalog: &MenuCatalog,
    cache: &MenuImageCache,
    frontend: &MenuFrontend,
    widget: &Widget,
    font: &Handle<Font>,
    win_w: f32,
    win_h: f32,
    interactive: bool,
) -> bool {
    if !widget_is_visible(widget, frontend) {
        return false;
    }
    let (left, top, width, height) = place_rect(&widget.rect, win_w, win_h, widget.style.canvas);
    if width <= 0.0 || height <= 0.0 {
        return false;
    }
    let bg_handle = if widget.style.background.is_empty() {
        None
    } else {
        cache.handles.get(&widget.style.background).cloned()
    };
    let icon_handle = if widget.icon.is_empty() {
        None
    } else {
        cache.handles.get(&widget.icon).cloned()
    };
    let mut label = widget_label(loc, widget);
    let control = matches!(
        widget.content,
        Content::Cycler { .. }
            | Content::Slider { .. }
            | Content::Bind { .. }
            | Content::TextEdit { .. }
    );
    let clickable = interactive && widget.focusable && (!label.is_empty() || control);
    if widget.focusable && label.is_empty() && !control {
        return false;
    }
    let panel_fill = matches!(widget.content, Content::Panel) && widget.style.fore_color[3] > 0.0;
    if !clickable && label.is_empty() && bg_handle.is_none() && icon_handle.is_none() && !panel_fill
    {
        return false;
    }
    let color = if widget.style.fore_color[3] > 0.0 {
        Color::srgba(
            widget.style.fore_color[0].clamp(0.0, 1.0),
            widget.style.fore_color[1].clamp(0.0, 1.0),
            widget.style.fore_color[2].clamp(0.0, 1.0),
            widget.style.fore_color[3].clamp(0.0, 1.0),
        )
    } else {
        Color::srgb(0.92, 0.93, 0.95)
    };
    let contain = widget.style.canvas.scale(win_w, win_h);
    let retail_font = crate::retail_font::catalog_font(
        catalog,
        widget.style.font_enum,
        contain,
        widget.style.text_scale,
    );
    let font_atlas = retail_font.and_then(|def| {
        if widget.id.ends_with("/context_help") {
            return None;
        }
        let stem = assets::AssetRef::bare_name(&def.material);
        let handle = cache.handles.get(stem).cloned()?;
        let &(tw, th) = cache.sizes.get(stem)?;
        Some((def, handle, tw, th))
    });
    let used_oxanium = !label.is_empty() && font_atlas.is_none();
    if widget.style.text_wrap
        && let Some((font, _, _, _)) = &font_atlas
    {
        let glyph_scale =
            crate::retail_font::r_normalized_text_scale(font.pixel_height, widget.style.text_scale)
                * contain;
        let limit = width - widget.style.text_align_x.abs() * contain;
        let mut wrapped = String::new();
        let mut line_width = 0.0;
        let space = font
            .glyph(' ' as u32)
            .map_or(0.0, |g| g.dx as f32 * glyph_scale);
        for word in label.split_whitespace() {
            let word_width: f32 = word
                .chars()
                .filter_map(|ch| font.glyph(ch as u32))
                .map(|g| g.dx as f32 * glyph_scale)
                .sum();
            if line_width > 0.0 {
                if line_width + space + word_width > limit {
                    wrapped.push('\n');
                    line_width = 0.0;
                } else {
                    wrapped.push(' ');
                    line_width += space;
                }
            }
            wrapped.push_str(word);
            line_width += word_width;
        }
        label = wrapped;
    }
    let font_px = (widget.style.text_scale * 48.0 * contain).clamp(8.0, 64.0);
    let unsupported = matches!(widget.content, Content::Unsupported { .. });
    let mut node = parent.spawn((
        Node {
            position_type: PositionType::Absolute,
            left: Val::Px(left),
            top: Val::Px(top),
            width: Val::Px(width),
            height: Val::Px(height),
            justify_content: if widget.style.text_align_mode == 6 {
                JustifyContent::FlexEnd
            } else {
                JustifyContent::FlexStart
            },
            align_items: AlignItems::FlexStart,
            overflow: Overflow::visible(),
            border: if unsupported {
                UiRect::all(Val::Px(2.0))
            } else {
                UiRect::DEFAULT
            },
            ..default()
        },
        PaintedWidget {
            id: widget.id.clone(),
        },
        WidgetBehavior {
            on_activate: widget.on_activate.clone(),
            on_focus: widget.on_focus.clone(),
        },
        WidgetControl(widget.content.clone()),
    ));
    if let Some(help) = &widget.help {
        node.insert(WidgetHelp(help.clone()));
    }
    if let Content::Slider { min, max, key, .. } = widget.content {
        node.insert(SliderControl { key, min, max });
    }
    if let WidgetAnimation::ScrollX {
        period_seconds,
        distance_640,
    } = widget.style.animation
    {
        node.insert(RetailScrollX {
            origin_px: left,
            distance_px: distance_640 * contain,
            period_seconds,
        });
    }
    if unsupported {
        node.insert(BorderColor::all(UNSUPPORTED_MARK));
    }
    if matches!(widget.content, Content::Panel) && widget.style.fore_color[3] > 0.0 {
        node.insert(BackgroundColor(Color::srgba(
            widget.style.fore_color[0].clamp(0.0, 1.0),
            widget.style.fore_color[1].clamp(0.0, 1.0),
            widget.style.fore_color[2].clamp(0.0, 1.0),
            widget.style.fore_color[3].clamp(0.0, 1.0),
        )));
    }
    if clickable {
        node.insert((
            Button,
            Pickable::default(),
            Focusable {
                id: widget.id.clone(),
                x: left,
                y: top,
                w: width,
                h: height,
                order: widget.focus_order,
            },
        ));
    } else {
        node.insert((UI_PASS_FOCUS, Pickable::IGNORE));
    }
    node.with_children(|row| {
        if widget.focusable {
            spawn_selection_bar(row, bg_handle);
        } else if let Some(handle) = bg_handle {
            let (image_w, image_h) = if widget.style.image_contain {
                cache
                    .sizes
                    .get(&widget.style.background)
                    .map(|&(w, h)| {
                        let scale = (width / w as f32).min(height / h as f32);
                        (w as f32 * scale, h as f32 * scale)
                    })
                    .unwrap_or((width, height))
            } else {
                (width, height)
            };
            let mut image = row.spawn((
                ImageNode {
                    image: handle,
                    color,
                    image_mode: bevy::ui::widget::NodeImageMode::Stretch,
                    ..default()
                },
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px((width - image_w) * 0.5),
                    top: Val::Px((height - image_h) * 0.5),
                    width: Val::Px(image_w),
                    height: Val::Px(image_h),
                    ..default()
                },
                UI_PASS_FOCUS,
                Pickable::IGNORE,
            ));
            if let WidgetAnimation::PulseAlpha { radians_per_second } = widget.style.animation {
                image.insert(RetailPulseAlpha {
                    rgb: [
                        widget.style.fore_color[0],
                        widget.style.fore_color[1],
                        widget.style.fore_color[2],
                    ],
                    radians_per_second,
                });
            }
        }
        if let Some(handle) = icon_handle {
            spawn_content_icon(
                row,
                handle,
                width,
                height,
                cache.sizes.get(&widget.icon).copied(),
            );
        }
        if control {
            spawn_control_content(
                row,
                widget,
                font,
                font_px,
                color,
                width,
                height,
                font_atlas.as_ref(),
                contain,
            );
        } else if !label.is_empty() {
            if let Some((def, atlas, tw, th)) = font_atlas {
                spawn_font_label(
                    row,
                    def,
                    atlas,
                    tw,
                    th,
                    &label,
                    color,
                    contain,
                    widget.style.text_scale,
                    width,
                    height,
                    widget.style.text_align_mode,
                    widget.style.text_align_x,
                    widget.style.text_align_y,
                    0.0,
                    0.0,
                );
            } else {
                let mut text = row.spawn((
                    Text::new(label),
                    game_text_font(font, font_px),
                    TextColor(color),
                    Node {
                        margin: if widget.style.text_align_mode == 6 {
                            UiRect::right(Val::Px((-widget.style.text_align_x * contain).max(0.0)))
                        } else {
                            UiRect::left(Val::Px(8.0))
                        },
                        ..default()
                    },
                    UI_PASS_FOCUS,
                    Pickable::IGNORE,
                ));
                if widget.id.ends_with("/context_help") {
                    text.insert(FocusHelpText);
                }
            }
        }
    });
    used_oxanium
}

fn spawn_control_content(
    row: &mut ChildSpawnerCommands,
    widget: &Widget,
    font: &Handle<Font>,
    font_px: f32,
    color: Color,
    width: f32,
    height: f32,
    font_atlas: Option<&(&assets::FontDef, Handle<Image>, u32, u32)>,
    contain: f32,
) {
    let (left, right) = match &widget.content {
        Content::Cycler { label, value, .. } => (label.as_str(), value.as_str()),
        Content::Bind { label, chord, .. } => (label.as_str(), chord.as_str()),
        Content::TextEdit {
            label,
            buffer,
            editing,
            ..
        } => {
            let caret = if *editing { "_" } else { "" };
            spawn_control_text(
                row,
                &format!("{buffer}{caret}"),
                font,
                font_px,
                color,
                width * 0.48,
                font_atlas,
                contain,
                widget.style.text_scale,
                height,
            );
            (label.as_str(), "")
        }
        Content::Slider {
            label,
            value,
            min,
            max,
            ..
        } => {
            let fraction = ((*value - *min) / (*max - *min).max(f32::EPSILON)).clamp(0.0, 1.0);
            row.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Px(width * 0.48),
                    top: Val::Px(height * 0.36),
                    width: Val::Px(width * 0.38),
                    height: Val::Px(4.0),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.75, 0.77, 0.78, 0.35)),
                UI_PASS_FOCUS,
                Pickable::IGNORE,
            ))
            .with_child((
                Node {
                    position_type: PositionType::Absolute,
                    left: Val::Percent(fraction * 100.0),
                    top: Val::Px(-2.0),
                    width: Val::Px(3.0),
                    height: Val::Px(8.0),
                    ..default()
                },
                BackgroundColor(Color::WHITE),
                UI_PASS_FOCUS,
                Pickable::IGNORE,
            ));
            spawn_control_text(
                row,
                &format!("{value:.2}"),
                font,
                font_px * 0.86,
                color,
                width * 0.86,
                font_atlas,
                contain,
                widget.style.text_scale * 0.86,
                height,
            );
            (label.as_str(), "")
        }
        _ => ("", ""),
    };
    let value_x = width
        * if matches!(widget.content, Content::Bind { .. }) {
            0.72
        } else {
            0.48
        };
    let fit = |text: &str, available: f32| {
        let measured = if let Some((def, _, _, _)) = font_atlas {
            let scale = crate::retail_font::r_normalized_text_scale(
                def.pixel_height,
                widget.style.text_scale,
            ) * contain;
            text.chars()
                .filter_map(|ch| def.glyph(ch as u32))
                .map(|glyph| glyph.dx as f32 * scale)
                .sum::<f32>()
        } else {
            text.chars().count() as f32 * font_px * 0.6
        };
        (available / measured.max(1.0)).min(1.0)
    };
    let label_fit = fit(left, value_x - 12.0 * contain);
    let value_fit = fit(right, width - value_x - 4.0 * contain);
    spawn_control_text(
        row,
        left,
        font,
        font_px * label_fit,
        color,
        6.0,
        font_atlas,
        contain,
        widget.style.text_scale * label_fit,
        height,
    );
    if !right.is_empty() {
        spawn_control_text(
            row,
            right,
            font,
            font_px * value_fit,
            color,
            value_x,
            font_atlas,
            contain,
            widget.style.text_scale * value_fit,
            height,
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn spawn_control_text(
    row: &mut ChildSpawnerCommands,
    text: &str,
    fallback: &Handle<Font>,
    fallback_px: f32,
    color: Color,
    x: f32,
    font_atlas: Option<&(&assets::FontDef, Handle<Image>, u32, u32)>,
    contain: f32,
    text_scale: f32,
    height: f32,
) {
    if let Some((def, atlas, tw, th)) = font_atlas {
        spawn_font_label(
            row,
            def,
            atlas.clone(),
            *tw,
            *th,
            text,
            color,
            contain,
            text_scale,
            640.0,
            height,
            4,
            0.0,
            0.0,
            x,
            0.0,
        );
    } else {
        row.spawn((
            Text::new(text),
            game_text_font(fallback, fallback_px),
            TextColor(color),
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(x),
                top: Val::Px(0.0),
                ..default()
            },
            UI_PASS_FOCUS,
            Pickable::IGNORE,
        ));
    }
}

#[derive(Component)]
pub(crate) struct ContentIcon;

fn spawn_content_icon(
    row: &mut ChildSpawnerCommands,
    handle: Handle<Image>,
    width: f32,
    height: f32,
    size: Option<(u32, u32)>,
) {
    let Some((source_w, source_h)) = size else {
        return;
    };
    let scale = (height / source_h as f32).min(width * 0.45 / source_w as f32);
    let icon_h = source_h as f32 * scale;
    let icon_w = source_w as f32 * scale;
    row.spawn((
        ContentIcon,
        ImageNode::new(handle).with_mode(bevy::ui::widget::NodeImageMode::Stretch),
        Node {
            width: Val::Px(icon_w),
            height: Val::Px(icon_h),
            flex_shrink: 0.0,
            margin: UiRect::right(Val::Px(6.0)),
            ..default()
        },
        UI_PASS_FOCUS,
        Pickable::IGNORE,
    ));
}

fn spawn_font_label(
    row: &mut ChildSpawnerCommands,
    font: &assets::FontDef,
    atlas: Handle<Image>,
    tex_w: u32,
    tex_h: u32,
    label: &str,
    color: Color,
    contain: f32,
    text_scale: f32,
    container_width: f32,
    container_height: f32,
    text_align_mode: i32,
    text_align_x: f32,
    text_align_y: f32,
    offset_x: f32,
    offset_y: f32,
) {
    let scale =
        crate::retail_font::r_normalized_text_scale(font.pixel_height, text_scale) * contain;
    if scale <= 0.0 {
        return;
    }
    for (line, label) in label.lines().enumerate() {
        let mut width_chars = label.chars().peekable();
        let mut text_width = 0.0;
        while let Some(letter) = crate::retail_font::next_letter(&mut width_chars) {
            if let Some(glyph) = font.glyph(letter) {
                text_width += glyph.dx as f32 * scale;
            }
        }
        let text_height = text_scale * 48.0 * contain;
        let (mut cursor_x, cursor_y) = crate::retail_font::item_text_origin(
            container_width,
            container_height,
            text_align_mode,
            text_align_x * contain,
            text_align_y * contain,
            text_width,
            text_height,
        );
        cursor_x += offset_x;
        let cursor_y = cursor_y + offset_y + line as f32 * text_height;
        let mut chars = label.chars().peekable();
        while let Some(letter) = crate::retail_font::next_letter(&mut chars) {
            let Some(glyph) = font.glyph(letter) else {
                continue;
            };
            if let Some((x, y, w, h, rect)) = crate::retail_font::glyph_screen_quad(
                glyph,
                cursor_x,
                cursor_y,
                scale,
                tex_w as f32,
                tex_h as f32,
            ) {
                row.spawn((
                    ImageNode {
                        image: atlas.clone(),
                        color,
                        rect: Some(rect),
                        image_mode: bevy::ui::widget::NodeImageMode::Stretch,
                        ..default()
                    },
                    Node {
                        position_type: PositionType::Absolute,
                        left: Val::Px(x),
                        top: Val::Px(y),
                        width: Val::Px(w),
                        height: Val::Px(h),
                        ..default()
                    },
                    UI_PASS_FOCUS,
                    Pickable::IGNORE,
                ));
            }
            cursor_x += glyph.dx as f32 * scale;
        }
    }
}

pub(crate) fn animate_retail_widgets(
    time: Res<Time>,
    mut scrolling: Query<(&RetailScrollX, &mut Node)>,
    mut pulsing: Query<(&RetailPulseAlpha, &mut ImageNode)>,
) {
    let elapsed = time.elapsed_secs();
    for (animation, mut node) in &mut scrolling {
        let phase = (elapsed / animation.period_seconds).rem_euclid(1.0);
        node.left = Val::Px(animation.origin_px + animation.distance_px * phase);
    }
    for (animation, mut image) in &mut pulsing {
        let alpha = ((elapsed * animation.radians_per_second).sin() + 1.0) * 0.25 + 0.25;
        image.color = Color::srgba(animation.rgb[0], animation.rgb[1], animation.rgb[2], alpha);
    }
}

fn widget_label(loc: &LocalizeCatalog, widget: &Widget) -> String {
    let raw = widget.style.text_key.as_str();
    if raw.is_empty() {
        return String::new();
    }

    if !raw.starts_with('@') {
        return raw.to_owned();
    }
    let key = raw.trim_start_matches('@');
    match loc.text(key) {
        Some(text) => text.to_owned(),
        None => {
            diag::warn!(Ui, "menu: missing localize key `{key}`");
            String::new()
        }
    }
}

fn widget_is_visible(widget: &Widget, frontend: &MenuFrontend) -> bool {
    match &widget.enabled {
        Enabled::Always => true,
        Enabled::Bound { dvar, value } if dvar == "gameMode" => match frontend.game_mode.as_deref()
        {
            Some(mode) => mode == value.as_str(),
            None => true,
        },
        Enabled::Bound { .. } => true,
    }
}

pub(crate) fn place_rect(
    rect: &Rect640,
    win_w: f32,
    win_h: f32,
    canvas: crate::model::Canvas,
) -> (f32, f32, f32, f32) {
    if rect.horz_align == 4
        && rect.vert_align == 4
        && rect.x == 0.0
        && rect.y == 0.0
        && rect.w == 640.0
        && rect.h == 480.0
    {
        return (0.0, 0.0, win_w, win_h);
    }
    let scale = canvas.scale(win_w, win_h);
    let ox = (win_w - 640.0 * scale) * 0.5;
    let oy = (win_h - 480.0 * scale) * 0.5;
    let mut x = rect.x;
    let mut y = rect.y;
    match rect.horz_align {
        2 => x += 320.0,
        3 => x += 640.0,
        _ => {}
    }
    match rect.vert_align {
        2 => y += 240.0,
        3 => y += 480.0,
        _ => {}
    }
    (
        ox + x * scale,
        oy + y * scale,
        rect.w * scale,
        rect.h * scale,
    )
}

pub(crate) fn warm_screen_images(
    catalog: &MenuCatalog,
    screen: &Screen,
    games: Option<&std::path::Path>,
    installed_games: Option<&std::path::Path>,
    images: &mut Assets<Image>,
    cache: &mut MenuImageCache,
) {
    let source = |stem: &str| {
        if stem.contains(":material/") {
            installed_games.or(games)
        } else {
            games
        }
    };
    if let Some(bg) = &screen.background {
        cache.ensure(catalog, source(bg), images, bg);
    }
    for widget in &screen.widgets {
        if !widget.style.background.is_empty() {
            cache.ensure(
                catalog,
                source(&widget.style.background),
                images,
                &widget.style.background,
            );
        }
        if !widget.icon.is_empty() {
            cache.ensure(catalog, source(&widget.icon), images, &widget.icon);
        }
    }
    for font in catalog.fonts.values() {
        cache.ensure(
            catalog,
            games,
            images,
            assets::AssetRef::bare_name(&font.material),
        );
    }
}

impl MenuImageCache {
    pub(crate) fn ensure(
        &mut self,
        catalog: &MenuCatalog,
        games: Option<&std::path::Path>,
        images: &mut Assets<Image>,
        stem: &str,
    ) {
        if stem.is_empty()
            || self.handles.contains_key(stem)
            || self.missing.iter().any(|m| m == stem)
        {
            return;
        }
        if let Some(image) = catalog.zone_image(stem) {
            self.handles.insert(
                stem.to_owned(),
                images.add(rgba_ui_image(
                    image.width,
                    image.height,
                    image.rgba.as_ref().clone(),
                )),
            );
            self.sizes
                .insert(stem.to_owned(), (image.width, image.height));
            return;
        }
        let Some(games) = games else {
            return;
        };

        let image_name = catalog.material_image(stem).unwrap_or(stem);
        match assets::decode_ui_image(games, image_name) {
            Ok(Some((width, height, pixels))) => {
                self.handles.insert(
                    stem.to_owned(),
                    images.add(rgba_ui_image(width, height, pixels)),
                );
                self.sizes.insert(stem.to_owned(), (width, height));
            }
            Ok(None) => {
                diag::warn!(Ui, "menu: IWD miss for material `{stem}` / `{image_name}`");
                self.missing.push(stem.to_owned());
            }
            Err(error) => {
                diag::warn!(Ui, "menu: decode `{stem}` / `{image_name}`: {error}");
                self.missing.push(stem.to_owned());
            }
        }
    }
}
