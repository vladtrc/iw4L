use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, SyncSender, TryRecvError};

use bevy::asset::RenderAssetUsages;
use bevy::image::ImageSampler;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

use crate::{ConsoleCommand, ConsoleLine, ConsoleRegistry, ConsoleSettings, ConsoleState};

const QUEUE: usize = 8;

enum Msg {
    Frame(f64, Vec<u8>),
    Failed(String),
}

#[derive(Resource)]
pub(crate) struct Cinematic {
    frames: std::sync::Mutex<Receiver<Msg>>,
    recycle: SyncSender<Vec<u8>>,
    stop: Arc<AtomicBool>,
    image: Handle<Image>,
    root: Entity,
    clock: Option<audio::PcmClock>,
    audio_sync: Option<(u64, std::time::Instant)>,
    last_now: f64,
    started: Option<std::time::Instant>,
    pending: Option<(f64, Vec<u8>)>,
    console_was_open: bool,
    kind: Kind,
}

enum Kind {
    Console,
    Script,
    Load,
}

fn remove(c: &Cinematic, commands: &mut Commands) {
    commands.entity(c.root).despawn();
    commands.remove_resource::<Cinematic>();
    match c.kind {
        Kind::Load => commands.remove_resource::<frame::LoadCinematicPlaying>(),
        Kind::Console | Kind::Script => {}
    }
}

