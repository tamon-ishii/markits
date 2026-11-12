use clap::{Args, Subcommand};
use markits::{
    cli_support::{self, Result},
    metadata,
    recording::{OperationRecording, RecordedEvent, RecordingSession},
};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

#[derive(Debug, Subcommand)]
pub enum HistoryCommand {
    /// List capture history as JSON
    List,
    /// Export the stored capture, including metadata
    Load {
        id: String,
        #[arg(short, long)]
        output: PathBuf,
    },
    /// Add an image to desktop history
    Save { image: PathBuf },
    /// Delete one history item and its thumbnail/base image
    Delete { id: String },
}

#[derive(Debug, Subcommand)]
pub enum RecordCommand {
    /// Start a background recording session; use screenshot/finish from another invocation
    Start {
        /// New session directory (must not already exist)
        #[arg(long)]
        session: PathBuf,
        #[arg(long, required_unless_present = "pid", conflicts_with = "pid")]
        window: Option<String>,
        #[arg(long)]
        pid: Option<u32>,
    },
    /// Show recording state, event count and screenshot paths as JSON
    Status {
        #[arg(long)]
        session: PathBuf,
    },
    /// Pause input monitoring in an active session
    Pause {
        #[arg(long)]
        session: PathBuf,
    },
    /// Resume input monitoring in an active session
    Resume {
        #[arg(long)]
        session: PathBuf,
    },
    /// Capture the target now and record this capture point
    Screenshot {
        #[arg(long)]
        session: PathBuf,
        #[arg(long)]
        include_cursor: bool,
    },
    /// Stop recording and save recording.json plus screenshots with embedded operations
    Finish {
        #[arg(long)]
        session: PathBuf,
    },
    /// Stop recording without saving operations (existing screenshots remain)
    Cancel {
        #[arg(long)]
        session: PathBuf,
    },
}

#[derive(Debug, Args)]
pub struct ReplayArgs {
    /// recording.json or an image containing embedded operations
    pub input: PathBuf,
    /// Target window ID or unique title substring; defaults to recorded title
    #[arg(long, conflicts_with = "pid")]
    pub window: Option<String>,
    /// Target process ID (must identify one window)
    #[arg(long)]
    pub pid: Option<u32>,
    /// Destination directory for recaptured images
    #[arg(short, long, required_unless_present = "dry_run")]
    pub output_dir: Option<PathBuf>,
    /// Validate and print operations without sending input or taking screenshots
    #[arg(long)]
    pub dry_run: bool,
    /// Skip recorded idle time (settle time after actions is retained)
    #[arg(long)]
    pub shorten_waits: bool,
    /// Maximum wait per event in milliseconds
    #[arg(long,default_value_t=60000,value_parser=clap::value_parser!(u64).range(0..=3600000))]
    pub max_wait_ms: u64,
    /// Include cursor in every recaptured screenshot (Windows only)
    #[arg(long)]
    pub include_cursor: bool,
}

pub fn export(image: &Path, output: &Path, editable: bool) -> Result<()> {
    let png = cli_support::to_png(&fs::read(image)?)?;
    let png = if editable {
        png
    } else {
        markits::image_metadata::strip_markits_metadata(&png)?
    };
    cli_support::write_image(output, &png)
}

