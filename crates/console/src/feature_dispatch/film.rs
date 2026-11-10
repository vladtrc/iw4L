//! `render <demo|clip> [fps]`: play a recording back on a fixed clock and
//! encode every frame, with the mix, into a video file.
//!
//! Nothing is captured live. The playback advances by exactly one frame of
//! film time per rendered frame however long that frame took, every frame is
//! read back, and the mix is taken off the device and advanced by the same
//! clock, so the file is the same on any machine. Encoding is `ffmpeg`'s.

use std::collections::BTreeMap;
use std::fs::File;
use std::io::{BufWriter, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, SyncSender, sync_channel};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use bevy::prelude::*;
use bevy::render::view::screenshot::{Screenshot, ScreenshotCaptured};
use bevy::time::TimeUpdateStrategy;
use replay::ReplayPlayback;

use crate::{ConsoleLine, ConsoleSettings, ConsoleState};

pub(crate) const DEFAULT_FPS: u32 = 60;

/// Readbacks waiting for the encoder. A full queue holds the frame loop, so
/// a slow encode slows the render down instead of growing memory.
const QUEUED_FRAMES: usize = 6;

/// Film time kept after the recording ends, so the last sounds ring out.
const TAIL: Duration = Duration::from_millis(500);

#[derive(Resource)]
pub(crate) struct FilmRender {
    name: String,
    output: PathBuf,
    fps: u32,
    video_args: Vec<&'static str>,
    phase: Phase,
    frames: u64,
    tail_frames: u32,
    in_flight: Arc<AtomicUsize>,
    to_encoder: Option<SyncSender<FilmPart>>,
    encoder: Option<JoinHandle<Result<(), String>>>,
    started: Option<Instant>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Phase {
    /// The recording is loading, paused on its first tick.
    Loading,
    Rolling,
    /// No more frames; the readbacks and the mix still owed are collected.
    Draining,
    Encoding,
}

enum FilmPart {
    Frame(u64, Box<Image>),
    Audio(Vec<[f32; 2]>),
}

impl FilmRender {
    /// Check that a video can be made at all before the session is torn down
    /// for it.
    pub(crate) fn prepare(artifacts: &Path, name: &str, fps: u32) -> Result<Self, String> {
        let video_args = video_encoder()?;
        let stem = replay::demo_stem(name);
        let dir = artifacts.join("renders");
        std::fs::create_dir_all(&dir)
            .map_err(|error| format!("create {}: {error}", dir.display()))?;
        Ok(Self {
            name: stem.to_owned(),
            output: dir.join(format!("{stem}.mp4")),
            fps,
            video_args,
            phase: Phase::Loading,
            frames: 0,
            tail_frames: (TAIL.as_secs_f64() * f64::from(fps)).ceil() as u32,
            in_flight: Arc::new(AtomicUsize::new(0)),
            to_encoder: None,
            encoder: None,
            started: None,
        })
    }

    pub(crate) fn output(&self) -> &Path {
        &self.output
    }

    /// Audio frames owed for video frames `0..=frame`, rounded the same way
    /// every time so no sample is lost or doubled between frames.
    fn audio_through(&self, frame: u64) -> u64 {
        (frame + 1) * u64::from(audio::SAMPLE_RATE) / u64::from(self.fps)
    }
}

pub(crate) fn parse_render_args(args: &[String]) -> Result<(&str, u32), String> {
    let usage = || "usage: render <demo|clip> [fps]".to_owned();
    match args {
        [name] => Ok((name, DEFAULT_FPS)),
        [name, fps] => match fps.parse::<u32>() {
            Ok(fps @ 1..=240) => Ok((name, fps)),
            _ => Err(usage()),
        },
        _ => Err(usage()),
    }
}

/// The best H.264 encoder this `ffmpeg` has, or why there is none.
fn video_encoder() -> Result<Vec<&'static str>, String> {
    let listed = Command::new("ffmpeg")
        .args(["-hide_banner", "-encoders"])
        .stderr(Stdio::null())
        .output()
        .map_err(|error| format!("ffmpeg is needed to encode and was not run: {error}"))?;
    let listed = String::from_utf8_lossy(&listed.stdout);
    let has = |encoder: &str| {
        listed
            .lines()
            .any(|line| line.split_whitespace().nth(1) == Some(encoder))
    };
    if has("libx264") {
        Ok(vec!["-c:v", "libx264", "-preset", "medium", "-crf", "16"])
    } else if has("libopenh264") {
        Ok(vec!["-c:v", "libopenh264", "-b:v", "40M"])
    } else if has("mpeg4") {
        Ok(vec!["-c:v", "mpeg4", "-q:v", "2"])
    } else {
        Err("ffmpeg has no H.264 or MPEG-4 encoder".into())
    }
}

