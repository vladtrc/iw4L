use asset_core::AssetNamespace;
use bevy::prelude::*;

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct NativeUiPaint;

#[derive(Component)]
pub(crate) struct NativeUiRoot(pub bool);

#[derive(Component)]
struct Glyph;

#[derive(Component)]
struct BitmapText {
    size: f32,
    wrap: f32,
    value: String,
    color: Color,
    atlas: Handle<Image>,
    children: Vec<Entity>,
}

pub(crate) fn register(app: &mut App) {
    app.add_systems(
        Update,
        refresh.in_set(frame::ClientSet::Ui).after(NativeUiPaint),
    );
}

fn refresh(
    mut commands: Commands,
    mut texts: Query<
        (
            Entity,
            &Text,
            &mut TextFont,
            &mut TextColor,
            &mut Node,
            Option<&mut BitmapText>,
        ),
        Without<Glyph>,
    >,
    parents: Query<&ChildOf>,
    computed: Query<&ComputedNode>,
    roots: Query<&NativeUiRoot>,
    mut art: ResMut<super::t6_art::T6Art>,
    mut images: ResMut<Assets<Image>>,
) {
    let Some(font) = asset_material::ui_font::ui_font(AssetNamespace::T6, "fonts/distFont") else {
        return;
    };
    let Some(atlas) = art.image(&font.material, &mut images) else {
        return;
    };
    let Some(image) = images.get(&atlas) else {
        return;
    };
    let dimensions = image.size().as_vec2();
    for (entity, text, mut text_font, mut color, mut node, previous) in &mut texts {
        let mut ancestor = entity;
        let mut native = false;
        while let Ok(parent) = parents.get(ancestor) {
            ancestor = parent.parent();
            if let Ok(root) = roots.get(ancestor) {
                native = root.0;
                break;
            }
        }
        if !native
            || text.0.chars().any(|letter| {
                letter != '\n'
                    && (letter as u32 > u16::MAX as u32
                        || !font.glyphs.contains_key(&(letter as u16)))
            })
        {
            if let Some(previous) = previous {
                text_font.font_size = bevy::text::FontSize::Px(previous.size);
                color.0 = previous.color;
                for child in &previous.children {
                    commands.entity(*child).despawn();
                }
                commands.entity(entity).remove::<BitmapText>();
                node.min_width = Val::Auto;
                node.min_height = Val::Auto;
            }
            continue;
        }
        let size = previous.as_ref().map_or_else(
            || match text_font.font_size {
                bevy::text::FontSize::Px(size) => size,
                _ => 18.0,
            },
            |previous| previous.size,
        );
        let source_color = previous
            .as_ref()
            .filter(|_| color.0.alpha() == 0.0)
            .map_or(color.0, |previous| previous.color);
        color.0 = Color::NONE;
        let wrap = if node.position_type == PositionType::Absolute {
            f32::INFINITY
        } else {
            parents
                .get(entity)
                .ok()
                .and_then(|parent| computed.get(parent.parent()).ok())
                .map(|parent| {
                    (parent.content_box().width() * parent.inverse_scale_factor()).floor()
                })
                .filter(|width| *width > 1.0)
                .unwrap_or(f32::INFINITY)
        };
        if previous.as_ref().is_some_and(|previous| {
            previous.value == text.0
                && previous.color == source_color
                && previous.atlas == atlas
                && previous.wrap == wrap
        }) {
            continue;
        }
        if let Some(previous) = previous {
            for child in &previous.children {
                commands.entity(*child).despawn();
            }
        }
        let render_size = size.max(22.0);
        let scale = render_size / font.pixel_height.max(1) as f32;
        let mut position = Vec2::ZERO;
        let mut width = 0.0_f32;
        let mut children = Vec::new();
        let mut letters = text.0.chars().peekable();
        let mut word_start = true;
        while let Some(letter) = letters.next() {
            if letter == '\n' {
                width = width.max(position.x);
                position.x = 0.0;
                position.y += render_size * 1.2;
                word_start = true;
                continue;
            }
            let glyph = &font.glyphs[&(letter as u16)];
            if word_start && !letter.is_whitespace() {
                let word_width = (glyph.dx as f32
                    + letters
                        .clone()
                        .take_while(|c| !c.is_whitespace())
                        .map(|c| font.glyphs[&(c as u16)].dx as f32)
                        .sum::<f32>())
                    * scale;
                if position.x > 0.0 && position.x + word_width > wrap {
                    width = width.max(position.x);
                    position.x = 0.0;
                    position.y += render_size * 1.2;
                }
            }
            word_start = letter.is_whitespace();
            if word_start && position.x == 0.0 {
                continue;
            }
            if glyph.pixel_width != 0 && glyph.pixel_height != 0 {
                let rect = Rect::from_corners(
                    Vec2::new(glyph.s0, glyph.t0) * dimensions,
                    Vec2::new(glyph.s1, glyph.t1) * dimensions,
                );
                let at = position
                    + Vec2::new(glyph.x0 as f32, font.pixel_height as f32 + glyph.y0 as f32)
                        * scale;
                for (offset, tint) in [
                    (
                        Vec2::splat(1.0),
                        Color::srgba(0.0, 0.0, 0.0, source_color.alpha() * 0.8),
                    ),
                    (Vec2::ZERO, source_color),
                ] {
                    let child = commands
                        .spawn((
                            Glyph,
                            ImageNode {
                                image: atlas.clone(),
                                rect: Some(rect),
                                color: tint,
                                ..default()
                            },
                            Node {
                                position_type: PositionType::Absolute,
                                left: Val::Px(at.x + offset.x),
                                top: Val::Px(at.y + offset.y),
                                width: Val::Px(glyph.pixel_width as f32 * scale),
                                height: Val::Px(glyph.pixel_height as f32 * scale),
                                ..default()
                            },
                        ))
                        .id();
                    commands.entity(entity).add_child(child);
                    children.push(child);
                }
            }
            position.x += glyph.dx as f32 * scale;
        }
        width = width.max(position.x);
        text_font.font_size = bevy::text::FontSize::Px(0.1);
        node.min_width = Val::Px(width);
        node.min_height = Val::Px(position.y + render_size * 1.2);
        commands.entity(entity).insert(BitmapText {
            size,
            wrap,
            value: text.0.clone(),
            color: source_color,
            atlas: atlas.clone(),
            children,
        });
    }
}
