#![cfg(target_os = "linux")]

use std::{
    process::Command,
    sync::{Arc, Mutex, OnceLock},
    time::{Duration, Instant},
};

use chrono::Utc;
use serde_json::Value;

use super::{
    privacy::PrivacyFilter,
    types::{CapturedContent, ContentType},
};

/// Process execution is deliberately kept here rather than relying on an
/// unbounded `wait_with_output()`: it gives capture a deadline and output cap.
/// The helper also puts children
/// in their own process group so a shell fixture (or a screenshot helper that
/// forks) cannot survive the parent being killed.
mod bounded_process {
    use std::{
        io::{self, Read, Write},
        os::fd::AsRawFd,
        process::{Child, Command, ExitStatus, Stdio},
        sync::{atomic::{AtomicBool, Ordering}, Arc},
        thread,
        time::{Duration, Instant},
    };

    #[derive(Clone, Copy)]
    pub(super) struct Limits {
        pub timeout: Duration,
        pub stdout_bytes: usize,
        pub stderr_bytes: usize,
    }

    impl Limits {
        pub(super) const fn new(
            timeout: Duration,
            stdout_bytes: usize,
            stderr_bytes: usize,
        ) -> Self {
            Self { timeout, stdout_bytes, stderr_bytes }
        }
    }

    pub(super) struct Output {
        pub status: ExitStatus,
        pub stdout: Vec<u8>,
        pub stderr: Vec<u8>,
        pub timed_out: bool,
        pub output_limited: bool,
    }

    struct DrainResult {
        bytes: Vec<u8>,
        limited: bool,
        errored: bool,
    }

    // Linux-only file: these are the small POSIX calls needed to terminate a
    // process group without adding a libc dependency to the desktop app.
    unsafe extern "C" {
        fn setpgid(pid: i32, pgid: i32) -> i32;
        fn kill(pid: i32, signal: i32) -> i32;
        fn fcntl(fd: i32, command: i32, ...) -> i32;
    }

    fn make_process_group(command: &mut Command) {
        use std::os::unix::process::CommandExt;
        // SAFETY: this closure runs in the child between fork and exec and
        // only calls the async-signal-safe setpgid syscall.
        unsafe {
            command.pre_exec(|| {
                if setpgid(0, 0) == 0 {
                    Ok(())
                } else {
                    Err(io::Error::last_os_error())
                }
            });
        }
    }

    fn kill_and_reap(child: &mut Child) -> io::Result<ExitStatus> {
        let pid = child.id() as i32;
        // SAFETY: pid is the live child process id returned by std::process.
        // A negative pid targets only the process group created above.
        unsafe {
            let _ = kill(-pid, 9);
        }
        // Keep this fallback for a process that exited before setpgid/kill,
        // and for platforms whose kernel rejects the group signal.
        let _ = child.kill();
        child.wait()
    }

    fn nonblocking(pipe: &impl AsRawFd) -> io::Result<()> {
        // Linux F_GETFL=3, F_SETFL=4, O_NONBLOCK=0x800. We own these
        // descriptors; no other code depends on their blocking mode.
        unsafe {
            let flags = fcntl(pipe.as_raw_fd(), 3);
            if flags < 0 || fcntl(pipe.as_raw_fd(), 4, flags | 0x800) < 0 {
                Err(io::Error::last_os_error())
            } else {
                Ok(())
            }
        }
    }

