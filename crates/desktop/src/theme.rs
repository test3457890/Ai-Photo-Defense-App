//! Original visual identity: neutral graphite and paper themes, one restrained accent, thin
//! 1 px separators, 2 px corner radius, compact controls. No gradients, glows or shadows.
use iced::widget::{button, container, text_input};
use iced::{Background, Border, Color, Theme};

pub const FONT_SMALL: u16 = 11;
pub const FONT: u16 = 12;
pub const FONT_TITLE: u16 = 13;

pub fn rgb(hex: u32) -> Color {
    Color::from_rgb8((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Appearance {
    #[default]
    Graphite,
    Paper,
}
impl std::fmt::Display for Appearance {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Graphite => "Graphite (dark)",
            Self::Paper => "Paper (light)",
        })
    }
}

pub fn theme(a: Appearance) -> Theme {
    match a {
        Appearance::Graphite => Theme::custom(
            "Graphite".into(),
            iced::theme::Palette {
                background: rgb(0x232427),
                text: rgb(0xd4d6d9),
                primary: rgb(0x4f8dcc),
                success: rgb(0x5aa36f),
                danger: rgb(0xd0614f),
            },
        ),
        Appearance::Paper => Theme::custom(
            "Paper".into(),
            iced::theme::Palette {
                background: rgb(0xe9eaec),
                text: rgb(0x1e2023),
                primary: rgb(0x2f6aa8),
                success: rgb(0x2f7d4a),
                danger: rgb(0xb0412f),
            },
        ),
    }
}

fn dark(t: &Theme) -> bool {
    t.extended_palette().is_dark
}
/// Surface tones relative to the window background.
pub fn surface(t: &Theme, level: u8) -> Color {
    let b = t.palette().background;
    let k = if dark(t) { 0.035 } else { -0.03 } * f32::from(level);
    Color::from_rgb(
        (b.r + k).clamp(0., 1.),
        (b.g + k).clamp(0., 1.),
        (b.b + k).clamp(0., 1.),
    )
}
pub fn line(t: &Theme) -> Color {
    if dark(t) {
        rgb(0x37393d)
    } else {
        rgb(0xc5c8cc)
    }
}
pub fn muted(t: &Theme) -> Color {
    if dark(t) {
        rgb(0x8d9095)
    } else {
        rgb(0x5d6166)
    }
}
fn border(color: Color) -> Border {
    Border {
        color,
        width: 1.,
        radius: 2.0.into(),
    }
}

pub fn panel(t: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(surface(t, 1))),
        border: Border {
            color: line(t),
            width: 0.,
            radius: 0.0.into(),
        },
        ..Default::default()
    }
}
pub fn bar(t: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(surface(t, 2))),
        ..Default::default()
    }
}
/// Photograph workspace: neutral mid-grey so colour judgement is not biased.
pub fn viewer(t: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(if dark(t) {
            rgb(0x1a1b1d)
        } else {
            rgb(0x8a8c8f)
        })),
        ..Default::default()
    }
}
pub fn menu_popup(t: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(surface(t, 3))),
        border: border(line(t)),
        ..Default::default()
    }
}
pub fn section_header(t: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(surface(t, 2))),
        border: Border {
            color: line(t),
            width: 0.,
            radius: 0.0.into(),
        },
        ..Default::default()
    }
}
pub fn banner(t: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(t.palette().primary.scale_alpha(0.18))),
        border: border(t.palette().primary.scale_alpha(0.6)),
        ..Default::default()
    }
}
pub fn warning_box(t: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(t.palette().danger.scale_alpha(0.14))),
        border: border(t.palette().danger.scale_alpha(0.6)),
        ..Default::default()
    }
}

/// Compact rectangular command button.
pub fn tool(t: &Theme, s: button::Status) -> button::Style {
    let base = surface(t, 2);
    let bg = match s {
        button::Status::Hovered => surface(t, 3),
        button::Status::Pressed => surface(t, 4),
        _ => base,
    };
    button::Style {
        background: Some(Background::Color(bg)),
        text_color: if s == button::Status::Disabled {
            muted(t).scale_alpha(0.6)
        } else {
            t.palette().text
        },
        border: border(line(t)),
        ..Default::default()
    }
}
/// Toggle/selected state uses the accent, never a different shape.
pub fn tool_active(t: &Theme, s: button::Status) -> button::Style {
    let p = t.palette().primary;
    button::Style {
        background: Some(Background::Color(if s == button::Status::Hovered {
            p.scale_alpha(0.85)
        } else {
            p
        })),
        text_color: Color::WHITE,
        border: border(p),
        ..Default::default()
    }
}
pub fn primary(t: &Theme, s: button::Status) -> button::Style {
    if s == button::Status::Disabled {
        tool(t, s)
    } else {
        tool_active(t, s)
    }
}
pub fn danger(t: &Theme, s: button::Status) -> button::Style {
    let d = t.palette().danger;
    button::Style {
        background: Some(Background::Color(d.scale_alpha(
            if s == button::Status::Hovered {
                1.
            } else {
                0.85
            },
        ))),
        text_color: Color::WHITE,
        border: border(d),
        ..Default::default()
    }
}
/// Flat item used in menus, lists and tabs.
pub fn flat(t: &Theme, s: button::Status) -> button::Style {
    button::Style {
        background: match s {
            button::Status::Hovered | button::Status::Pressed => {
                Some(Background::Color(t.palette().primary.scale_alpha(0.25)))
            }
            _ => None,
        },
        text_color: if s == button::Status::Disabled {
            muted(t).scale_alpha(0.6)
        } else {
            t.palette().text
        },
        border: Border {
            radius: 0.0.into(),
            ..Default::default()
        },
        ..Default::default()
    }
}
pub fn flat_selected(t: &Theme, _s: button::Status) -> button::Style {
    button::Style {
        background: Some(Background::Color(t.palette().primary.scale_alpha(0.35))),
        text_color: t.palette().text,
        ..Default::default()
    }
}
/// Text-input style without frame or padding; the frame comes from `field_frame`.
pub fn input_bare(t: &Theme, s: text_input::Status) -> text_input::Style {
    let mut d = input(t, s);
    d.border = iced::Border::default();
    d.background = Background::Color(iced::Color::TRANSPARENT);
    d
}
/// Frame for text fields (see `views::field`).
pub fn field_frame(t: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(surface(t, 0))),
        border: border(line(t)),
        ..Default::default()
    }
}
pub fn input(t: &Theme, s: text_input::Status) -> text_input::Style {
    let mut d = text_input::default(t, s);
    d.border = border(if matches!(s, text_input::Status::Focused) {
        t.palette().primary
    } else {
        line(t)
    });
    d.background = Background::Color(surface(t, 0));
    d
}
