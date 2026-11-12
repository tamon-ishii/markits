use clap::Args;
use markits::{
    capture_recipe::{CaptureRecipe, EmbeddedScript, LaunchSpec, WindowBounds},
    cli_support::{self, Result},
    recording::{OperationRecording, RecordedEvent},
};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

#[derive(Debug, Args)]
pub struct CaptureRunArgs {
    /// Execute a previously extracted recipe JSON instead of specifying launch options
    #[arg(long)]
    pub recipe: Option<PathBuf>,
    /// Executable to start (arguments are passed literally, without a shell)
    #[arg(long, conflicts_with = "launch_script")]
    pub launch: Option<String>,
    /// Script file: its actual contents are embedded for later recapture
    #[arg(long)]
    pub launch_script: Option<PathBuf>,
    /// Interpreter for --launch-script (.ps1/.py/.sh inferred otherwise)
    #[arg(long, requires = "launch_script")]
    pub script_runner: Option<String>,
    /// One literal startup argument; repeat, using --arg=-option for leading dashes
    #[arg(long="arg",action=clap::ArgAction::Append,allow_hyphen_values=true)]
    pub args: Vec<String>,
    /// Working directory for executable/script (stored as an absolute path)
    #[arg(long)]
    pub cwd: Option<PathBuf>,
    /// Unique window title substring; required for scripts that launch another process
    #[arg(long)]
    pub window: Option<String>,
    /// Recorded operations JSON (recording.json or raw OperationRecording object)
    #[arg(long)]
    pub operations: Option<PathBuf>,
    /// Set window placement before operations: JSON [x,y,width,height] (Windows)
    #[arg(long)]
    pub window_bounds: Option<String>,
    /// Maximum time to wait for the target window, in milliseconds
    #[arg(long)]
    pub timeout_ms: Option<u64>,
    /// Delay after window readiness/placement, in milliseconds
    #[arg(long)]
    pub settle_ms: Option<u64>,
    /// Include the mouse pointer in captured images (Windows)
    #[arg(long)]
    pub include_cursor: bool,
    /// Destination image; additional capture points use name_002.ext, etc.
    #[arg(short, long, required_unless_present = "dry_run")]
    pub output: Option<PathBuf>,
    /// Print the full recipe without launching, operating or capturing
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(Debug, Args)]
pub struct RecaptureArgs {
    /// PNG, WebP or JPEG XL containing a capture recipe
    pub image: PathBuf,
    #[arg(short, long, required_unless_present = "dry_run")]
    pub output: Option<PathBuf>,
    /// Print the full saved script, arguments, cwd and operations without executing
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(Debug, Args)]
pub struct RecipeArgs {
    /// Image with an embedded recipe
    pub image: PathBuf,
    /// Save recipe JSON to a file; otherwise print to stdout. Never executes it.
    #[arg(short, long)]
    pub output: Option<PathBuf>,
}

pub fn recipe(args: RecipeArgs) -> Result<()> {
    let recipe = markits::capture_recipe::extract(&fs::read(args.image)?)?;
    let json = serde_json::to_string_pretty(&recipe)?;
    if let Some(path) = args.output {
        fs::write(path, format!("{json}\n"))?;
    } else {
        println!("{json}");
    }
    Ok(())
}