    fn drain<R: Read>(
        mut reader: R,
        cap: usize,
        signal: Arc<AtomicBool>,
        stop: Arc<AtomicBool>,
        deadline: Instant,
    ) -> DrainResult {
        let mut bytes = Vec::with_capacity(cap.min(8192));
        let mut buffer = [0_u8; 8192];
        let mut limited = false;
        let mut errored = false;
        loop {
            if stop.load(Ordering::Acquire) || Instant::now() >= deadline {
                break;
            }
            match reader.read(&mut buffer) {
                Ok(0) => break,
                Ok(count) => {
                    let room = cap.saturating_sub(bytes.len());
                    if count > room {
                        limited = true;
                        signal.store(true, Ordering::Release);
                        bytes.extend_from_slice(&buffer[..room]);
                        break;
                    }
                    bytes.extend_from_slice(&buffer[..count.min(room)]);
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(2));
                }
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(_) => {
                    errored = true;
                    signal.store(true, Ordering::Release);
                    break;
                }
            }
        }
        DrainResult { bytes, limited, errored }
    }

    /// Run a command with bounded output and a hard deadline. On every error,
    /// timeout, and output overflow path the child is killed and reaped before
    /// this function returns. Reader threads are joined on every path too.
    pub(super) fn run(
        mut command: Command,
        input: Option<Vec<u8>>,
        limits: Limits,
    ) -> io::Result<Output> {
        command.stdout(Stdio::piped()).stderr(Stdio::piped());
        if input.is_some() {
            command.stdin(Stdio::piped());
        } else {
            command.stdin(Stdio::null());
        }
        make_process_group(&mut command);
        let deadline = Instant::now() + limits.timeout;
        let mut child = command.spawn()?;

        let stdout = match child.stdout.take() {
            Some(pipe) => pipe,
            None => {
                let _ = kill_and_reap(&mut child);
                return Err(io::Error::new(io::ErrorKind::BrokenPipe, "stdout was not piped"));
            }
        };
        let stderr = match child.stderr.take() {
            Some(pipe) => pipe,
            None => {
                let _ = kill_and_reap(&mut child);
                return Err(io::Error::new(io::ErrorKind::BrokenPipe, "stderr was not piped"));
            }
        };
        if let Err(error) = nonblocking(&stdout)
            .and_then(|_| nonblocking(&stderr))
            .and_then(|_| child.stdin.as_ref().map_or(Ok(()), nonblocking))
        {
            let _ = kill_and_reap(&mut child);
            return Err(error);
        }

        let output_limited = Arc::new(AtomicBool::new(false));
        let reader_error = Arc::new(AtomicBool::new(false));
        let stop = Arc::new(AtomicBool::new(false));
        let stdout_signal = Arc::clone(&output_limited);
        let stdout_error = Arc::clone(&reader_error);
        let stdout_stop = Arc::clone(&stop);
        let stdout_thread = thread::spawn(move || {
            let result = drain(stdout, limits.stdout_bytes, stdout_signal, stdout_stop, deadline);
            if result.errored {
                stdout_error.store(true, Ordering::Release);
            }
            result
        });
        let stderr_signal = Arc::clone(&output_limited);
        let stderr_error = Arc::clone(&reader_error);
        let stderr_stop = Arc::clone(&stop);
        let stderr_thread = thread::spawn(move || {
            let result = drain(stderr, limits.stderr_bytes, stderr_signal, stderr_stop, deadline);
            if result.errored {
                stderr_error.store(true, Ordering::Release);
            }
            result
        });

        let stdin_error = Arc::new(AtomicBool::new(false));
        let stdin_thread = if let Some(input) = input {
            let pipe = match child.stdin.take() {
                Some(pipe) => pipe,
                None => {
                    let _ = kill_and_reap(&mut child);
                    let _ = stdout_thread.join();
                    let _ = stderr_thread.join();
                    return Err(io::Error::new(io::ErrorKind::BrokenPipe, "stdin was not piped"));
                }
            };
            let error = Arc::clone(&stdin_error);
            let writer_stop = Arc::clone(&stop);
            Some(thread::spawn(move || {
                let mut pipe = pipe;
                let mut written = 0;
                while written < input.len()
                    && !writer_stop.load(Ordering::Acquire)
                    && Instant::now() < deadline
                {
                    match pipe.write(&input[written..(written + 8192).min(input.len())]) {
                        Ok(0) => {
                            error.store(true, Ordering::Release);
                            break;
                        }
                        Ok(count) => written += count,
                        Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                            thread::sleep(Duration::from_millis(2));
                        }
                        Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                        Err(_) => {
                            error.store(true, Ordering::Release);
                            break;
                        }
                    }
                }
                // Closing stdin is required for tesseract to finish reading.
            }))
        } else {
            None
        };

        let mut timed_out = false;
        let mut status = None;
        loop {
            match child.try_wait() {
                Ok(Some(exit)) => {
                    status = Some(exit);
                    // A parent can exit while a descendant retains either
                    // pipe. Kill the whole group before joining readers; this
                    // prevents a hidden `sleep`/shell child from making the
                    // join unbounded.
                    status = kill_and_reap(&mut child).ok().or(status);
                    break;
                }
                Ok(None) => {
                    if output_limited.load(Ordering::Acquire)
                        || reader_error.load(Ordering::Acquire)
                        || stdin_error.load(Ordering::Acquire)
                    {
                        break;
                    }
                    if Instant::now() >= deadline {
                        timed_out = true;
                        break;
                    }
                    thread::sleep(Duration::from_millis(5));
                }
                Err(_) => break,
            }
        }

        if status.is_none() {
            // This is also used for pipe/read/try_wait errors. In all cases,
            // reap first, then join the drainers below.
            status = kill_and_reap(&mut child).ok();
            stop.store(true, Ordering::Release);
        }
        // Nonblocking pipe loops have their own deadline, so even a descendant
        // that escapes the process group and retains a pipe cannot hang joins.
        // Join *all* workers before propagating any worker panic.
        let stdout_join = stdout_thread.join();
        let stderr_join = stderr_thread.join();
        let stdin_join = stdin_thread.map(|thread| thread.join()).transpose();
        let stdout_result = stdout_join.map_err(|_| io::Error::other("stdout reader panicked"))?;
        let stderr_result = stderr_join.map_err(|_| io::Error::other("stderr reader panicked"))?;
        stdin_join.map_err(|_| io::Error::other("stdin writer panicked"))?;
        timed_out |= Instant::now() >= deadline;
        let status = status.ok_or_else(|| io::Error::other("could not reap child"))?;
        if stdout_result.errored || stderr_result.errored || stdin_error.load(Ordering::Acquire) {
            return Err(io::Error::other("child pipe I/O failed"));
        }
        Ok(Output {
            status,
            stdout: stdout_result.bytes,
            stderr: stderr_result.bytes,
            timed_out,
            output_limited: output_limited.load(Ordering::Acquire)
                || stdout_result.limited
                || stderr_result.limited,
        })
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use std::process::Command;

        fn limits(timeout: Duration, output: usize) -> Limits {
            Limits::new(timeout, output, output)
        }

        #[test]
        fn harmless_child_succeeds_and_drains_both_pipes() {
            let output = run(
                {
                    let mut command = Command::new("sh");
                    command.args(["-c", "printf success; printf warning >&2"]);
                    command
                },
                None,
                limits(Duration::from_secs(2), 1024),
            )
            .expect("child should succeed");
            assert!(output.status.success());
            assert_eq!(output.stdout, b"success");
            assert_eq!(output.stderr, b"warning");
        }

        #[test]
        fn output_limit_kills_and_reaps_child() {
            let output = run(
                {
                    let mut command = Command::new("sh");
                    command.args(["-c", "yes x"]);
                    command
                },
                None,
                limits(Duration::from_secs(2), 4096),
            )
            .expect("limited child should be reaped");
            assert!(output.output_limited);
            assert!(output.stdout.len() <= 4096);
        }

        #[test]
        fn timeout_kills_process_group_and_reaps_fixture() {
            let started = Instant::now();
            let output = run(
                {
                    let mut command = Command::new("sh");
                    command.args(["-c", "sleep 30"]);
                    command
                },
                None,
                limits(Duration::from_millis(50), 1024),
            )
            .expect("timed out child should be reaped");
            assert!(output.timed_out);
            assert!(started.elapsed() < Duration::from_secs(3));
        }

        #[test]
        fn input_is_closed_after_successful_write() {
            let output = run(
                {
                    let mut command = Command::new("sh");
                    command.args(["-c", "cat"]);
                    command
                },
                Some(b"in-memory".to_vec()),
                limits(Duration::from_secs(2), 1024),
            )
            .expect("stdin fixture should finish");
            assert_eq!(output.stdout, b"in-memory");
        }
    }
}