pub fn history(command: HistoryCommand) -> Result<()> {
    match command {
        HistoryCommand::List => println!(
            "{}",
            serde_json::to_string_pretty(&markits::history::list_history()?)?
        ),
        HistoryCommand::Load { id, output } => {
            markits::history::validate_id(&id)?;
            let path = markits::history::get_history_dir().join(format!("{id}.png"));
            let stored = fs::read(path)?;
            let mut png = if let Some(json) =
                metadata::extract_text_chunk(&stored, metadata::MARKITS_KEYWORD)?
            {
                let loaded = metadata::load_image_with_metadata(&stored)?;
                let background = decode_url(&loaded.image_data_url)?;
                let rendered = cli_support::render(&json, &background, None, true)?;
                if let Some(crop) = loaded.crop_info {
                    metadata::embed_crop_info(&rendered, &crop)?
                } else {
                    rendered
                }
            } else {
                stored
            };
            let base = markits::history::get_history_dir().join(format!("{id}.base.png"));
            if base.is_file() {
                png = metadata::embed_text_chunk(
                    &png,
                    "markits:base_image",
                    &cli_support::data_url(&fs::read(base)?),
                )?;
            }
            cli_support::write_image(&output, &png)?;
        }
        HistoryCommand::Save { image } => {
            let png = cli_support::to_png(&fs::read(image)?)?;
            let loaded = metadata::load_image_with_metadata(&png)?;
            let background = decode_url(&loaded.image_data_url)?;
            let base = loaded
                .base_image_data_url
                .as_deref()
                .map(decode_url)
                .transpose()?;
            let item = markits::history::save_or_update_history_item(
                None,
                &background,
                loaded.annotations_json.as_deref(),
                loaded.ui_elements.as_deref(),
                base.as_deref(),
                loaded.crop_info.as_ref(),
            )?;
            println!("{}", serde_json::to_string_pretty(&item)?);
        }
        HistoryCommand::Delete { id } => markits::history::delete_history_item(&id)?,
    }
    Ok(())
}

pub fn crop(
    image: &Path,
    output: &Path,
    x: u32,
    y: u32,
    w: u32,
    h: u32,
    editable: bool,
) -> Result<()> {
    let source = cli_support::to_png(&fs::read(image)?)?;
    let mut png = cli_support::crop_png(&source, x, y, w, h)?;
    if let Some(elements) = markits::raster::extract_png_uimap(&source) {
        png = markits::raster::embed_png_uimap(
            &png,
            &markits::raster::filter_ui_elements_for_crop(
                &elements, x as f64, y as f64, w as f64, h as f64,
            ),
        )?;
    }
    if editable {
        let loaded = metadata::load_image_with_metadata(&source)?;
        let background = decode_url(&loaded.image_data_url)?;
        if let Some(json) = loaded.annotations_json {
            let mut scene = cli_support::translate_scene(&json, x as f64, y as f64)?;
            scene["canvas"] = serde_json::json!({"width":w,"height":h});
            scene["uimap"] =
                serde_json::to_value(markits::raster::extract_png_uimap(&png).unwrap_or_default())?;
            png = metadata::embed_annotations(&png, &serde_json::to_string(&scene)?)?;
            png = metadata::embed_text_chunk(
                &png,
                metadata::MARKITS_SOURCE_KEYWORD,
                &cli_support::data_url(&cli_support::crop_png(&background, x, y, w, h)?),
            )?;
        }
        let previous = loaded.crop_info;
        let offset_x = x as f64 + previous.as_ref().map_or(0.0, |c| c.offset_x);
        let offset_y = y as f64 + previous.as_ref().map_or(0.0, |c| c.offset_y);
        png = metadata::embed_crop_info(
            &png,
            &metadata::CropInfo {
                is_auto_cropped: false,
                offset_x,
                offset_y,
                base_width: previous.as_ref().map_or(loaded.width, |c| c.base_width),
                base_height: previous.as_ref().map_or(loaded.height, |c| c.base_height),
            },
        )?;
        let base = loaded
            .base_image_data_url
            .unwrap_or_else(|| cli_support::data_url(&background));
        png = metadata::embed_text_chunk(&png, "markits:base_image", &base)?;
        png = markits::recording_metadata::copy_metadata(&png, &source)?;
    }
    cli_support::write_image(output, &png)
}