pub fn capture_run(args: CaptureRunArgs) -> Result<()> {
    let recipe = if let Some(path) = args.recipe {
        if args.launch.is_some()
            || args.launch_script.is_some()
            || !args.args.is_empty()
            || args.cwd.is_some()
            || args.window.is_some()
            || args.operations.is_some()
            || args.window_bounds.is_some()
            || args.timeout_ms.is_some()
            || args.settle_ms.is_some()
            || args.include_cursor
        {
            return Err("--recipe cannot be combined with launch, window, timing or operation options; edit the extracted JSON instead".into());
        }
        serde_json::from_slice::<CaptureRecipe>(&fs::read(path)?)?
    } else {
        if args.launch.is_none()
            && args.launch_script.is_none()
            && (!args.args.is_empty() || args.cwd.is_some())
        {
            return Err("--arg and --cwd require --launch or --launch-script".into());
        }
        let launch = if args.launch.is_some() || args.launch_script.is_some() {
            let cwd = fs::canonicalize(args.cwd.unwrap_or(std::env::current_dir()?))?;
            let script = args
                .launch_script
                .map(|path| -> Result<EmbeddedScript> {
                    Ok(EmbeddedScript {
                        name: path
                            .file_name()
                            .and_then(|s| s.to_str())
                            .ok_or("Invalid script filename")?
                            .into(),
                        content: fs::read_to_string(&path)?
                            .trim_start_matches('\u{feff}')
                            .to_string(),
                        runner: args.script_runner,
                    })
                })
                .transpose()?;
            let program = args.launch.map(|program| {
                if Path::new(&program).components().count() > 1
                    && !Path::new(&program).is_absolute()
                {
                    cwd.join(program).to_string_lossy().into_owned()
                } else {
                    program
                }
            });
            Some(LaunchSpec {
                program,
                script,
                args: args.args,
                cwd,
            })
        } else {
            None
        };
        let recording = args
            .operations
            .map(|path| -> Result<OperationRecording> {
                let value: serde_json::Value = serde_json::from_slice(&fs::read(path)?)?;
                Ok(serde_json::from_value(
                    value.get("recording").unwrap_or(&value).clone(),
                )?)
            })
            .transpose()?;
        let window_bounds = args
            .window_bounds
            .map(|s| -> Result<WindowBounds> {
                let [x, y, w, h]: [i64; 4] = serde_json::from_str(&s)?;
                Ok(WindowBounds {
                    x: i32::try_from(x)?,
                    y: i32::try_from(y)?,
                    width: u32::try_from(w)?,
                    height: u32::try_from(h)?,
                })
            })
            .transpose()?;
        CaptureRecipe {
            version: 1,
            launch,
            window: args.window,
            window_bounds,
            timeout_ms: args.timeout_ms.unwrap_or(30000),
            settle_ms: args.settle_ms.unwrap_or(1000),
            include_cursor: args.include_cursor,
            ui_depth: markits::ui_elements::capture_depth(),
            recording,
            capture_index: None,
        }
    };
    run(recipe, args.output, args.dry_run, None)
}

pub fn recapture(args: RecaptureArgs) -> Result<()> {
    let bytes = fs::read(&args.image)?;
    let recipe = markits::capture_recipe::extract(&bytes)?;
    run(
        recipe,
        args.output,
        args.dry_run,
        Some(cli_support::to_png(&bytes)?),
    )
}

fn run(
    mut recipe: CaptureRecipe,
    output: Option<PathBuf>,
    dry_run: bool,
    source: Option<Vec<u8>>,
) -> Result<()> {
    recipe.validate()?;
    if recipe.launch.as_ref().is_some_and(|l| l.script.is_some()) && recipe.window.is_none() {
        return Err("Script launch requires a --window title selector".into());
    }
    if let Some(recording) = &recipe.recording {
        crate::cli_commands::validate_recording(recording)?;
    }
    // Validate interpreter selection even during dry-run, but do not execute it.
    if let Some(script) = recipe.launch.as_ref().and_then(|l| l.script.as_ref()) {
        script_command(script)?;
    }
    if dry_run {
        println!("{}", serde_json::to_string_pretty(&recipe)?);
        return Ok(());
    }
    if !cfg!(target_os = "windows") && (recipe.window_bounds.is_some() || recipe.include_cursor) {
        return Err("Window placement and cursor capture currently require Windows".into());
    }
    let output = output.ok_or("--output is required")?;
    markits::image_export::format_for_path(&output.to_string_lossy())?;
    let parent = output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(parent)?;
    let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let work = parent.join(format!("markits-run-{nonce}"));
    fs::create_dir(&work)?;
    let work = fs::canonicalize(work)?;
    let _depth = markits::ui_elements::capture_depth_scope(recipe.ui_depth);
    let before = markits::window_capture::list_windows()?
        .into_iter()
        .map(|w| w.id)
        .collect::<Vec<_>>();
    let mut child = recipe
        .launch
        .as_ref()
        .map(|launch| start(launch, &work))
        .transpose()?;
    let window = wait_window(&recipe, &mut child, &before)?;
    if let Some(bounds) = recipe.window_bounds {
        place_window(&window.id, bounds)?;
    }
    std::thread::sleep(Duration::from_millis(recipe.settle_ms));
    let current = cli_support::native_window(Some(&window.id), None)?;
    // Persist a reusable title and current placement, never a stale HWND or PID.
    recipe.window = Some(current.title.clone());
    if cfg!(target_os = "windows") && recipe.window_bounds.is_none() {
        recipe.window_bounds = Some(outer_bounds(&current)?);
    }
    let recording = recipe.recording.get_or_insert_with(|| OperationRecording {
        version: 1,
        window: current.title.clone(),
        bounds: [
            current.x as i64,
            current.y as i64,
            current.width as i64,
            current.height as i64,
        ],
        events: vec![],
    });
    // An operation-only sequence gets one final capture automatically.
    if !recording
        .events
        .iter()
        .any(|e| matches!(e, RecordedEvent::Screenshot { .. }))
    {
        let elapsed_ms = recording
            .events
            .last()
            .map(event_time)
            .unwrap_or(0)
            .saturating_add(recipe.settle_ms);
        recording.events.push(RecordedEvent::Screenshot {
            elapsed_ms,
            include_cursor: recipe.include_cursor,
            pointer: None,
        });
    }
    let record_path = work.join("recording.json");
    fs::write(&record_path, serde_json::to_vec_pretty(&recording)?)?;
    let captures = crate::cli_commands::replay_capture(crate::cli_commands::ReplayArgs {
        input: record_path,
        window: Some(current.id),
        pid: None,
        output_dir: Some(work.clone()),
        dry_run: false,
        shorten_waits: false,
        max_wait_ms: 3600000,
        include_cursor: recipe.include_cursor,
    })?;
    let mut outputs = Vec::new();
    for (index, path) in captures.iter().enumerate() {
        if recipe
            .capture_index
            .is_some_and(|selected| selected != index)
        {
            continue;
        }
        let mut png = fs::read(path)?;
        if let Some(original) = source.as_deref() {
            png = reapply_edits(&png, original)?;
        }
        let mut image_recipe = recipe.clone();
        image_recipe.capture_index = Some(index);
        png = markits::capture_recipe::embed(&png, &image_recipe)?;
        let destination = if outputs.is_empty() {
            output.clone()
        } else {
            output.with_file_name(format!(
                "{}_{:03}.{}",
                output
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("capture"),
                index + 1,
                output.extension().and_then(|s| s.to_str()).unwrap_or("png")
            ))
        };
        cli_support::write_image(&destination, &png)?;
        outputs.push(destination);
    }
    println!("{}", serde_json::to_string_pretty(&outputs)?);
    Ok(())
}