pub struct LinuxWindow {
    pub app_name: String,
    pub window_title: String,
    pub platform_handle: isize,
    pub pid: i32,
}

pub struct LinuxReader {
    privacy: Arc<PrivacyFilter>,
    ocr_enabled: bool,
}

const MAX_IMAGE_EDGE: i64 = 1920;
const MAX_SCREENSHOT_BYTES: usize = 16 * 1024 * 1024;
const MAX_OCR_IMAGE_BYTES: usize = 12 * 1024 * 1024;
const MAX_TEXT_BYTES: usize = 256 * 1024;
const PROCESS_TIMEOUT: Duration = Duration::from_secs(5);

impl LinuxReader {
    pub fn new(privacy: Arc<PrivacyFilter>) -> Self {
        Self {
            privacy,
            ocr_enabled: true,
        }
    }

    pub fn with_ocr(mut self, enabled: bool) -> Self {
        self.ocr_enabled = enabled;
        self
    }

    pub fn active_window() -> Option<LinuxWindow> {
        hyprland_active_window().or_else(x11_active_window)
    }

    pub fn read_window_content(
        &self,
        platform_handle: isize,
        pid: i32,
        app_name: &str,
        window_title: &str,
        task_id: String,
    ) -> Option<CapturedContent> {
        if !self.privacy.should_capture_window(app_name, window_title) || !self.ocr_enabled {
            return None;
        }

        if let Some(reason) = linux_ocr_unavailable_reason(platform_handle) {
            eprintln!("[taskflow:capture] Linux OCR unavailable: {reason}");
            return None;
        }

        let text = (if platform_handle != 0 {
            self.ocr_x11_window(platform_handle)
        } else {
            self.ocr_hyprland_window(pid, window_title)
        })?;

        let text = self
            .privacy
            .sanitize_content(&truncate(&text, 5000))
            .trim()
            .to_string();
        if text.is_empty() {
            return None;
        }

        Some(CapturedContent {
            content_type: content_type(app_name),
            app_name: app_name.to_string(),
            window_title: window_title.to_string(),
            text: Some(text),
            url: None,
            timestamp: Utc::now().to_rfc3339(),
            task_id,
            capture_method: "linux_ocr".to_string(),
        })
    }

