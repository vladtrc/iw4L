use crate::UiLayer;
use assets::MenuRect;
use bevy::prelude::Message;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Rect640 {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub horz_align: u8,
    pub vert_align: u8,
}

impl From<MenuRect> for Rect640 {
    fn from(r: MenuRect) -> Self {
        Self {
            x: r.x,
            y: r.y,
            w: r.w,
            h: r.h,
            horz_align: r.horz_align,
            vert_align: r.vert_align,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)]
pub enum Modality {
    Opaque,
    Overlay,
    Passive,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Canvas {
    #[default]
    Standard,
    Wide,
}

impl Canvas {
    pub fn scale(self, width: f32, height: f32) -> f32 {
        (width
            / match self {
                Self::Standard => 640.0,
                Self::Wide => 854.0,
            })
        .min(height / 480.0)
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Style {
    pub canvas: Canvas,
    pub fore_color: [f32; 4],
    pub text_scale: f32,
    pub font_enum: i32,
    pub text_align_mode: i32,
    pub text_align_x: f32,
    pub text_align_y: f32,

    pub image_contain: bool,
    pub text_wrap: bool,

    pub background: String,

    pub text_key: String,
    pub animation: WidgetAnimation,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum WidgetAnimation {
    #[default]
    None,
    ScrollX {
        period_seconds: f32,
        distance_640: f32,
    },
    PulseAlpha {
        radians_per_second: f32,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Enabled {
    Always,

    Bound { dvar: String, value: String },
}

#[derive(Clone, Debug, PartialEq)]
pub enum Content {
    Panel,
    Image,
    Label,
    Button,

    Cycler {
        label: String,
        value: String,
        previous: UiIntent,
        next: UiIntent,
    },
    Slider {
        label: String,
        value: f32,
        min: f32,
        max: f32,
        step: f32,
        key: SettingKey,
    },
    Bind {
        label: String,
        command_id: u32,
        chord: String,
        listening: bool,
    },
    TextEdit {
        label: String,
        buffer: String,
        cursor: usize,
        editing: bool,
    },
    Unsupported {
        reason: String,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub enum ScreenCmd {
    Open(String),
    Back,
    CloseAll,
    PlaySound(String),

    Emit(UiIntent),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettingKey {
    Resolution,
    Fullscreen,
    Vsync,
    MasterVolume,
    Sensitivity,
    InvertMouse,
    PlayerName,
}

#[derive(Clone, Debug, PartialEq)]
pub enum SettingValue {
    Resolution(frame::DisplayResolution),
    Bool(bool),
    Float(f32),
    Text(String),
}

#[derive(Message, Clone, Debug, PartialEq)]
#[allow(dead_code)]
pub enum UiIntent {
    LoadMap(String),
    RefreshServers,

    JoinPublicLobby {
        advert_id: net::AdvertId,
        map: String,
        mode: String,
    },
    SelectGamePrivacy(bool),

    SelectGameMap(String),
    SelectGameMapPage(u32),
    SelectGameMode(String),
    CreateLobby {
        map: String,
        public: bool,
    },
    StartLobbyMatch {
        map: String,
        public: bool,
    },
    VoteToSkip,
    LeaveLobby,
    SelectClass(i32),
    SetBinding {
        id: u32,
        chord: String,
    },
    BeginBinding {
        id: u32,
    },
    SetSetting {
        key: SettingKey,
        value: SettingValue,
    },
    SelectOptionsTab(u8),
    BeginPlayerNameEdit,
    CommitPlayerNameEdit(String),
    CancelPlayerNameEdit,
    Quit,

    CacSelectSlot(u32),

    CacEditRow(u8),
    CacPick(String),

    CacPickCategory(u8),

    CacResetClass,
    CacEditAttachments(u8),
    CacPickAttachment(Option<String>),
    CacBeginRename,
    CacCommitRename(String),
    CacCancelRename,
    CacCancelEdit,
    CacPage(i32),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Widget {
    pub id: String,
    pub rect: Rect640,
    pub style: Style,
    pub content: Content,
    pub focusable: bool,
    pub enabled: Enabled,
    pub on_focus: Vec<ScreenCmd>,
    pub on_activate: Vec<ScreenCmd>,

    pub icon: String,

    pub help: Option<String>,

    pub focus_order: Option<u32>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Screen {
    pub id: String,
    pub layer: UiLayer,
    pub modality: Modality,
    pub background: Option<String>,
    pub bed: Option<String>,
    pub widgets: Vec<Widget>,
    pub focus_overrides: Vec<(String, crate::NavDir, String)>,
    pub on_open: Vec<ScreenCmd>,
    pub on_back: Vec<ScreenCmd>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MenuCommand {
    pub verb: String,
    pub args: Vec<String>,
}

pub fn parse_menu_script(script: &str) -> Vec<MenuCommand> {
    let tokens = tokenize(script);
    let mut out = Vec::new();
    let mut cur = Vec::new();
    for token in tokens {
        if token == ";" {
            push_cmd(&mut out, &mut cur);
        } else {
            cur.push(token);
        }
    }
    push_cmd(&mut out, &mut cur);
    out
}

pub fn screen_cmds(script: &str) -> (Vec<ScreenCmd>, Vec<MenuCommand>) {
    let mut cmds = Vec::new();
    let mut unknown = Vec::new();
    for tok in parse_menu_script(script) {
        match tok.verb.as_str() {
            "open" => {
                if let Some(target) = tok.args.first() {
                    cmds.push(ScreenCmd::Open(target.clone()));
                }
            }
            "close" => cmds.push(ScreenCmd::Back),
            "closeall" => cmds.push(ScreenCmd::CloseAll),
            "play" => {
                if let Some(alias) = tok.args.first() {
                    cmds.push(ScreenCmd::PlaySound(alias.clone()));
                }
            }
            _ => unknown.push(tok),
        }
    }
    (cmds, unknown)
}

pub fn screen_cmds_all(scripts: &[String]) -> (Vec<ScreenCmd>, Vec<MenuCommand>) {
    let mut cmds = Vec::new();
    let mut unknown = Vec::new();
    for script in scripts {
        let (one, rest) = screen_cmds(script);
        cmds.extend(one);
        unknown.extend(rest);
    }
    (cmds, unknown)
}

fn push_cmd(out: &mut Vec<MenuCommand>, cur: &mut Vec<String>) {
    if cur.is_empty() {
        return;
    }
    let verb = cur.remove(0);
    out.push(MenuCommand {
        verb,
        args: std::mem::take(cur),
    });
}

fn tokenize(script: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut chars = script.chars().peekable();
    while let Some(c) = chars.next() {
        if c == ';' {
            out.push(";".into());
        } else if c == '"' {
            let mut buf = String::new();
            while let Some(ch) = chars.next() {
                if ch == '"' {
                    break;
                }
                if ch == '\\' {
                    if let Some(n) = chars.next() {
                        buf.push(n);
                    }
                } else {
                    buf.push(ch);
                }
            }
            out.push(buf);
        } else if c.is_whitespace() {
            continue;
        } else {
            let mut buf = String::from(c);
            while let Some(&n) = chars.peek() {
                if n.is_whitespace() || n == ';' || n == '"' {
                    break;
                }
                buf.push(chars.next().expect("peeked"));
            }
            out.push(buf);
        }
    }
    out
}