fn script_command(script: &EmbeddedScript) -> Result<(String, Vec<String>)> {
    let extension = Path::new(&script.name)
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let (default, prefix) = match extension.as_str() {
        "ps1" => (
            if cfg!(target_os = "windows") {
                "powershell.exe"
            } else {
                "pwsh"
            },
            vec!["-NoProfile".into(), "-File".into()],
        ),
        "py" => ("python", vec![]),
        "sh" => ("sh", vec![]),
        _ if script.runner.is_some() => ("", vec![]),
        _ => return Err("Unknown script extension; specify --script-runner".into()),
    };
    Ok((
        script.runner.clone().unwrap_or_else(|| default.into()),
        prefix,
    ))
}
fn start(launch: &LaunchSpec, work: &Path) -> Result<Child> {
    if !launch.cwd.is_dir() {
        return Err(format!("Working directory does not exist: {}", launch.cwd.display()).into());
    }
    let mut command = if let Some(script) = &launch.script {
        let (program, prefix) = script_command(script)?;
        let scripts = work.join("script");
        fs::create_dir(&scripts)?;
        let path = scripts.join(&script.name);
        let content = if script.name.to_ascii_lowercase().ends_with(".ps1") {
            format!("\u{feff}{}", script.content)
        } else {
            script.content.clone()
        };
        fs::write(&path, content)?;
        let mut command = Command::new(program);
        command.args(prefix).arg(path);
        command
    } else {
        Command::new(launch.program.as_ref().ok_or("Missing launch program")?)
    };
    command
        .args(&launch.args)
        .current_dir(&launch.cwd)
        .stdin(Stdio::null())
        .stdout(fs::File::create(work.join("launch.stdout.log"))?)
        .stderr(fs::File::create(work.join("launch.stderr.log"))?);
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    Ok(command.spawn()?)
}