    fn ocr_hyprland_window(&self, pid: i32, window_title: &str) -> Option<String> {
        let mut command = Command::new("hyprctl");
        command.args(["clients", "-j"]);
        let output = bounded_process::run(
            command,
            None,
            bounded_process::Limits::new(PROCESS_TIMEOUT, 2 * 1024 * 1024, 64 * 1024),
        )
        .ok()?;
        if !output.status.success() || output.timed_out || output.output_limited {
            return None;
        }

        let clients: Vec<Value> = serde_json::from_slice(&output.stdout).ok()?;
        let window = clients.into_iter().find(|window| {
            window.get("pid").and_then(Value::as_i64) == Some(i64::from(pid))
                && window.get("title").and_then(Value::as_str) == Some(window_title)
        })?;
        let (x, y, width, height) = parse_hyprland_geometry(&window)?;

        let geometry = format!("{x},{y} {width}x{height}");
        let scale = if width.max(height) > MAX_IMAGE_EDGE {
            (MAX_IMAGE_EDGE - 1) as f64 / width.max(height) as f64
        } else {
            1.0
        };
        let mut command = Command::new("grim");
        command
            .args(["-g", &geometry, "-s", &format!("{scale:.8}"), "-l", "1", "-"]);
        let image = capture_image(command)?;
        self.ocr_image(image)
    }

    fn ocr_x11_window(&self, window_id: isize) -> Option<String> {
        let id = window_id.to_string();
        let mut import = Command::new("import");
        // No maim fallback: it cannot bound dimensions before emitting the
        // native-size image. Disable ImageMagick's disk pixel cache and limit
        // its resources; reduction and PNG encoding occur entirely in RAM.
        import.args([
            "-limit", "memory", "64MiB",
            "-limit", "map", "0",
            "-limit", "disk", "0",
            "-limit", "thread", "1",
            "-limit", "width", "16384",
            "-limit", "height", "16384",
            "-window", &id,
            "-resize", "1920x1920>", "png:-",
        ]);
        let image = capture_image(import)?;
        self.ocr_image(image)
    }

    fn ocr_image(&self, image: Vec<u8>) -> Option<String> {
        if image.is_empty() || image.len() > MAX_OCR_IMAGE_BYTES || !bounded_png_dimensions(&image) {
            return None;
        }
        let mut tesseract = Command::new("tesseract");
        tesseract
            .env("OMP_THREAD_LIMIT", "1")
            .env("OMP_NUM_THREADS", "1")
            .args(["stdin", "stdout"]);
        let output = bounded_process::run(
            tesseract,
            Some(image),
            bounded_process::Limits::new(Duration::from_secs(15), MAX_TEXT_BYTES, 64 * 1024),
        )
        .ok()?;
        if !output.status.success() || output.timed_out || output.output_limited {
            return None;
        }
        let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
        (!text.is_empty()).then_some(text)
    }
}