impl Drop for Cinematic {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

pub(crate) fn register(registry: &mut ConsoleRegistry) {
    let names: Vec<String> = video_dirs()
        .iter()
        .filter_map(|dir| std::fs::read_dir(dir).ok())
        .flatten()
        .filter_map(|e| {
            let path = e.ok()?.path();
            path.extension()
                .is_some_and(|x| x.eq_ignore_ascii_case("bik"))
                .then(|| path.file_stem()?.to_str().map(str::to_owned))?
        })
        .chain(std::iter::once("stop".to_owned()))
        .collect();
    registry.register(
        crate::CommandSpec::new("cinematic")
            .usage("cinematic <name>|stop — play video/<name>.bik fullscreen")
            .arg(crate::StaticCompleter::new(names)),
    );
}

fn video_dirs() -> Vec<PathBuf> {
    asset_transport::games_root_from_env()
        .map(|root| asset_transport::video_dirs(&root.0))
        .unwrap_or_default()
}

fn find_video(name: &str) -> Option<PathBuf> {
    let file = format!("{}.bik", name.trim_end_matches(".bik"));
    video_dirs()
        .into_iter()
        .map(|d| d.join(&file))
        .find(|p| p.is_file())
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn route(
    mut events: MessageReader<ConsoleCommand>,
    mut console: ResMut<ConsoleState>,
    settings: Res<ConsoleSettings>,
    mut line: ResMut<ConsoleLine>,
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    audio: Option<Res<audio::AudioRuntime>>,
    playing: Option<Res<Cinematic>>,
) {
    let capacity = settings.log_capacity;
    for cmd in events.read() {
        if cmd.name != "cinematic" {
            continue;
        }
        let msg = match cmd.args.as_slice() {
            [arg] if arg == "stop" => {
                if let Some(c) = &playing {
                    remove(c, &mut commands);
                }
                "cinematic stopped".to_owned()
            }
            [name] => match find_video(name) {
                None => format!("cinematic: video/{name}.bik not found"),
                Some(path) => {
                    if let Some(c) = &playing {
                        remove(c, &mut commands);
                    }
                    match start(
                        &path,
                        &mut commands,
                        &mut images,
                        audio.as_deref(),
                        Kind::Console,
                    ) {
                        Ok(info) => info,
                        Err(e) => format!("cinematic: {e}"),
                    }
                }
            },
            _ => "usage: cinematic <name>|stop".to_owned(),
        };
        diag::info!(Console, "{msg}");
        line.0 = msg.clone();
        console.echo(msg, capacity);
    }
}

fn start(
    path: &Path,
    commands: &mut Commands,
    images: &mut Assets<Image>,
    audio: Option<&audio::AudioRuntime>,
    kind: Kind,
) -> Result<String, bink::Error> {
    start_looped(path, commands, images, audio, kind, false, true)
}

fn start_looped(
    path: &Path,
    commands: &mut Commands,
    images: &mut Assets<Image>,
    audio: Option<&audio::AudioRuntime>,
    kind: Kind,
    looped: bool,
    fullscreen: bool,
) -> Result<String, bink::Error> {
    let bink = bink::Bink::open(path)?;
    let h = *bink.header();
    let (w, hh) = (h.width, h.height);
    let size = (w * hh * 4) as usize;

    let mut image = Image::new(
        Extent3d {
            width: w,
            height: hh,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        vec![0; size],
        TextureFormat::Rgba8Unorm,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    image.sampler = ImageSampler::linear();
    let handle = images.add(image);
    let root = commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                display: if fullscreen {
                    Display::Flex
                } else {
                    Display::None
                },
                ..default()
            },
            BackgroundColor(Color::BLACK),
            GlobalZIndex(15_000),
        ))
        .with_child((
            Node {
                height: Val::Percent(100.0),
                max_width: Val::Percent(100.0),
                aspect_ratio: Some(w as f32 / hh as f32),
                ..default()
            },
            ImageNode::new(handle.clone()),
        ))
        .id();

    let (frames_tx, frames) = std::sync::mpsc::sync_channel(QUEUE);
    let (recycle, recycle_rx) = std::sync::mpsc::sync_channel(QUEUE + 2);
    let stop = Arc::new(AtomicBool::new(false));
    let stream = h.audio.and(audio).map(audio::AudioRuntime::open_stream);
    let clock = stream.as_ref().map(audio::PcmStream::clock);
    let thread_stop = stop.clone();
    let again = looped.then(|| path.to_owned());
    std::thread::Builder::new()
        .name("iw4l-bink".into())
        .spawn(move || {
            decode(
                bink,
                again,
                size,
                stream,
                frames_tx,
                recycle_rx,
                &thread_stop,
            );
        })
        .map_err(bink::Error::Io)?;

    commands.insert_resource(Cinematic {
        frames: frames.into(),
        recycle,
        stop,
        image: handle,
        root,
        clock,
        audio_sync: None,
        last_now: 0.0,
        started: None,
        pending: None,
        console_was_open: true,
        kind,
    });
    let fps = f64::from(h.fps_num) / f64::from(h.fps_den);
    let sound = match h.audio {
        Some(a) => format!("{} Hz {}ch", a.rate, a.channels),
        None => "no audio".to_owned(),
    };
    Ok(format!(
        "cinematic: {} {w}x{hh} {fps:.2} fps, {} frames, {sound}",
        path.display(),
        h.frames
    ))
}

fn decode(
    mut bink: bink::Bink,
    again: Option<PathBuf>,
    size: usize,
    stream: Option<audio::PcmStream>,
    frames: SyncSender<Msg>,
    recycle: Receiver<Vec<u8>>,
    stop: &AtomicBool,
) {
    let h = *bink.header();
    let frame_time = f64::from(h.fps_den) / f64::from(h.fps_num);
    let mut resample = h.audio.map(|a| Resampler::new(a.rate, a.channels as usize));
    let (mut pcm, mut stereo) = (Vec::new(), Vec::new());
    for index in 0.. {
        if stop.load(Ordering::Relaxed) {
            return;
        }
        pcm.clear();
        let mut next = bink.next_frame(&mut pcm);
        if let (Ok(false), Some(path)) = (&next, &again) {
            next = bink::Bink::open(path).and_then(|reopened| {
                bink = reopened;
                bink.next_frame(&mut pcm)
            });
        }
        match next {
            Ok(true) => {}
            Ok(false) => return,
            Err(e) => {
                let _ = frames.send(Msg::Failed(e.to_string()));
                return;
            }
        }
        if let (Some(stream), Some(r)) = (&stream, &mut resample) {
            stereo.clear();
            r.run(&pcm, &mut stereo);
            let mut rest = stereo.as_slice();
            while !rest.is_empty() && !stop.load(Ordering::Relaxed) {
                rest = &rest[stream.push(rest) * 2..];
                if !rest.is_empty() {
                    std::thread::sleep(std::time::Duration::from_millis(5));
                }
            }
        }
        let mut rgba = recycle.try_recv().unwrap_or_else(|_| vec![0; size]);
        bink.rgba(&mut rgba);
        if frames
            .send(Msg::Frame(f64::from(index) * frame_time, rgba))
            .is_err()
        {
            return;
        }
    }
}

struct Resampler {
    step: f64,
    pos: f64,
    channels: usize,
    prev: [f32; 2],
}

impl Resampler {
    fn new(rate: u32, channels: usize) -> Self {
        Self {
            step: f64::from(rate) / f64::from(audio::SAMPLE_RATE),
            pos: 0.0,
            channels,
            prev: [0.0; 2],
        }
    }