fn wait_window(
    recipe: &CaptureRecipe,
    child: &mut Option<Child>,
    before: &[String],
) -> Result<markits::window_capture::WindowInfo> {
    let deadline = Instant::now() + Duration::from_millis(recipe.timeout_ms);
    loop {
        let windows = markits::window_capture::list_windows()?;
        let matching = windows
            .into_iter()
            .filter(|w| {
                recipe
                    .window
                    .as_ref()
                    .is_none_or(|title| w.title.to_lowercase().contains(&title.to_lowercase()))
            })
            .collect::<Vec<_>>();
        let owned = if let Some(child) = child.as_ref() {
            matching
                .iter()
                .filter(|w| {
                    markits::semantic_target::window_context(&w.id)
                        .is_ok_and(|c| c.pid == Some(child.id()))
                })
                .cloned()
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        let fresh = matching
            .iter()
            .filter(|w| !before.contains(&w.id))
            .cloned()
            .collect::<Vec<_>>();
        let candidates = if !owned.is_empty() {
            &owned
        } else if child.is_some() {
            if recipe.window.is_some() {
                &fresh
            } else {
                &owned
            }
        } else {
            &matching
        };
        if candidates.len() == 1 {
            return Ok(candidates[0].clone());
        }
        if candidates.len() > 1 {
            return Err("Multiple target windows matched; specify a unique --window title".into());
        }
        if let Some(child) = child.as_mut() {
            if let Some(exit) = child.try_wait()? {
                if !exit.success() {
                    return Err(format!(
                        "Launch failed ({exit}); inspect launch.stderr.log in the run directory"
                    )
                    .into());
                }
            }
        }
        if Instant::now() >= deadline {
            return Err("Timed out waiting for a unique target window; scripts that launch child processes should use --window".into());
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

fn place_window(id: &str, bounds: WindowBounds) -> Result<()> {
    #[cfg(target_os = "windows")]
    {
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            SW_RESTORE, SWP_NOACTIVATE, SWP_NOZORDER, SetWindowPos, ShowWindow,
        };
        let handle = usize::from_str_radix(id.trim_start_matches("0x"), 16)?
            as windows_sys::Win32::Foundation::HWND;
        unsafe {
            ShowWindow(handle, SW_RESTORE);
            if SetWindowPos(
                handle,
                std::ptr::null_mut(),
                bounds.x,
                bounds.y,
                bounds.width as i32,
                bounds.height as i32,
                SWP_NOZORDER | SWP_NOACTIVATE,
            ) == 0
            {
                return Err(std::io::Error::last_os_error().into());
            }
        }
        Ok(())
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (id, bounds);
        Err("Window placement currently requires Windows".into())
    }
}

fn outer_bounds(window: &markits::window_capture::WindowInfo) -> Result<WindowBounds> {
    #[cfg(target_os = "windows")]
    {
        use windows_sys::Win32::{
            Foundation::{HWND, RECT},
            UI::WindowsAndMessaging::GetWindowRect,
        };
        let handle = usize::from_str_radix(window.id.trim_start_matches("0x"), 16)? as HWND;
        let mut rect = RECT {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
        if unsafe { GetWindowRect(handle, &mut rect) } == 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        Ok(WindowBounds {
            x: rect.left,
            y: rect.top,
            width: (rect.right - rect.left) as u32,
            height: (rect.bottom - rect.top) as u32,
        })
    }
    #[cfg(not(target_os = "windows"))]
    {
        Ok(WindowBounds {
            x: window.x,
            y: window.y,
            width: window.width,
            height: window.height,
        })
    }
}

fn reapply_edits(fresh: &[u8], original: &[u8]) -> Result<Vec<u8>> {
    let loaded = markits::metadata::load_image_with_metadata(original)?;
    let mut background = fresh.to_vec();
    if let Some(crop) = loaded.crop_info.as_ref() {
        let dimensions = image::load_from_memory(original)?;
        background = cli_support::crop_png(
            &background,
            crop.offset_x as u32,
            crop.offset_y as u32,
            dimensions.width(),
            dimensions.height(),
        )?;
        let ui = markits::raster::extract_png_uimap(fresh).unwrap_or_default();
        background = markits::raster::embed_png_uimap(
            &background,
            &markits::raster::filter_ui_elements_for_crop(
                &ui,
                crop.offset_x,
                crop.offset_y,
                dimensions.width() as f64,
                dimensions.height() as f64,
            ),
        )?;
    }
    let mut png = if let Some(json) = loaded.annotations_json {
        cli_support::render(&json, &background, None, true)?
    } else {
        background
    };
    if let Some(crop) = loaded.crop_info {
        png = markits::metadata::embed_crop_info(&png, &crop)?;
        png = markits::metadata::embed_text_chunk(
            &png,
            "markits:base_image",
            &cli_support::data_url(fresh),
        )?;
    }
    Ok(markits::recording_metadata::copy_metadata(&png, fresh)?)
}
fn event_time(event: &RecordedEvent) -> u64 {
    match event {
        RecordedEvent::Click { elapsed_ms, .. }
        | RecordedEvent::Text { elapsed_ms, .. }
        | RecordedEvent::Key { elapsed_ms, .. }
        | RecordedEvent::Scroll { elapsed_ms, .. }
        | RecordedEvent::Screenshot { elapsed_ms, .. } => *elapsed_ms,
    }
}