pub(crate) fn drive_film_render(
    mut commands: Commands,
    film: Option<ResMut<FilmRender>>,
    mut playback: Option<ResMut<ReplayPlayback>>,
    capture: Option<Res<render::diag::capture::CaptureQueue>>,
    audio: Option<Res<audio::AudioRuntime>>,
    mut clock: Option<ResMut<TimeUpdateStrategy>>,
    mut output: (
        ResMut<ConsoleState>,
        Res<ConsoleSettings>,
        ResMut<ConsoleLine>,
    ),
) {
    let Some(mut film) = film else {
        return;
    };
    let (console, settings, line) = &mut output;
    let capacity = settings.log_capacity;
    let echo = |msg: String, console: &mut ConsoleState, line: &mut ConsoleLine| {
        diag::info!(Console, "{msg}");
        line.0 = msg.clone();
        console.echo(msg, capacity);
    };

    match film.phase {
        Phase::Loading => {
            let Some(playback) = playback.as_mut() else {
                return;
            };
            if playback.ticks() == 0 || !capture.is_some_and(|capture| capture.settled()) {
                return;
            }
            let (to_encoder, frames) = sync_channel(QUEUED_FRAMES);
            let job = EncodeJob {
                output: film.output.clone(),
                fps: film.fps,
                video_args: film.video_args.clone(),
            };
            film.encoder = Some(
                std::thread::Builder::new()
                    .name("film-encode".into())
                    .spawn(move || job.run(frames))
                    .expect("cannot start the film encoder thread"),
            );
            film.to_encoder = Some(to_encoder);
            if let Some(audio) = audio.as_deref() {
                audio.begin_capture();
            }
            playback.retain_at_end = true;
            playback.control.paused = false;
            // The first frame captured is the one already presented; every
            // later one is exactly one frame of film time after it.
            if let Some(clock) = clock.as_mut() {
                **clock = TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(
                    1.0 / f64::from(film.fps),
                ));
            }
            console.open = false;
            film.phase = Phase::Rolling;
            film.started = Some(Instant::now());
            echo(
                format!(
                    "render: `{}` at {} fps → {}",
                    film.name,
                    film.fps,
                    film.output.display()
                ),
                console,
                line,
            );
            capture_frame(&mut commands, &mut film, audio.as_deref());
        }
        Phase::Rolling => {
            let ended = playback.as_ref().is_none_or(|playback| playback.ended);
            if ended {
                if film.tail_frames == 0 {
                    film.phase = Phase::Draining;
                    return;
                }
                film.tail_frames -= 1;
            }
            capture_frame(&mut commands, &mut film, audio.as_deref());
        }
        Phase::Draining => {
            let audio_owed = audio
                .as_deref()
                .is_some_and(audio::AudioRuntime::capture_behind);
            if film.in_flight.load(Ordering::Acquire) > 0 || audio_owed {
                return;
            }
            if let Some(audio) = audio.as_deref() {
                let tail = audio.take_captured();
                audio.end_capture();
                send(&mut film, FilmPart::Audio(tail));
            }
            if let Some(clock) = clock.as_mut() {
                **clock = TimeUpdateStrategy::Automatic;
            }
            // Closing the channel is the encoder's end of input.
            film.to_encoder = None;
            film.phase = Phase::Encoding;
            echo(
                format!(
                    "render: {} frames rendered in {:.1}s, encoding",
                    film.frames,
                    film.started.map_or(0.0, |at| at.elapsed().as_secs_f64())
                ),
                console,
                line,
            );
        }
        Phase::Encoding => {
            if !film.encoder.as_ref().is_some_and(JoinHandle::is_finished) {
                return;
            }
            let result = film
                .encoder
                .take()
                .expect("encoder checked above")
                .join()
                .unwrap_or_else(|_| Err("the encoder thread panicked".into()));
            match result {
                Ok(()) => echo(
                    format!("render: wrote {}", film.output.display()),
                    console,
                    line,
                ),
                Err(error) => echo(format!("render: {error}"), console, line),
            }
            commands.remove_resource::<FilmRender>();
            // The playback was armed to quit at its end; that end is now.
            if let Some(playback) = playback.as_mut() {
                playback.retain_at_end = false;
            }
        }
    }
}