    fn run(&mut self, input: &[f32], out: &mut Vec<f32>) {
        let n = input.len() / self.channels;
        if n == 0 {
            return;
        }
        let frame = |i: isize| -> [f32; 2] {
            if i < 0 {
                self.prev
            } else {
                let at = i as usize * self.channels;
                [input[at], input[at + self.channels - 1]]
            }
        };
        while self.pos < (n - 1) as f64 {
            let i = self.pos.floor();
            let t = (self.pos - i) as f32;
            let (a, b) = (frame(i as isize), frame(i as isize + 1));
            out.extend([a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]);
            self.pos += self.step;
        }
        self.prev = frame(n as isize - 1);
        self.pos -= n as f64;
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn tick(
    mut commands: Commands,
    mut c: ResMut<Cinematic>,
    mut images: ResMut<Assets<Image>>,
    mut console: ResMut<ConsoleState>,
    settings: Res<ConsoleSettings>,
    keys: Res<ButtonInput<KeyCode>>,
    pads: Query<&Gamepad>,
    readiness: Option<Res<session::SessionReadinessPolicy>>,
) {
    let skippable = match c.kind {
        Kind::Console => true,
        Kind::Script => false,
        Kind::Load => readiness.is_some_and(|r| r.presentation_allowed),
    };
    let skip = !c.console_was_open
        && !console.open
        && skippable
        && (keys.any_just_pressed([KeyCode::Escape, KeyCode::Enter, KeyCode::Space])
            || pads
                .iter()
                .any(|p| p.any_just_pressed([GamepadButton::South, GamepadButton::Start])));
    c.console_was_open = console.open;

    let raw_now = if let Some(clock) = c.clock.clone() {
        let frames = clock.played_frames();
        let instant_now = std::time::Instant::now();
        if frames > 0 {
            let (last_frames, last_time) = c.audio_sync.get_or_insert((frames, instant_now));
            let extra = if *last_frames != frames {
                *last_frames = frames;
                *last_time = instant_now;
                0.0
            } else {
                instant_now.duration_since(*last_time).as_secs_f64()
            };
            clock.played_seconds() + extra
        } else if c
            .started
            .is_some_and(|t| t.elapsed() > std::time::Duration::from_millis(500))
        {
            c.started.unwrap().elapsed().as_secs_f64()
        } else {
            0.0
        }
    } else {
        c.started.map_or(0.0, |t| t.elapsed().as_secs_f64())
    };
    c.last_now = c.last_now.max(raw_now);
    let now = c.last_now;
    let mut newest = None;
    let mut ended = skip;
    loop {
        let next = match c.pending.take() {
            Some(f) => f,
            None => match c
                .frames
                .get_mut()
                .unwrap_or_else(|e| e.into_inner())
                .try_recv()
            {
                Ok(Msg::Frame(pts, rgba)) => (pts, rgba),
                Ok(Msg::Failed(e)) => {
                    console.echo(format!("cinematic: {e}"), settings.log_capacity);
                    ended = true;
                    break;
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    ended = match c.kind {
                        Kind::Load => skippable,
                        _ => newest.is_none(),
                    };
                    break;
                }
            },
        };
        if c.started.is_some() && next.0 > now {
            c.pending = Some(next);
            break;
        }
        c.started.get_or_insert_with(std::time::Instant::now);
        if let Some((_, old)) = newest.replace(next) {
            let _ = c.recycle.try_send(old);
        }
    }
    if let Some((_, rgba)) = newest
        && let Some(mut image) = images.get_mut(&c.image)
        && let Some(old) = image.data.replace(rgba)
    {
        let _ = c.recycle.try_send(old);
    }
    if ended {
        remove(&c, &mut commands);
    }
}

pub(crate) fn sync_script_cinematic(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    audio: Option<Res<audio::AudioRuntime>>,
    authority: Option<Res<net::AuthorityWorld>>,
    playing: Option<Res<Cinematic>>,
    mut shown: Local<Option<sim::ScriptCinematic>>,
) {
    let current = match &authority {
        Some(authority) => authority.0.cinematic(),
        None => None,
    };
    if current == *shown {
        return;
    }
    if let Some(c) = &playing
        && (current.is_some() || matches!(c.kind, Kind::Script))
    {
        remove(c, &mut commands);
    }
    if let Some(next) = &current {
        match find_video(&next.name) {
            None => diag::warn!(Console, "cinematic: video/{}.bik not found", next.name),
            Some(path) => match start_looped(
                &path,
                &mut commands,
                &mut images,
                audio.as_deref(),
                Kind::Script,
                next.looped,
                next.fullscreen,
            ) {
                Ok(info) => diag::info!(Console, "script {info}"),
                Err(e) => diag::warn!(Console, "cinematic: {e}"),
            },
        }
    }
    *shown = current;
}

pub(crate) fn start_load_cinematic(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    audio: Option<Res<audio::AudioRuntime>>,
    load: Option<Res<assets::MapLoadProcess>>,
    signon: Option<Res<net::SignonState>>,
    playing: Option<Res<Cinematic>>,
    mut seen: Local<Option<u64>>,
) {
    if let Some(c) = &playing
        && matches!(c.kind, Kind::Load)
        && signon.is_some_and(|s| s.phase.is_failed())
    {
        remove(c, &mut commands);
    }
    let Some(load) = load else {
        return;
    };
    if *seen == Some(load.request_id) {
        return;
    }
    *seen = Some(load.request_id);
    let Some(path) = find_video(&format!("{}_load", load.zone)) else {
        return;
    };
    if let Some(c) = &playing {
        remove(c, &mut commands);
    }
    match start(
        &path,
        &mut commands,
        &mut images,
        audio.as_deref(),
        Kind::Load,
    ) {
        Ok(info) => {
            diag::info!(Console, "briefing {info}");
            commands.insert_resource(frame::LoadCinematicPlaying);
        }
        Err(e) => diag::warn!(Console, "briefing: {}: {e}", path.display()),
    }
}