fn capture_image(command: Command) -> Option<Vec<u8>> {
    let output = bounded_process::run(
        command,
        None,
        bounded_process::Limits::new(Duration::from_secs(8), MAX_SCREENSHOT_BYTES, 64 * 1024),
    )
    .ok()?;
    if !output.status.success() || output.timed_out || output.output_limited || output.stdout.is_empty() {
        return None;
    }
    Some(output.stdout)
}

fn bounded_png_dimensions(image: &[u8]) -> bool {
    // All capture commands emit PNG. A tiny compressed image can describe
    // enormous dimensions, so a byte cap alone is not enough for OCR.
    if image.len() < 33
        || &image[..8] != b"\x89PNG\r\n\x1a\n"
        || image[8..12] != 13_u32.to_be_bytes()
        || &image[12..16] != b"IHDR"
    {
        return false;
    }
    let width = u32::from_be_bytes(image[16..20].try_into().unwrap());
    let height = u32::from_be_bytes(image[20..24].try_into().unwrap());
    width > 0 && height > 0 && i64::from(width.max(height)) <= MAX_IMAGE_EDGE
}

#[derive(Clone)]
struct CachedCapability {
    checked_at: Instant,
    unavailable: Option<String>,
}

static OCR_CAPABILITY_CACHE: OnceLock<Mutex<[Option<CachedCapability>; 2]>> = OnceLock::new();

fn linux_ocr_unavailable_reason(platform_handle: isize) -> Option<String> {
    let index = usize::from(platform_handle != 0);
    let cache = OCR_CAPABILITY_CACHE.get_or_init(|| Mutex::new([None, None]));
    let now = Instant::now();
    {
        let guard = cache.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(entry) = &guard[index] {
            let ttl = if entry.unavailable.is_some() {
                Duration::from_secs(10)
            } else {
                Duration::from_secs(60)
            };
            if now.duration_since(entry.checked_at) < ttl {
                return entry.unavailable.clone();
            }
        }
    }

    let reason = check_linux_ocr_capability(platform_handle);
    let mut guard = cache.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    guard[index] = Some(CachedCapability { checked_at: now, unavailable: reason.clone() });
    reason
}

fn check_linux_ocr_capability(platform_handle: isize) -> Option<String> {
    let mut languages_command = Command::new("tesseract");
    languages_command.arg("--list-langs");
    let languages = match bounded_process::run(
        languages_command,
        None,
        bounded_process::Limits::new(Duration::from_secs(3), 128 * 1024, 32 * 1024),
    ) {
        Ok(output) if output.status.success() && !output.timed_out && !output.output_limited => output,
        Ok(output) => {
            let error = String::from_utf8_lossy(&output.stderr).trim().to_string();
            return Some(if error.is_empty() {
                "tesseract could not list languages".to_string()
            } else {
                format!("tesseract could not list languages: {error}")
            });
        }
        Err(error) => return Some(format!("tesseract is not installed or unavailable: {error}")),
    };
    if !String::from_utf8_lossy(&languages.stdout)
        .lines()
        .any(|language| language.trim() == "eng")
    {
        return Some("English OCR data is missing; install tesseract-data-eng".to_string());
    }

    let tool_available = if platform_handle == 0 {
        let mut command = Command::new("grim");
        command.arg("-h");
        bounded_process::run(
            command,
            None,
            bounded_process::Limits::new(Duration::from_secs(2), 32 * 1024, 32 * 1024),
        )
        .is_ok_and(|output| !output.timed_out && !output.output_limited)
    } else {
        let mut command = Command::new("import");
        command.arg("-version");
        bounded_process::run(
            command,
            None,
            bounded_process::Limits::new(Duration::from_secs(2), 32 * 1024, 32 * 1024),
        )
        .is_ok_and(|output| output.status.success() && !output.timed_out && !output.output_limited)
    };
    if !tool_available {
        return Some(if platform_handle == 0 {
            "grim is not installed".to_string()
        } else {
            "ImageMagick import is not installed or unavailable (maim-only capture is unsafe)".to_string()
        });
    }
    None
}