pub fn capture_native(
    window: &markits::window_capture::WindowInfo,
    detect: bool,
    cursor: bool,
) -> Result<markits::capture::CapturedImage> {
    let temp = std::env::temp_dir().join(format!(
        "markits-cli-{}-{}.png",
        std::process::id(),
        nonce()
    ));
    let capture = (|| -> Result<_> {
        let bounds = markits::window_capture::capture_window(&window.id, 0, &temp, false, None)?;
        let mut png = fs::read(&temp)?;
        if cursor {
            png = cli_support::cursor(&png, &bounds)?;
        }
        let image = image::load_from_memory(&png)?;
        let mut result = markits::capture::rgba_to_captured_image(&image.to_rgba8())?;
        if detect {
            let target = markits::semantic_target::window_context(&window.id)?;
            let elements =
                markits::ui_elements::capture_desktop_detailed_elements_for_window(0, 0, &target);
            let mut elements = markits::ui_elements::filter_elements_for_crop(
                &elements,
                bounds.x as f64,
                bounds.y as f64,
                bounds.width as f64,
                bounds.height as f64,
            );
            for e in &mut elements {
                e.x *= image.width() as f64 / bounds.width as f64;
                e.width *= image.width() as f64 / bounds.width as f64;
                e.y *= image.height() as f64 / bounds.height as f64;
                e.height *= image.height() as f64 / bounds.height as f64;
            }
            result.ui_elements = elements;
        }
        if detect {
            result.raw_png =
                markits::raster::embed_png_uimap(&result.raw_png, &result.to_ui_elements())?;
        }
        result.raw_png = metadata::embed_text_chunk(
            &result.raw_png,
            metadata::MARKITS_CAPTURE_TARGET_KEYWORD,
            &serde_json::to_string(&markits::semantic_target::window_context(&window.id)?)?,
        )?;
        Ok(result)
    })();
    let _ = fs::remove_file(&temp);
    capture
}

pub fn capture_screen_cursor(
    mut captured: markits::capture::CapturedImage,
    screen: Option<usize>,
    x: Option<u32>,
    y: Option<u32>,
) -> Result<markits::capture::CapturedImage> {
    let screens = markits::capture::list_screens()?;
    let index = screen.unwrap_or_else(|| screens.iter().position(|s| s.is_primary).unwrap_or(0));
    let s = screens.get(index).ok_or("Screen index not found")?;
    let bounds = markits::window_capture::WindowInfo {
        id: "screen".into(),
        title: "Screen".into(),
        x: s.x + x.unwrap_or(0) as i32,
        y: s.y + y.unwrap_or(0) as i32,
        width: captured.width,
        height: captured.height,
    };
    captured.raw_png = cli_support::cursor(&captured.raw_png, &bounds)?;
    Ok(captured)
}

