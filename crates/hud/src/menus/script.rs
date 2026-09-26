#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Command {
    Open(String),
    Close(String),
    Escape(String),
    SetFocus(String),
    FocusFirst,
    SetItemColor {
        item: String,
        field: String,
        color: [f32; 4],
    },
    Show(String),
    Hide(String),
    Play(String),
    ScriptMenuResponse(String),
    Exec(String),
    SetDvar(String, String),
    ExecOnDvarIntValue {
        dvar: String,
        value: i32,
        command: String,
    },
    ExecOnDvarStringValue {
        dvar: String,
        value: String,
        command: String,
    },
    Unhandled(String),
}

pub(crate) fn tokens(script: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut chars = script.chars().peekable();
    while let Some(&c) = chars.peek() {
        if c.is_whitespace() || c == ';' {
            chars.next();
            continue;
        }
        let mut token = String::new();
        if c == '"' {
            chars.next();
            for c in chars.by_ref() {
                if c == '"' {
                    break;
                }
                token.push(c);
            }
        } else {
            while let Some(&c) = chars.peek() {
                if c.is_whitespace() || c == ';' || c == '"' {
                    break;
                }
                token.push(c);
                chars.next();
            }
        }
        out.push(token);
    }
    out
}

pub(crate) fn parse(script: &str) -> Vec<Command> {
    let toks = tokens(script);
    let mut out = Vec::new();
    let mut i = 0;
    let arg = |i: usize| toks.get(i).cloned().unwrap_or_default();
    let float = |i: usize| {
        toks.get(i)
            .and_then(|t| t.parse::<f32>().ok())
            .unwrap_or(0.0)
    };
    while i < toks.len() {
        let name = toks[i].to_ascii_lowercase();
        let (command, used) = match name.as_str() {
            "open" | "ingameopen" => (Command::Open(arg(i + 1)), 2),
            "close" | "ingameclose" => (Command::Close(arg(i + 1)), 2),
            "escape" => (Command::Escape(arg(i + 1)), 2),
            "setfocus" => (Command::SetFocus(arg(i + 1)), 2),
            "focusfirst" => (Command::FocusFirst, 1),
            "setitemcolor" => (
                Command::SetItemColor {
                    item: arg(i + 1),
                    field: arg(i + 2).to_ascii_lowercase(),
                    color: [float(i + 3), float(i + 4), float(i + 5), float(i + 6)],
                },
                7,
            ),
            "show" | "showmenu" => (Command::Show(arg(i + 1)), 2),
            "hide" | "hidemenu" => (Command::Hide(arg(i + 1)), 2),
            "play" => (Command::Play(arg(i + 1)), 2),
            "scriptmenuresponse" => (Command::ScriptMenuResponse(arg(i + 1)), 2),
            "exec" | "execnow" => (Command::Exec(arg(i + 1)), 2),
            "setdvar" => (Command::SetDvar(arg(i + 1), arg(i + 2)), 3),
            "execondvarintvalue" => (
                Command::ExecOnDvarIntValue {
                    dvar: arg(i + 1),
                    value: arg(i + 2).parse().unwrap_or(0),
                    command: arg(i + 3),
                },
                4,
            ),
            "execondvarstringvalue" => (
                Command::ExecOnDvarStringValue {
                    dvar: arg(i + 1),
                    value: arg(i + 2),
                    command: arg(i + 3),
                },
                4,
            ),
            "uiscript" | "fadein" | "fadeout" | "setbackground" | "playlooped"
            | "setfocusbydvar" => (Command::Unhandled(format!("{name} {}", arg(i + 1))), 2),
            "setcolor" => (Command::Unhandled(name.clone()), 6),
            "openforgametype" | "closeforgametype" => (Command::Unhandled(name.clone()), 3),
            _ => (Command::Unhandled(name.clone()), 1),
        };
        out.push(command);
        i += used;
    }
    out
}