fn hyprland_active_window() -> Option<LinuxWindow> {
    if std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_none() {
        return None;
    }

    let mut command = Command::new("hyprctl");
    command.args(["activewindow", "-j"]);
    let output = bounded_process::run(
        command,
        None,
        bounded_process::Limits::new(Duration::from_secs(2), 512 * 1024, 64 * 1024),
    )
    .ok()?;
    if !output.status.success() || output.timed_out || output.output_limited {
        return None;
    }

    let window: Value = serde_json::from_slice(&output.stdout).ok()?;
    let app_name = ["initialClass", "class"]
        .into_iter()
        .find_map(|field| window.get(field).and_then(Value::as_str))
        .map(str::trim)
        .filter(|app_name| !app_name.is_empty())?
        .to_string();
    if app_name.is_empty() {
        return None;
    }

    Some(LinuxWindow {
        app_name,
        window_title: window
            .get("title")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        platform_handle: 0,
        pid: window
            .get("pid")
            .and_then(Value::as_i64)
            .and_then(|pid| i32::try_from(pid).ok())
            .unwrap_or_default(),
    })
}

fn x11_active_window() -> Option<LinuxWindow> {
    let mut active_command = Command::new("xprop");
    active_command.args(["-root", "-notype", "_NET_ACTIVE_WINDOW"]);
    let output = bounded_process::run(
        active_command,
        None,
        bounded_process::Limits::new(Duration::from_secs(2), 64 * 1024, 32 * 1024),
    )
    .ok()?;
    if !output.status.success() || output.timed_out || output.output_limited {
        return None;
    }

    let active = String::from_utf8_lossy(&output.stdout);
    let window_id = active.lines().find_map(parse_x11_window_id).or_else(|| {
        active
            .trim()
            .split_whitespace()
            .last()
            .and_then(|value| parse_number(value))
    })?;

    let mut properties_command = Command::new("xprop");
    properties_command
        .arg("-id")
        .arg(window_id.to_string())
        .args(["-notype", "WM_CLASS", "_NET_WM_NAME", "_NET_WM_PID"]);
    let output = bounded_process::run(
        properties_command,
        None,
        bounded_process::Limits::new(Duration::from_secs(2), 64 * 1024, 32 * 1024),
    )
    .ok()?;
    if !output.status.success() || output.timed_out || output.output_limited {
        return None;
    }

    let properties = String::from_utf8_lossy(&output.stdout);
    let app_name = properties
        .lines()
        .find_map(parse_x11_wm_class)
        .unwrap_or_else(|| "Unknown".to_string());
    let window_title = properties
        .lines()
        .find(|line| line.starts_with("_NET_WM_NAME"))
        .and_then(parse_x11_property_value)
        .unwrap_or_default();
    let pid = properties
        .lines()
        .find_map(parse_x11_pid)
        .unwrap_or_default();

    Some(LinuxWindow {
        app_name,
        window_title,
        platform_handle: window_id,
        pid,
    })
}

fn parse_hyprland_geometry(window: &Value) -> Option<(i64, i64, i64, i64)> {
    let x = window
        .pointer("/at/0")
        .or_else(|| window.pointer("/at/x"))
        .and_then(Value::as_i64)?;
    let y = window
        .pointer("/at/1")
        .or_else(|| window.pointer("/at/y"))
        .and_then(Value::as_i64)?;
    let width = window
        .pointer("/size/0")
        .or_else(|| window.pointer("/size/width"))
        .and_then(Value::as_i64)?;
    let height = window
        .pointer("/size/1")
        .or_else(|| window.pointer("/size/height"))
        .and_then(Value::as_i64)?;
    if width <= 0 || height <= 0 || width > 16384 || height > 16384
        || i32::try_from(x).is_err() || i32::try_from(y).is_err()
        || x.checked_add(width).and_then(|edge| i32::try_from(edge).ok()).is_none()
        || y.checked_add(height).and_then(|edge| i32::try_from(edge).ok()).is_none()
    {
        return None;
    }
    Some((x, y, width, height))
}