#[derive(Serialize, Deserialize)]
struct Config {
    window: String,
    pid: u32,
    depth: usize,
}
#[derive(Serialize, Deserialize)]
struct Request {
    action: String,
    #[serde(default)]
    include_cursor: bool,
}
#[derive(Serialize, Deserialize)]
struct SavedRecording {
    recording: OperationRecording,
    window_id: String,
    pid: u32,
}
fn nonce() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
}
fn decode_url(url: &str) -> Result<Vec<u8>> {
    use base64::Engine;
    Ok(base64::engine::general_purpose::STANDARD
        .decode(url.split_once(',').ok_or("Invalid image data URL")?.1)?)
}
fn json_write(path: &Path, value: &impl Serialize) -> Result<()> {
    let temp = path.with_extension(format!("{}.tmp", nonce()));
    fs::write(&temp, serde_json::to_vec_pretty(value)?)?;
    // Windows rename cannot replace an existing file.
    if path.exists() {
        fs::remove_file(path)?;
    }
    fs::rename(temp, path)?;
    Ok(())
}
fn status(session: &Path) -> Result<serde_json::Value> {
    for _ in 0..5 {
        if let Ok(bytes) = fs::read(session.join("status.json")) {
            return Ok(serde_json::from_slice(&bytes)?);
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    Err("Recording status is unavailable; check worker.log".into())
}
pub fn record(command: RecordCommand) -> Result<()> {
    if let RecordCommand::Start {
        session,
        window,
        pid,
    } = command
    {
        let window = cli_support::native_window(window.as_deref(), pid)?;
        let context = markits::semantic_target::window_context(&window.id)?;
        fs::create_dir(&session)?;
        let session = fs::canonicalize(session)?;
        json_write(
            &session.join("config.json"),
            &Config {
                window: window.id,
                pid: context.pid.unwrap_or(0),
                depth: markits::ui_elements::capture_depth(),
            },
        )?;
        let log = fs::File::create(session.join("worker.log"))?;
        let mut child = std::process::Command::new(std::env::current_exe()?);
        child
            .arg("record-worker")
            .arg(&session)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(log);
        #[cfg(target_os = "windows")]
        {
            use std::os::windows::process::CommandExt;
            child.creation_flags(0x08000000);
        }
        let mut child = child.spawn()?;
        let deadline = Instant::now() + Duration::from_secs(12);
        loop {
            if session.join("status.json").exists() {
                let current = status(&session)?;
                if let Some(error) = current["error"].as_str() {
                    return Err(error.to_string().into());
                }
                println!("{current}");
                return Ok(());
            }
            if let Some(code) = child.try_wait()? {
                return Err(format!(
                    "Recording worker exited ({code}); see {}",
                    session.join("worker.log").display()
                )
                .into());
            }
            if Instant::now() > deadline {
                let _ = child.kill();
                let _ = child.wait();
                return Err("Recording worker did not become ready".into());
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }
    let (session, action, cursor) = match command {
        RecordCommand::Status { session } => {
            println!("{}", status(&session)?);
            return Ok(());
        }
        RecordCommand::Pause { session } => (session, "pause", false),
        RecordCommand::Resume { session } => (session, "resume", false),
        RecordCommand::Screenshot {
            session,
            include_cursor,
        } => (session, "screenshot", include_cursor),
        RecordCommand::Finish { session } => (session, "finish", false),
        RecordCommand::Cancel { session } => (session, "cancel", false),
        RecordCommand::Start { .. } => unreachable!(),
    };
    if !status(&session)?["running"].as_bool().unwrap_or(false) {
        return Err("Recording is already stopped".into());
    }
    let request = session.join(format!("request-{}.json", nonce()));
    json_write(
        &request,
        &Request {
            action: action.into(),
            include_cursor: cursor,
        },
    )?;
    let response = request.with_extension("response");
    let deadline = Instant::now() + Duration::from_secs(60);
    while !response.exists() {
        if let Ok(current) = status(&session) {
            if let Some(error) = current["error"].as_str() {
                return Err(error.to_string().into());
            }
        }
        if Instant::now() > deadline {
            return Err(
                "Recorder did not respond; inspect status and worker.log before retrying".into(),
            );
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let response: serde_json::Value = serde_json::from_slice(&fs::read(response)?)?;
    if let Some(error) = response["error"].as_str() {
        return Err(error.to_string().into());
    }
    println!("{response}");
    Ok(())
}

pub fn record_worker(session: &Path) -> Result<()> {
    let result = record_worker_inner(session);
    if let Err(error) = &result {
        let _ = json_write(
            &session.join("status.json"),
            &serde_json::json!({"running":false,"error":error.to_string()}),
        );
    }
    result
}
fn record_worker_inner(session: &Path) -> Result<()> {
    let config: Config = serde_json::from_slice(&fs::read(session.join("config.json"))?)?;
    let _depth = markits::ui_elements::capture_depth_scope(config.depth);
    let origin = cli_support::native_window(Some(&config.window), None)?;
    let mut recorder = RecordingSession::start(&config.window, &std::env::current_exe()?)?;
    let mut screenshots = Vec::new();
    let mut manual_events = Vec::new();
    loop {
        let current = recorder.status(None)?;
        json_write(
            &session.join("status.json"),
            &serde_json::json!({"running":current.running,"paused":current.paused,"events":current.events.len()+manual_events.len(),"screenshots":screenshots,"worker_pid":std::process::id()}),
        )?;
        if !current.running {
            return Err("Input recorder stopped; use CLI finish before stopping the helper".into());
        }
        let mut requests = fs::read_dir(session)?
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| {
                p.file_name()
                    .and_then(|s| s.to_str())
                    .is_some_and(|s| s.starts_with("request-") && s.ends_with(".json"))
            })
            .collect::<Vec<_>>();
        requests.sort();
        for request in requests {
            let command: Request = serde_json::from_slice(&fs::read(&request)?)?;
            let response = request.with_extension("response");
            let action = (|| -> Result<serde_json::Value> {
                match command.action.as_str() {
                    "pause" => recorder.pause(true)?,
                    "resume" => recorder.pause(false)?,
                    "screenshot" => {
                        if recorder.status(None)?.paused {
                            return Err("Resume recording before taking a screenshot".into());
                        }
                        let window = cli_support::native_window(Some(&config.window), None)?;
                        let pointer = if command.include_cursor {
                            markits::cursor_capture::pointer_in_bounds(&window)?.map(|mut p| {
                                p.x = origin.x as f64
                                    + (p.x - window.x as f64) * origin.width as f64
                                        / window.width as f64;
                                p.y = origin.y as f64
                                    + (p.y - window.y as f64) * origin.height as f64
                                        / window.height as f64;
                                p
                            })
                        } else {
                            None
                        };
                        let elapsed_ms = recorder.elapsed_ms()?;
                        let captured = capture_native(&window, true, command.include_cursor)?;
                        let path =
                            session.join(format!("capture_{:03}.png", screenshots.len() + 1));
                        fs::write(&path, &captured.raw_png)?;
                        screenshots.push(path);
                        manual_events.push(RecordedEvent::Screenshot {
                            elapsed_ms,
                            include_cursor: command.include_cursor,
                            pointer,
                        });
                    }
                    "finish" | "cancel" => {}
                    _ => return Err("Unknown recorder action".into()),
                }
                Ok(serde_json::json!({"ok":true,"action":command.action,"screenshots":screenshots}))
            })();
            if let Err(error) = action {
                json_write(&response, &serde_json::json!({"error":error.to_string()}))?;
                fs::remove_file(request)?;
                continue;
            }
            if command.action == "finish" {
                let mut recording = recorder.finish(None)?;
                recording.events.extend(manual_events);
                recording.events.sort_by_key(event_time);
                let saved = SavedRecording {
                    recording,
                    window_id: config.window,
                    pid: config.pid,
                };
                json_write(&session.join("recording.json"), &saved)?;
                for path in &screenshots {
                    let png = metadata::embed_text_chunk(
                        &fs::read(path)?,
                        markits::recording_metadata::OPERATIONS_KEY,
                        &serde_json::to_string(&saved)?,
                    )?;
                    fs::write(path, png)?;
                }
                json_write(
                    &session.join("status.json"),
                    &serde_json::json!({"running":false,"saved":session.join("recording.json"),"screenshots":screenshots}),
                )?;
                json_write(
                    &response,
                    &serde_json::json!({"ok":true,"saved":session.join("recording.json"),"screenshots":screenshots}),
                )?;
                fs::remove_file(request)?;
                return Ok(());
            }
            if command.action == "cancel" {
                drop(recorder);
                json_write(
                    &session.join("status.json"),
                    &serde_json::json!({"running":false,"cancelled":true}),
                )?;
                json_write(&response, &serde_json::json!({"ok":true,"cancelled":true}))?;
                fs::remove_file(request)?;
                return Ok(());
            }
            let updated = recorder.status(None)?;
            json_write(
                &session.join("status.json"),
                &serde_json::json!({"running":updated.running,"paused":updated.paused,"events":updated.events.len()+manual_events.len(),"screenshots":screenshots,"worker_pid":std::process::id()}),
            )?;
            json_write(&response, &action?)?;
            fs::remove_file(request)?;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn event_time(e: &RecordedEvent) -> u64 {
    match e {
        RecordedEvent::Click { elapsed_ms, .. }
        | RecordedEvent::Text { elapsed_ms, .. }
        | RecordedEvent::Key { elapsed_ms, .. }
        | RecordedEvent::Scroll { elapsed_ms, .. }
        | RecordedEvent::Screenshot { elapsed_ms, .. } => *elapsed_ms,
    }
}

pub fn replay(args: ReplayArgs) -> Result<()> {
    let captures = replay_capture(args)?;
    if !captures.is_empty() {
        println!("{}", serde_json::to_string_pretty(&captures)?);
    }
    Ok(())
}

pub fn replay_capture(args: ReplayArgs) -> Result<Vec<PathBuf>> {
    let bytes = fs::read(&args.input)?;
    let value: serde_json::Value = if let Ok(value) = serde_json::from_slice(&bytes) {
        value
    } else {
        let png = cli_support::to_png(&bytes)?;
        serde_json::from_str(
            &metadata::extract_text_chunk(&png, markits::recording_metadata::OPERATIONS_KEY)?
                .ok_or("Image has no operation recording")?,
        )?
    };
    let recording: OperationRecording =
        serde_json::from_value(value.get("recording").unwrap_or(&value).clone())?;
    validate_recording(&recording)?;
    if args.dry_run {
        println!("{}", serde_json::to_string_pretty(&recording)?);
        return Ok(Vec::new());
    }
    if !cfg!(target_os = "windows")
        && (args.include_cursor
            || recording.events.iter().any(|e| {
                matches!(
                    e,
                    RecordedEvent::Screenshot {
                        include_cursor: true,
                        ..
                    }
                )
            }))
    {
        return Err("Cursor capture requires Windows".into());
    }
    let query = args.window.as_deref().or_else(|| {
        if args.pid.is_none() {
            Some(recording.window.as_str())
        } else {
            None
        }
    });
    let window = cli_support::native_window(query, args.pid)?;
    let output = args.output_dir.ok_or("--output-dir is required")?;
    fs::create_dir_all(&output)?;
    // Avoid overwriting prior recaptures or discovering an unwritable output after sending input.
    let prefix = format!("replay-{}", nonce());
    let probe = output.join(format!("{prefix}.tmp"));
    fs::write(&probe, [])?;
    fs::remove_file(probe)?;
    markits::window_capture::activate_window(&window.id)?;
    let mut previous = 0;
    let mut captures = Vec::new();
    for event in &recording.events {
        let elapsed = event_time(event);
        if !args.shorten_waits {
            std::thread::sleep(Duration::from_millis(
                elapsed.saturating_sub(previous).min(args.max_wait_ms),
            ));
        }
        previous = elapsed;
        let current = cli_support::native_window(Some(&window.id), None)?;
        if !markits::window_capture::is_window_active(&window.id)? {
            return Err("Target window lost focus; replay stopped".into());
        }
        match event {
            RecordedEvent::Screenshot {
                include_cursor,
                pointer,
                ..
            } => {
                if let Some(pointer) = pointer {
                    let point = if let Some(target) = &pointer.target {
                        let elements = markits::semantic_target::observe_window(&window.id)?;
                        let e = target
                            .resolve(&elements)?
                            .ok_or("Recorded pointer target was not found")?;
                        let [x, y] = pointer.target_offset.unwrap_or([0.5, 0.5]);
                        (e.x + e.width * x, e.y + e.height * y)
                    } else {
                        relative_point(pointer.x, pointer.y, &recording, &current)?
                    };
                    markits::replay_input::send(rdev::EventType::MouseMove {
                        x: point.0,
                        y: point.1,
                    })?;
                    std::thread::sleep(Duration::from_millis(300));
                }
                let captured =
                    capture_native(&current, true, args.include_cursor || *include_cursor)?;
                let saved = serde_json::json!({"recording":recording,"window_id":window.id,"pid":args.pid.unwrap_or(0)});
                let png = metadata::embed_text_chunk(
                    &captured.raw_png,
                    markits::recording_metadata::OPERATIONS_KEY,
                    &serde_json::to_string(&saved)?,
                )?;
                let path = output.join(format!("{prefix}_{:03}.png", captures.len() + 1));
                fs::write(&path, png)?;
                captures.push(path);
            }
            RecordedEvent::Click { x, y, target, .. } => {
                let (x, y) = if let Some(target) = target {
                    let elements = markits::semantic_target::observe_window(&window.id)?;
                    let e = target
                        .resolve(&elements)?
                        .ok_or("Recorded click target was not found")?;
                    (e.x + e.width / 2.0, e.y + e.height / 2.0)
                } else {
                    relative_point(*x, *y, &recording, &current)?
                };
                if !markits::window_capture::pointer_targets_window(&window.id, x, y)? {
                    return Err("Click target is obscured by another window".into());
                }
                markits::replay_input::send(rdev::EventType::MouseMove { x, y })?;
                let pressed =
                    markits::replay_input::send(rdev::EventType::ButtonPress(rdev::Button::Left));
                let released =
                    markits::replay_input::send(rdev::EventType::ButtonRelease(rdev::Button::Left));
                pressed?;
                released?;
                std::thread::sleep(Duration::from_millis(1000));
            }
            RecordedEvent::Text { value, .. } => markits::replay_input::paste(value)?,
            RecordedEvent::Key { value, .. } => {
                markits::replay_input::chord(&markits::replay_input::parse_chord(value)?)?
            }
            RecordedEvent::Scroll { x, y, dx, dy, .. } => {
                let (x, y) = relative_point(*x, *y, &recording, &current)?;
                if !markits::window_capture::pointer_targets_window(&window.id, x, y)? {
                    return Err("Scroll target is obscured by another window".into());
                }
                markits::replay_input::send(rdev::EventType::MouseMove { x, y })?;
                markits::replay_input::send(rdev::EventType::Wheel {
                    delta_x: *dx,
                    delta_y: *dy,
                })?;
                std::thread::sleep(Duration::from_millis(300));
            }
        }
    }
    Ok(captures)
}

fn relative_point(
    x: f64,
    y: f64,
    recording: &OperationRecording,
    window: &markits::window_capture::WindowInfo,
) -> Result<(f64, f64)> {
    let [left, top, width, height] = recording.bounds;
    if !x.is_finite()
        || !y.is_finite()
        || x < left as f64
        || y < top as f64
        || x >= left as f64 + width as f64
        || y >= top as f64 + height as f64
    {
        return Err("Recorded point is outside target window".into());
    }
    Ok((
        window.x as f64 + (x - left as f64) * window.width as f64 / width as f64,
        window.y as f64 + (y - top as f64) * window.height as f64 / height as f64,
    ))
}
pub fn validate_recording(recording: &OperationRecording) -> Result<()> {
    if recording.version != 1 || recording.bounds[2] <= 0 || recording.bounds[3] <= 0 {
        return Err("Unsupported recording version or invalid bounds".into());
    }
    let dummy = markits::window_capture::WindowInfo {
        id: "".into(),
        title: "".into(),
        x: 0,
        y: 0,
        width: recording.bounds[2] as u32,
        height: recording.bounds[3] as u32,
    };
    let mut previous = 0;
    for event in &recording.events {
        let elapsed = event_time(event);
        if elapsed < previous {
            return Err("Recording events are not in time order".into());
        }
        previous = elapsed;
        match event {
            RecordedEvent::Key { value, .. } => {
                markits::replay_input::parse_chord(value)?;
            }
            RecordedEvent::Click {
                x, y, target: None, ..
            }
            | RecordedEvent::Scroll { x, y, .. } => {
                relative_point(*x, *y, recording, &dummy)?;
            }
            RecordedEvent::Screenshot {
                pointer: Some(p), ..
            } => {
                if let Some([x, y]) = p.target_offset {
                    if !x.is_finite()
                        || !y.is_finite()
                        || !(0.0..1.0).contains(&x)
                        || !(0.0..1.0).contains(&y)
                    {
                        return Err("Invalid pointer offset".into());
                    }
                }
                if p.target.is_none() {
                    relative_point(p.x, p.y, recording, &dummy)?;
                }
            }
            _ => {}
        }
    }
    Ok(())
}
