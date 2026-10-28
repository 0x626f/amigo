//! Shared asynchronous image loading and inline FFmpeg video playback.
use crate::design;
use amigo_core::media::{self, Kind};
use egui::{ColorImage, TextureHandle, TextureOptions};
use image::ImageDecoder;
use std::{
    collections::BTreeMap,
    io::Read,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

#[derive(Clone)]
pub struct Location {
    pub root: PathBuf,
    pub base: PathBuf,
}
#[derive(Default)]
struct Cache {
    entries: BTreeMap<String, Item>,
}
struct Item {
    receiver: Option<mpsc::Receiver<Result<Loaded, String>>>,
    loaded: Option<Loaded>,
    texture: Option<TextureHandle>,
    error: String,
    warning: String,
    player: Option<Player>,
    playing: bool,
    muted: bool,
    position: f64,
    started: Instant,
    offset: f64,
    seen: u64,
}
struct Loaded {
    path: PathBuf,
    size: [usize; 2],
    rgba: Vec<u8>,
    video: bool,
    duration: f64,
}
struct Player {
    receiver: mpsc::Receiver<Vec<u8>>,
    warnings: mpsc::Receiver<String>,
    children: Arc<Mutex<Vec<Child>>>,
    cancelled: Arc<AtomicBool>,
}
impl Drop for Player {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Release);
        let children = self.children.clone();
        std::thread::spawn(move || {
            if let Ok(mut children) = children.lock() {
                for mut child in children.drain(..) {
                    let _ = child.kill();
                    let _ = child.wait();
                }
            }
        });
    }
}
fn cache(ctx: &egui::Context) -> Arc<Mutex<Cache>> {
    ctx.data_mut(|data| {
        data.get_temp_mut_or_default::<Arc<Mutex<Cache>>>(egui::Id::new("inline-media-cache"))
            .clone()
    })
}
pub fn set_location(ctx: &egui::Context, location: Option<Location>) {
    ctx.data_mut(|data| data.insert_temp(egui::Id::new("markdown-media-location"), location));
}
pub fn show_current(ui: &mut egui::Ui, target: &str) {
    let location = ui
        .ctx()
        .data(|data| data.get_temp::<Option<Location>>(egui::Id::new("markdown-media-location")))
        .flatten();
    if let Some(location) = location {
        show(ui, &location, target);
    } else {
        design::hint(
            ui,
            "Open this note from a library to display its attachment.",
        );
    }
}
pub fn finish_frame(ctx: &egui::Context) {
    let cache = cache(ctx);
    let Ok(mut cache) = cache.lock() else {
        return;
    };
    let frame = ctx.cumulative_frame_nr();
    for item in cache.entries.values_mut() {
        if item.seen != frame {
            item.player = None;
            item.playing = false;
        }
    }
    while cache.entries.len() > 16 {
        let Some(old) = cache
            .entries
            .iter()
            .min_by_key(|(_, v)| v.seen)
            .map(|(k, _)| k.clone())
        else {
            break;
        };
        cache.entries.remove(&old);
    }
}
pub fn shutdown(ctx: &egui::Context) {
    let cache = cache(ctx);
    let Ok(mut cache) = cache.lock() else {
        return;
    };
    for item in cache.entries.values_mut() {
        if let Some(player) = item.player.take() {
            player.cancelled.store(true, Ordering::Release);
            if let Ok(mut children) = player.children.lock() {
                for mut child in children.drain(..) {
                    let _ = child.kill();
                    let _ = child.wait();
                }
            }
        }
    }
}
fn tool(name: &str) -> Command {
    let override_name = format!("AMIGO_{}", name.to_ascii_uppercase());
    let path = std::env::var_os(override_name)
        .map(PathBuf::from)
        .or_else(|| {
            let exe = std::env::current_exe().ok()?;
            let path = exe
                .parent()?
                .join(format!("{name}{}", std::env::consts::EXE_SUFFIX));
            path.is_file().then_some(path)
        })
        .unwrap_or_else(|| PathBuf::from(name));
    let mut command = Command::new(path);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    command.stdin(Stdio::null()).stderr(Stdio::null());
    command
}
fn video_info(path: &Path) -> Result<([usize; 2], f64), String> {
    let output = tool("ffprobe")
        .args([
            "-v",
            "error",
            "-protocol_whitelist",
            "file,pipe",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=width,height,sample_aspect_ratio:stream_tags=rotate:stream_side_data=rotation:format=duration",
            "-of",
            "json",
        ])
        .arg(path)
        .output()
        .map_err(|_| "Video playback requires FFmpeg (ffmpeg, ffprobe, and ffplay).".to_owned())?;
    let value: serde_json::Value =
        serde_json::from_slice(&output.stdout).map_err(|_| "Cannot read this video format.")?;
    let stream = &value["streams"][0];
    let size = video_dimensions(stream)?;
    let duration = value["format"]["duration"]
        .as_str()
        .and_then(|s| s.parse::<f64>().ok())
        .filter(|s| s.is_finite() && *s > 0.)
        .unwrap_or(0.);
    Ok((size, duration))
}
fn video_dimensions(stream: &serde_json::Value) -> Result<[usize; 2], String> {
    let mut width = stream["width"].as_u64().unwrap_or(0) as f64;
    let mut height = stream["height"].as_u64().unwrap_or(0) as f64;
    if width == 0. || height == 0. {
        return Err("This file has no decodable video track.".into());
    }
    if let Some((a, b)) = stream["sample_aspect_ratio"]
        .as_str()
        .and_then(|s| s.split_once(':'))
        && let (Ok(a), Ok(b)) = (a.parse::<f64>(), b.parse::<f64>())
        && a.is_finite()
        && b.is_finite()
        && a > 0.
        && b > 0.
    {
        width *= a / b;
    }
    let rotation = stream["side_data_list"]
        .as_array()
        .and_then(|list| list.iter().find_map(|data| data["rotation"].as_f64()))
        .or_else(|| {
            stream["tags"]["rotate"]
                .as_str()
                .and_then(|s| s.parse().ok())
        })
        .unwrap_or(0.)
        .rem_euclid(180.);
    if (45. ..135.).contains(&rotation) {
        std::mem::swap(&mut width, &mut height);
    }
    let factor = (960. / width).min(540. / height).min(1.);
    Ok([
        ((width * factor).round() as usize / 2 * 2).max(2),
        ((height * factor).round() as usize / 2 * 2).max(2),
    ])
}
fn load(location: Location, target: String) -> Result<Loaded, String> {
    let path = media::resolve(&location.root, &location.base, &target)?;
    let kind =
        media::kind(&path.to_string_lossy()).ok_or("This attachment is not an image or video.")?;
    if kind == Kind::Video {
        let (size, duration) = video_info(&path)?;
        let scale = format!("scale={}:{}", size[0], size[1]);
        let output = tool("ffmpeg")
            .args([
                "-v",
                "error",
                "-nostdin",
                "-protocol_whitelist",
                "file,pipe",
                "-i",
            ])
            .arg(&path)
            .args([
                "-frames:v",
                "1",
                "-vf",
                &scale,
                "-pix_fmt",
                "rgba",
                "-f",
                "rawvideo",
                "pipe:1",
            ])
            .output()
            .map_err(|_| "Video playback requires FFmpeg.".to_owned())?;
        if !output.status.success() || output.stdout.len() != size[0] * size[1] * 4 {
            return Err("Cannot decode this video's first frame.".into());
        }
        Ok(Loaded {
            path,
            size,
            rgba: output.stdout,
            video: true,
            duration,
        })
    } else {
        let mut reader = image::ImageReader::open(&path)
            .map_err(|e| e.to_string())?
            .with_guessed_format()
            .map_err(|e| e.to_string())?;
        let mut limits = image::Limits::default();
        limits.max_alloc = Some(192 * 1024 * 1024);
        limits.max_image_width = Some(16000);
        limits.max_image_height = Some(16000);
        reader.limits(limits);
        let mut decoder = reader.into_decoder().map_err(|e| e.to_string())?;
        let orientation = decoder.orientation().map_err(|e| e.to_string())?;
        let mut image = image::DynamicImage::from_decoder(decoder)
            .map_err(|e| format!("Cannot decode image: {e}"))?;
        image.apply_orientation(orientation);
        let image = if image.width() > 1600 || image.height() > 1200 {
            image.thumbnail(1600, 1200)
        } else {
            image
        }
        .to_rgba8();
        Ok(Loaded {
            path,
            size: [image.width() as usize, image.height() as usize],
            rgba: image.into_raw(),
            video: false,
            duration: 0.,
        })
    }
}
fn begin(item: &mut Item, ctx: &egui::Context, playing: bool) {
    item.player = None;
    item.warning.clear();
    item.playing = playing;
    item.offset = item.position;
    item.started = Instant::now();
    let Some(loaded) = &item.loaded else {
        return;
    };
    let path = loaded.path.clone();
    let size = loaded.size;
    let offset = item.position;
    let audio = playing && !item.muted;
    let children = Arc::new(Mutex::new(Vec::<Child>::new()));
    let handles = children.clone();
    let ctx = ctx.clone();
    let cancelled = Arc::new(AtomicBool::new(false));
    let stop = cancelled.clone();
    let (sender, receiver) = mpsc::sync_channel(2);
    let (warning_sender, warnings) = mpsc::channel();
    std::thread::spawn(move || {
        let filter = format!("fps=24,scale={}:{}", size[0], size[1]);
        let mut command = tool("ffmpeg");
        command.args([
            "-v",
            "error",
            "-nostdin",
            "-protocol_whitelist",
            "file,pipe",
            "-ss",
            &offset.to_string(),
        ]);
        if playing {
            command.arg("-re");
        }
        command
            .arg("-i")
            .arg(&path)
            .args(["-an", "-vf", &filter, "-pix_fmt", "rgba"]);
        if !playing {
            command.args(["-frames:v", "1"]);
        }
        command
            .args(["-f", "rawvideo", "pipe:1"])
            .stdout(Stdio::piped());
        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(error) => {
                let _ = warning_sender.send(format!("Cannot start video playback: {error}"));
                ctx.request_repaint();
                return;
            }
        };
        let Some(mut pipe) = child.stdout.take() else {
            let _ = child.kill();
            let _ = child.wait();
            return;
        };
        {
            let mut handles = handles.lock().unwrap();
            if stop.load(Ordering::Acquire) {
                let _ = child.kill();
                let _ = child.wait();
                return;
            }
            handles.push(child);
        }
        if audio {
            let result = tool("ffplay")
                .args([
                    "-v",
                    "error",
                    "-protocol_whitelist",
                    "file,pipe",
                    "-nodisp",
                    "-autoexit",
                    "-ss",
                    &offset.to_string(),
                    "-volume",
                    "75",
                    "-vn",
                    "-i",
                ])
                .arg(&path)
                .spawn();
            match result {
                Ok(mut child) => {
                    let mut handles = handles.lock().unwrap();
                    if stop.load(Ordering::Acquire) {
                        let _ = child.kill();
                        let _ = child.wait();
                    } else {
                        handles.push(child);
                    }
                }
                Err(_) => {
                    let _ = warning_sender
                        .send("Audio requires ffplay. Video can continue muted.".into());
                }
            }
        }
        loop {
            let mut frame = vec![0; size[0] * size[1] * 4];
            if pipe.read_exact(&mut frame).is_err() {
                break;
            }
            match sender.try_send(frame) {
                Ok(()) | Err(mpsc::TrySendError::Full(_)) => ctx.request_repaint(),
                Err(mpsc::TrySendError::Disconnected(_)) => break,
            }
        }
        if !stop.load(Ordering::Acquire)
            && let Ok(mut children) = handles.lock()
            && let Some(video) = children.first_mut()
            && video.wait().is_ok_and(|status| !status.success())
        {
            let _ = warning_sender.send("Video decoding stopped. This codec may not be supported by your FFmpeg installation.".into());
        }
        ctx.request_repaint();
    });
    item.player = Some(Player {
        receiver,
        warnings,
        children,
        cancelled,
    });
}
pub fn show(ui: &mut egui::Ui, location: &Location, target: &str) {
    let cache = cache(ui.ctx());
    let Ok(mut cache) = cache.lock() else {
        return;
    };
    let key = format!("{:?}|{:?}|{target}", location.root, location.base);
    let item = cache.entries.entry(key.clone()).or_insert_with(|| {
        let (sender, receiver) = mpsc::channel();
        let location = location.clone();
        let target = target.to_owned();
        let ctx = ui.ctx().clone();
        std::thread::spawn(move || {
            let _ = sender.send(load(location, target));
            ctx.request_repaint();
        });
        Item {
            receiver: Some(receiver),
            loaded: None,
            texture: None,
            error: String::new(),
            warning: String::new(),
            player: None,
            playing: false,
            muted: false,
            position: 0.,
            started: Instant::now(),
            offset: 0.,
            seen: 0,
        }
    });
    item.seen = ui.ctx().cumulative_frame_nr();
    if let Some(receiver) = &item.receiver {
        match receiver.try_recv() {
            Ok(Ok(mut loaded)) => {
                item.texture = Some(ui.ctx().load_texture(
                    &key,
                    ColorImage::from_rgba_unmultiplied(loaded.size, &loaded.rgba),
                    TextureOptions::LINEAR,
                ));
                loaded.rgba = Vec::new();
                item.loaded = Some(loaded);
                item.receiver = None;
            }
            Ok(Err(error)) => {
                item.error = error;
                item.receiver = None;
            }
            _ => (),
        }
    }
    let mut ended = false;
    if let Some(player) = &item.player {
        let mut frame = None;
        loop {
            match player.receiver.try_recv() {
                Ok(next) => frame = Some(next),
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    ended = true;
                    break;
                }
            }
        }
        while let Ok(warning) = player.warnings.try_recv() {
            item.warning = warning;
        }
        if let Some(frame) = frame
            && let (Some(texture), Some(loaded)) = (&mut item.texture, &item.loaded)
        {
            texture.set(
                ColorImage::from_rgba_unmultiplied(loaded.size, &frame),
                TextureOptions::LINEAR,
            );
        }
    }
    if ended {
        if item.playing
            && item.warning.is_empty()
            && let Some(loaded) = &item.loaded
            && loaded.duration > 0.
        {
            item.position = loaded.duration;
        }
        item.playing = false;
        item.player = None;
    }
    if !item.error.is_empty() {
        ui.colored_label(design::colors(ui).red, &item.error);
        return;
    }
    let Some(loaded) = &item.loaded else {
        ui.horizontal(|ui| {
            ui.spinner();
            ui.label("Loading attachment…");
        });
        ui.ctx().request_repaint_after(Duration::from_millis(50));
        return;
    };
    let texture = item.texture.as_ref().unwrap();
    let natural = egui::vec2(loaded.size[0] as f32, loaded.size[1] as f32);
    let factor = (ui.available_width() / natural.x)
        .min(420. / natural.y)
        .min(1.);
    let response = ui.add(egui::Image::new((texture.id(), natural * factor)).corner_radius(6));
    if !ui.is_rect_visible(response.rect) {
        item.player = None;
        item.playing = false;
        return;
    }
    if loaded.video {
        let duration = loaded.duration;
        if item.playing {
            item.position = (item.offset + item.started.elapsed().as_secs_f64())
                .min(duration.max(item.offset + 1.));
            if duration > 0. && item.position >= duration {
                item.playing = false;
                item.player = None;
            }
            ui.ctx().request_repaint_after(Duration::from_millis(30));
        }
        let mut restart = false;
        ui.push_id(&key, |ui| {
            ui.horizontal_wrapped(|ui| {
                if design::small_button(ui, if item.playing { "Pause" } else { "Play" }).clicked() {
                    if item.playing {
                        item.playing = false;
                        item.player = None;
                    } else {
                        if duration > 0. && item.position >= duration {
                            item.position = 0.;
                        }
                        item.playing = true;
                        restart = true;
                    }
                }
                if design::small_button(ui, "Stop").clicked() {
                    item.position = 0.;
                    item.playing = false;
                    restart = true;
                }
                if design::small_button(ui, if item.muted { "Muted" } else { "Sound" }).clicked() {
                    item.muted = !item.muted;
                    restart = item.playing;
                }
                let seek = ui.add(
                    egui::Slider::new(&mut item.position, 0.0..=duration.max(0.1))
                        .show_value(false),
                );
                if seek.drag_stopped() || seek.changed() && !seek.dragged() {
                    restart = true;
                }
                ui.label(format!(
                    "{} / {}",
                    timestamp(item.position),
                    timestamp(duration)
                ));
            })
        });
        if restart {
            begin(item, ui.ctx(), item.playing);
        }
        if !item.warning.is_empty() {
            ui.colored_label(design::colors(ui).amber, &item.warning);
        }
    }
}
fn timestamp(seconds: f64) -> String {
    let n = seconds.max(0.) as u64;
    format!("{}:{:02}", n / 60, n % 60)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn video_display_size_respects_portrait_rotation_and_pixel_aspect() {
        assert_eq!(
            video_dimensions(
                &serde_json::json!({"width":1920,"height":1080,"side_data_list":[{"rotation":-90}]})
            )
            .unwrap(),
            [304, 540]
        );
        assert_eq!(
            video_dimensions(
                &serde_json::json!({"width":720,"height":576,"sample_aspect_ratio":"2:1"})
            )
            .unwrap(),
            [960, 384]
        );
        assert!(video_dimensions(&serde_json::json!({"width":0,"height":1080})).is_err());
    }
    #[test]
    #[ignore = "requires system FFmpeg tools for real video decoding"]
    fn video_poster_play_seek_and_stop() {
        let root = std::env::temp_dir().join(format!("amigo-video-{}", amigo_core::model::id()));
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("sample.mp4");
        let output = tool("ffmpeg")
            .args([
                "-v",
                "error",
                "-nostdin",
                "-f",
                "lavfi",
                "-i",
                "testsrc2=size=64x48:rate=24",
                "-t",
                "2",
                "-c:v",
                "mpeg4",
                "-q:v",
                "4",
            ])
            .arg(&path)
            .output()
            .unwrap();
        assert!(output.status.success());
        let rotated = root.join("portrait.mp4");
        assert!(
            tool("ffmpeg")
                .args(["-v", "error", "-nostdin", "-display_rotation", "90", "-i"])
                .arg(&path)
                .args(["-c", "copy"])
                .arg(&rotated)
                .status()
                .unwrap()
                .success()
        );
        let portrait = load(
            Location {
                root: root.clone(),
                base: root.clone(),
            },
            "portrait.mp4".into(),
        )
        .unwrap();
        assert_eq!(portrait.size, [48, 64]);
        let loaded = load(
            Location {
                root: root.clone(),
                base: root.clone(),
            },
            "sample.mp4".into(),
        )
        .unwrap();
        assert_eq!(loaded.size, [64, 48]);
        assert_eq!(loaded.rgba.len(), 64 * 48 * 4);
        assert!((loaded.duration - 2.).abs() < 0.1);
        let mut item = Item {
            receiver: None,
            loaded: Some(loaded),
            texture: None,
            error: String::new(),
            warning: String::new(),
            player: None,
            playing: false,
            muted: true,
            position: 0.,
            started: Instant::now(),
            offset: 0.,
            seen: 0,
        };
        let ctx = egui::Context::default();
        begin(&mut item, &ctx, true);
        let first = item
            .player
            .as_ref()
            .unwrap()
            .receiver
            .recv_timeout(Duration::from_secs(5))
            .unwrap();
        assert_eq!(first.len(), 64 * 48 * 4);
        let stopped = item.player.as_ref().unwrap().cancelled.clone();
        item.position = 1.;
        begin(&mut item, &ctx, false);
        assert!(stopped.load(Ordering::Acquire));
        let sought = item
            .player
            .as_ref()
            .unwrap()
            .receiver
            .recv_timeout(Duration::from_secs(5))
            .unwrap();
        assert_eq!(sought.len(), first.len());
        assert_ne!(sought, first);
        let stopped = item.player.as_ref().unwrap().cancelled.clone();
        drop(item);
        assert!(stopped.load(Ordering::Acquire));
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn local_png_decodes_and_rejects_external_urls() {
        let root = std::env::temp_dir().join(format!("amigo-image-{}", amigo_core::model::id()));
        std::fs::create_dir_all(&root).unwrap();
        image::RgbaImage::from_pixel(3, 2, image::Rgba([10, 20, 30, 255]))
            .save(root.join("a.png"))
            .unwrap();
        let location = Location {
            root: root.clone(),
            base: root.clone(),
        };
        let result = load(location.clone(), "a.png".into()).unwrap();
        assert_eq!(result.size, [3, 2]);
        assert_eq!(&result.rgba[..4], &[10, 20, 30, 255]);
        assert!(load(location, "https://example.com/image.png".into()).is_err());
        std::fs::remove_dir_all(root).unwrap();
    }
}