fn parse_x11_window_id(line: &str) -> Option<isize> {
    let value = line.split('#').nth(1)?.split(',').next()?.trim();
    parse_x11_number(value)
}

fn parse_x11_property_value(line: &str) -> Option<String> {
    let value = line
        .split_once('=')
        .or_else(|| line.split_once(':'))?
        .1
        .trim();
    let unquoted = value
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'));
    Some(unquoted.unwrap_or(value).trim().to_string())
}

fn parse_x11_wm_class(line: &str) -> Option<String> {
    let value = parse_x11_property_value(line)?;
    value
        .rsplit(',')
        .next()
        .map(|class| class.trim().trim_matches('"').to_string())
        .filter(|class| !class.is_empty())
}

fn parse_x11_pid(line: &str) -> Option<i32> {
    let value = line
        .split_once('=')
        .or_else(|| line.split_once(':'))?
        .1
        .trim();
    value.parse().ok()
}

fn parse_number(value: &str) -> Option<isize> {
    parse_x11_number(value.trim())
}

fn parse_x11_number(value: &str) -> Option<isize> {
    let parsed = if let Some(hex) = value.strip_prefix("0x") {
        i64::from_str_radix(hex, 16).ok()?
    } else {
        value.parse::<i64>().ok()?
    };
    isize::try_from(parsed).ok()
}

fn content_type(app_name: &str) -> ContentType {
    let app = app_name.to_lowercase();
    if [
        "chrome", "firefox", "safari", "edge", "brave", "arc", "opera",
    ]
    .iter()
    .any(|name| app.contains(name))
    {
        ContentType::BrowserContent
    } else if [
        "code", "vscodium", "pycharm", "intellij", "webstorm", "cursor", "sublime",
    ]
    .iter()
    .any(|name| app.contains(name))
    {
        ContentType::CodeContent
    } else if [
        "terminal",
        "iterm",
        "wezterm",
        "alacritty",
        "kitty",
        "foot",
        "konsole",
    ]
    .iter()
    .any(|name| app.contains(name))
    {
        ContentType::TerminalContent
    } else {
        ContentType::GenericContent
    }
}

fn truncate(value: &str, max_chars: usize) -> String {
    value.chars().take(max_chars).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_x11_active_window_id() {
        assert_eq!(
            parse_x11_window_id("_NET_ACTIVE_WINDOW: window id # 0x2c00007, 0x0"),
            Some(0x2c00007)
        );
    }

    #[test]
    fn parses_x11_window_metadata() {
        let class = "WM_CLASS: Navigator, \"firefox\"";
        let name = "_NET_WM_NAME: \"TaskFlow - Mozilla Firefox\"";
        let pid = "_NET_WM_PID: 12345";

        assert_eq!(parse_x11_wm_class(class), Some("firefox".to_string()));
        assert_eq!(
            parse_x11_property_value(name),
            Some("TaskFlow - Mozilla Firefox".to_string())
        );
        assert_eq!(parse_x11_pid(pid), Some(12345));
    }

    #[test]
    fn classifies_common_linux_apps() {
        assert!(matches!(
            content_type("Firefox"),
            ContentType::BrowserContent
        ));
        assert!(matches!(content_type("code"), ContentType::CodeContent));
        assert!(matches!(content_type("foot"), ContentType::TerminalContent));
        assert!(matches!(
            content_type("obsidian"),
            ContentType::GenericContent
        ));
    }

    #[test]
    fn parses_hyprland_window_geometry() {
        let json = serde_json::json!({
            "at": [3258, 21],
            "size": [908, 1038]
        });
        assert_eq!(
            parse_hyprland_geometry(&json),
            Some((3258, 21, 908, 1038))
        );

        let fallback_obj = serde_json::json!({
            "at": {"x": 100, "y": 200},
            "size": {"width": 800, "height": 600}
        });
        assert_eq!(
            parse_hyprland_geometry(&fallback_obj),
            Some((100, 200, 800, 600))
        );

        let invalid_size = serde_json::json!({
            "at": [100, 200],
            "size": [0, 500]
        });
        assert_eq!(parse_hyprland_geometry(&invalid_size), None);

        let missing = serde_json::json!({
            "title": "foot"
        });
        assert_eq!(parse_hyprland_geometry(&missing), None);
    }
}