/// Read this frame back and hand the mix owed up to its end to the encoder.
fn capture_frame(
    commands: &mut Commands,
    film: &mut FilmRender,
    audio: Option<&audio::AudioRuntime>,
) {
    let frame = film.frames;
    film.frames += 1;
    film.in_flight.fetch_add(1, Ordering::AcqRel);
    let in_flight = film.in_flight.clone();
    let to_encoder = film.to_encoder.clone();
    commands.spawn(Screenshot::primary_window()).observe(
        move |mut captured: On<ScreenshotCaptured>| {
            let image = std::mem::take(&mut captured.event_mut().image);
            if let Some(to_encoder) = to_encoder.as_ref() {
                // A send fails only when the encoder already gave up; its
                // error is reported when it is joined.
                let _ = to_encoder.send(FilmPart::Frame(frame, Box::new(image)));
            }
            in_flight.fetch_sub(1, Ordering::AcqRel);
        },
    );
    if let Some(audio) = audio {
        let owed = film.audio_through(frame)
            - frame
                .checked_sub(1)
                .map_or(0, |previous| film.audio_through(previous));
        audio.advance_capture(owed);
        let samples = audio.take_captured();
        if !samples.is_empty() {
            send(film, FilmPart::Audio(samples));
        }
    }
}

fn send(film: &mut FilmRender, part: FilmPart) {
    if let Some(to_encoder) = film.to_encoder.as_ref() {
        let _ = to_encoder.send(part);
    }
}

struct EncodeJob {
    output: PathBuf,
    fps: u32,
    video_args: Vec<&'static str>,
}

impl EncodeJob {
    fn run(self, parts: Receiver<FilmPart>) -> Result<(), String> {
        let video_path = self.output.with_extension("video.mp4");
        let audio_path = self.output.with_extension("wav");
        let result = self.encode(&parts, &video_path, &audio_path);
        // Drain what is still coming so the frame loop never blocks on a
        // queue nobody reads.
        for _ in parts.iter() {}
        let result = result.and_then(|()| self.mux(&video_path, &audio_path));
        let _ = std::fs::remove_file(&video_path);
        let _ = std::fs::remove_file(&audio_path);
        result
    }

    fn encode(
        &self,
        parts: &Receiver<FilmPart>,
        video_path: &Path,
        audio_path: &Path,
    ) -> Result<(), String> {
        let mut wav = WavWriter::create(audio_path)?;
        let mut video: Option<VideoPipe> = None;
        let mut next = 0u64;
        let mut early = BTreeMap::new();
        for part in parts.iter() {
            match part {
                FilmPart::Audio(samples) => wav.write(&samples)?,
                FilmPart::Frame(index, image) => {
                    early.insert(index, image);
                    while let Some(image) = early.remove(&next) {
                        let rgb = image
                            .try_into_dynamic()
                            .map_err(|error| {
                                format!("frame {next} is not a readable format: {error}")
                            })?
                            .to_rgb8();
                        let pipe = match video.as_mut() {
                            Some(pipe) => pipe,
                            None => video.insert(VideoPipe::spawn(
                                video_path,
                                rgb.width(),
                                rgb.height(),
                                self.fps,
                                &self.video_args,
                            )?),
                        };
                        if (rgb.width(), rgb.height()) != pipe.size {
                            return Err(format!(
                                "frame {next} is {}x{}, the film started at {}x{}: the window was resized",
                                rgb.width(),
                                rgb.height(),
                                pipe.size.0,
                                pipe.size.1
                            ));
                        }
                        pipe.write(rgb.as_raw())?;
                        next += 1;
                    }
                }
            }
        }
        wav.finish()?;
        match video {
            Some(pipe) => pipe.finish(),
            None => Err("no frame was captured".into()),
        }
    }

    fn mux(&self, video_path: &Path, audio_path: &Path) -> Result<(), String> {
        let status = Command::new("ffmpeg")
            .args(["-hide_banner", "-loglevel", "error", "-y", "-i"])
            .arg(video_path)
            .arg("-i")
            .arg(audio_path)
            .args(["-c:v", "copy", "-c:a", "aac", "-b:a", "256k", "-shortest"])
            .arg(&self.output)
            .stdin(Stdio::null())
            .status()
            .map_err(|error| format!("ffmpeg mux was not run: {error}"))?;
        if status.success() {
            Ok(())
        } else {
            Err(format!("ffmpeg mux failed: {status}"))
        }
    }
}

struct VideoPipe {
    child: Child,
    stdin: ChildStdin,
    size: (u32, u32),
}

impl VideoPipe {
    fn spawn(
        path: &Path,
        width: u32,
        height: u32,
        fps: u32,
        video_args: &[&str],
    ) -> Result<Self, String> {
        let mut child = Command::new("ffmpeg")
            .args(["-hide_banner", "-loglevel", "error", "-y"])
            .args(["-f", "rawvideo", "-pix_fmt", "rgb24"])
            .args(["-s", &format!("{width}x{height}")])
            .args(["-framerate", &fps.to_string()])
            .args(["-i", "-"])
            // 4:2:0 needs even sides.
            .args(["-vf", "crop=trunc(iw/2)*2:trunc(ih/2)*2"])
            .args(video_args)
            .args(["-pix_fmt", "yuv420p"])
            .arg(path)
            .stdin(Stdio::piped())
            .spawn()
            .map_err(|error| format!("ffmpeg encode was not run: {error}"))?;
        let stdin = child.stdin.take().expect("stdin is piped");
        Ok(Self {
            child,
            stdin,
            size: (width, height),
        })
    }

    fn write(&mut self, rgb: &[u8]) -> Result<(), String> {
        self.stdin
            .write_all(rgb)
            .map_err(|error| format!("ffmpeg stopped taking frames: {error}"))
    }

    fn finish(self) -> Result<(), String> {
        let Self {
            mut child, stdin, ..
        } = self;
        drop(stdin);
        let status = child
            .wait()
            .map_err(|error| format!("ffmpeg encode: {error}"))?;
        if status.success() {
            Ok(())
        } else {
            Err(format!("ffmpeg encode failed: {status}"))
        }
    }
}

/// 32-bit float stereo WAV at the mix rate, its sizes filled in at the end.
struct WavWriter {
    file: BufWriter<File>,
    frames: u64,
}

impl WavWriter {
    const HEADER: u64 = 44;

    fn create(path: &Path) -> Result<Self, String> {
        let file =
            File::create(path).map_err(|error| format!("create {}: {error}", path.display()))?;
        let mut writer = Self {
            file: BufWriter::new(file),
            frames: 0,
        };
        writer.header()?;
        Ok(writer)
    }

    fn header(&mut self) -> Result<(), String> {
        let data = self.frames * 8;
        let rate = audio::SAMPLE_RATE;
        let mut header = Vec::with_capacity(Self::HEADER as usize);
        header.extend_from_slice(b"RIFF");
        header.extend_from_slice(&((36 + data).min(u64::from(u32::MAX)) as u32).to_le_bytes());
        header.extend_from_slice(b"WAVEfmt ");
        header.extend_from_slice(&16u32.to_le_bytes());
        header.extend_from_slice(&3u16.to_le_bytes()); // IEEE float
        header.extend_from_slice(&2u16.to_le_bytes());
        header.extend_from_slice(&rate.to_le_bytes());
        header.extend_from_slice(&(rate * 8).to_le_bytes());
        header.extend_from_slice(&8u16.to_le_bytes());
        header.extend_from_slice(&32u16.to_le_bytes());
        header.extend_from_slice(b"data");
        header.extend_from_slice(&(data.min(u64::from(u32::MAX)) as u32).to_le_bytes());
        self.file
            .write_all(&header)
            .map_err(|error| format!("audio write: {error}"))
    }

    fn write(&mut self, samples: &[[f32; 2]]) -> Result<(), String> {
        for [left, right] in samples {
            self.file
                .write_all(&left.to_le_bytes())
                .and_then(|()| self.file.write_all(&right.to_le_bytes()))
                .map_err(|error| format!("audio write: {error}"))?;
        }
        self.frames += samples.len() as u64;
        Ok(())
    }

    fn finish(mut self) -> Result<(), String> {
        self.file
            .seek(SeekFrom::Start(0))
            .map_err(|error| format!("audio write: {error}"))?;
        self.header()?;
        self.file
            .flush()
            .map_err(|error| format!("audio write: {error}"))
    }
}
