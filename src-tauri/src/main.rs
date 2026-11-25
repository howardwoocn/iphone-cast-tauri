#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod bluetooth;
mod input;
mod speaker;
mod media_state;

use bluetooth::{BluetoothService, Control, PointerInput};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::os::windows::process::CommandExt;
use std::{
    fs,
    io::{BufRead, BufReader, Write},
    path::PathBuf,
    process::{Child, ChildStdin, Command, Stdio},
    sync::{
        atomic::{AtomicU64, Ordering},
        Mutex,
    },
    thread,
    time::Duration,
};
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, RunEvent, State, WindowEvent,
};
use windows::Win32::{
    Foundation::{CloseHandle, HANDLE},
    System::{
        JobObjects::*,
        Threading::{OpenProcess, PROCESS_SET_QUOTA, PROCESS_TERMINATE},
    },
};

const PROTOCOL: u64 = 1;
#[derive(Serialize, Deserialize)]
struct Preferences {
    name: String,
    volume: f64,
    muted: bool,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CastStatus {
    receiver: String,
    bluetooth: String,
    message: String,
    bluetooth_message: String,
    media_ready: bool,
    media_playback: String,
    media_volume: Option<f64>,
    receiver_name: String,
    received_fps: f64,
    presented_fps: f64,
    dropped: u64,
    width: u32,
    height: u32,
    decoder: String,
    audio: String,
    volume: f64,
    muted: bool,
    focus: bool,
    test_mode: bool,
}
impl Default for CastStatus {
    fn default() -> Self {
        Self {
            receiver: "stopped".into(),
            bluetooth: "off".into(),
            message: "准备连接你的 iPhone".into(),
            bluetooth_message: "蓝牙控制尚未开启".into(),
            media_ready: false,
            media_playback: "unknown".into(),
            media_volume: None,
            receiver_name: "澄镜 iPhone Cast".into(),
            received_fps: 0.0,
            presented_fps: 0.0,
            dropped: 0,
            width: 0,
            height: 0,
            decoder: String::new(),
            audio: String::new(),
            volume: 0.8,
            muted: false,
            focus: false,
            test_mode: false,
        }
    }
}
#[derive(Clone, Copy, Default, Deserialize, Serialize)]
pub struct Viewport {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    scale: f64,
}
struct RestoreWindow {
    size: PhysicalSize<u32>,
    position: PhysicalPosition<i32>,
}
pub struct AppState {
    status: Mutex<CastStatus>,
    receiver: Mutex<Option<ReceiverProcess>>,
    pub bluetooth: Mutex<Option<BluetoothService>>,
    speaker: Mutex<speaker::Speaker>,
    viewport: Mutex<Viewport>,
    restore: Mutex<Option<RestoreWindow>>,
    session: AtomicU64,
    media_generation: AtomicU64,
    volume_request: AtomicU64,
    volume_control: Mutex<()>,
}
impl Default for AppState {
    fn default() -> Self {
        Self {
            status: Mutex::new(CastStatus::default()),
            receiver: Mutex::new(None),
            bluetooth: Mutex::new(None),
            speaker: Mutex::new(speaker::Speaker::default()),
            viewport: Mutex::new(Viewport::default()),
            restore: Mutex::new(None),
            session: AtomicU64::new(0),
            media_generation: AtomicU64::new(0),
            volume_request: AtomicU64::new(0),
            volume_control: Mutex::new(()),
        }
    }
}
struct Job(HANDLE);
unsafe impl Send for Job {}
impl Drop for Job {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}
struct ReceiverProcess {
    child: Child,
    input: ChildStdin,
    _job: Job,
}
impl ReceiverProcess {
    fn send(&mut self, value: Value) -> Result<(), String> {
        writeln!(self.input, "{}", value).map_err(|e| e.to_string())
    }
    fn stop(&mut self) {
        let _ = self.send(json!({"v":PROTOCOL,"cmd":"stop"}));
        for _ in 0..20 {
            if matches!(self.child.try_wait(), Ok(Some(_))) {
                return;
            }
            thread::sleep(Duration::from_millis(25));
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
impl Drop for ReceiverProcess {
    fn drop(&mut self) {
        self.stop();
    }
}

pub fn update_status(app: &AppHandle, f: impl FnOnce(&mut CastStatus)) {
    let state = app.state::<AppState>();
    let snapshot = {
        let mut status = state.status.lock().unwrap();
        f(&mut status);
        status.clone()
    };
    let _ = app.emit("cast-status", &snapshot);
}
fn send(app: &AppHandle, mut value: Value) -> Result<(), String> {
    value["v"] = json!(PROTOCOL);
    let state = app.state::<AppState>();
    let mut receiver = state.receiver.lock().unwrap();
    if let Some(receiver) = receiver.as_mut() {
        receiver.send(value)?;
    }
    Ok(())
}
fn runtime(app: &AppHandle) -> Result<PathBuf, String> {
    let development = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/runtime");
    let packaged = app
        .path()
        .resource_dir()
        .map_err(|e| e.to_string())?
        .join("runtime");
    if packaged.join("cast-receiver.exe").exists() {
        Ok(packaged)
    } else if development.join("cast-receiver.exe").exists() {
        Ok(development)
    } else {
        Err("缺少原生接收端。请先运行 npm run native:build，再启动桌面应用。".into())
    }
}
fn save_preferences(app: &AppHandle) {
    let state = app.state::<AppState>();
    let status = state.status.lock().unwrap().clone();
    if let Ok(directory) = app.path().app_config_dir() {
        if fs::create_dir_all(&directory).is_ok() {
            let preferences = Preferences {
                name: status.receiver_name,
                volume: status.volume,
                muted: status.muted,
            };
            if let Ok(bytes) = serde_json::to_vec_pretty(&preferences) {
                let _ = fs::write(directory.join("settings.json"), bytes);
            }
        }
    }
}
fn attach_job(child: &Child) -> Result<Job, String> {
    unsafe {
        let job = Job(CreateJobObjectW(None, None).map_err(|e| e.to_string())?);
        let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        SetInformationJobObject(
            job.0,
            JobObjectExtendedLimitInformation,
            &info as *const _ as _,
            std::mem::size_of_val(&info) as u32,
        )
        .map_err(|e| e.to_string())?;
        let process = OpenProcess(PROCESS_SET_QUOTA | PROCESS_TERMINATE, false, child.id())
            .map_err(|e| e.to_string())?;
        let result = AssignProcessToJobObject(job.0, process);
        let _ = CloseHandle(process);
        result.map_err(|e| e.to_string())?;
        Ok(job)
    }
}
#[tauri::command]
fn get_status(state: State<AppState>) -> CastStatus {
    state.status.lock().unwrap().clone()
}

#[tauri::command]
async fn start_receiver(
    app: AppHandle,
    name: String,
    test_mode: bool,
    force_software: Option<bool>,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        start_receiver_inner(app, name, test_mode, force_software)
    })
    .await
    .map_err(|e| e.to_string())?
}
fn start_receiver_inner(
    app: AppHandle,
    name: String,
    test_mode: bool,
    force_software: Option<bool>,
) -> Result<(), String> {
    let name = name.trim();
    if name.is_empty() || name.chars().count() > 48 || name.chars().any(char::is_control) {
        return Err("接收名称必须为 1–48 个可见字符".into());
    }
    if name.len() > 48 {
        return Err("接收名称过长，请缩短为不超过 16 个汉字或 48 个英文字母".into());
    }
    let state = app.state::<AppState>();
    let mut guard = state.receiver.lock().unwrap();
    if guard.is_some() {
        if state.status.lock().unwrap().receiver == "error" {
            state.session.fetch_add(1, Ordering::SeqCst);
            let process = guard.take();
            drop(guard);
            drop(process);
            guard = state.receiver.lock().unwrap();
            if guard.is_some() {
                return Err("接收端已运行，请先停止接收".into());
            }
        } else {
            return Err("接收端已运行，请先停止接收".into());
        }
    }
    let directory = runtime(&app)?;
    let window = app.get_webview_window("main").ok_or("找不到主窗口")?;
    let hwnd = window.hwnd().map_err(|e| e.to_string())?.0 as usize;
    let log_dir = app.path().app_log_dir().map_err(|e| e.to_string())?;
    fs::create_dir_all(&log_dir).map_err(|e| e.to_string())?;
    let error_log = fs::File::create(log_dir.join("receiver.log")).map_err(|e| e.to_string())?;
    let data = app.path().app_data_dir().map_err(|e| e.to_string())?;
    fs::create_dir_all(&data).map_err(|e| e.to_string())?;
    let mut command = Command::new(directory.join("cast-receiver.exe"));
    command.args(["--parent", &hwnd.to_string(), "--name", name]);
    if force_software.unwrap_or(false) {
        command.arg("--software");
    }
    if test_mode {
        command.arg("--test");
    }
    command
        .current_dir(&directory)
        .env("CAST_DATA_DIR", &data)
        .env("GST_PLUGIN_PATH", directory.join("gstreamer-1.0"))
        .env("GST_PLUGIN_SYSTEM_PATH", directory.join("gstreamer-1.0"))
        .env(
            "GST_PLUGIN_SCANNER",
            directory.join("gst-plugin-scanner.exe"),
        )
        .env("GST_REGISTRY", data.join("gstreamer-registry.bin"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::from(error_log))
        .creation_flags(0x08000000);
    let mut child = command
        .spawn()
        .map_err(|e| format!("原生接收端启动失败：{e}"))?;
    let job = match attach_job(&child) {
        Ok(job) => job,
        Err(e) => {
            let _ = child.kill();
            let _ = child.wait();
            return Err(e);
        }
    };
    let input = child.stdin.take().ok_or("无法创建命令管道")?;
    let output = child.stdout.take().ok_or("无法创建事件管道")?;
    let session = state.session.fetch_add(1, Ordering::SeqCst) + 1;
    *guard = Some(ReceiverProcess {
        child,
        input,
        _job: job,
    });
    drop(guard);
    update_status(&app, |s| {
        s.receiver = "starting".into();
        s.message = "正在启动 AirPlay 接收端…".into();
        s.receiver_name = name.into();
        s.test_mode = test_mode;
        s.width = 0;
        s.height = 0;
        s.received_fps = 0.0;
        s.presented_fps = 0.0;
        s.dropped = 0;
        s.decoder.clear();
        s.audio.clear();
    });
    save_preferences(&app);
    let reader_app = app.clone();
    thread::spawn(move || {
        let reader = BufReader::new(output);
        for line in reader.lines() {
            if reader_app
                .state::<AppState>()
                .session
                .load(Ordering::SeqCst)
                != session
            {
                break;
            }
            match line {
                Ok(line) => match serde_json::from_str::<Value>(&line) {
                    Ok(value) if value["v"].as_u64() == Some(PROTOCOL) => {
                        handle_event(&reader_app, value, session)
                    }
                    _ => { /* Raw dependency logs never update application state. */ }
                },
                Err(_) => break,
            }
        }
        let state = reader_app.state::<AppState>();
        if state.session.load(Ordering::SeqCst) == session {
            // Serialize process removal with start/stop; a stale reader must not remove a new process.
            let mut receiver = state.receiver.lock().unwrap();
            if state.session.load(Ordering::SeqCst) == session {
                let process = receiver.take();
                drop(receiver);
                drop(process);
                let _ = set_focus(reader_app.clone(), false);
                update_status(&reader_app, |s| {
                    s.receiver = "error".into();
                    s.message = "接收端已退出，请查看诊断日志后重新启动".into();
                    s.width = 0;
                    s.height = 0;
                    s.received_fps = 0.0;
                    s.presented_fps = 0.0;
                });
            }
        }
    });
    // Handshake timeout detects missing native DLLs or an early loader failure.
    let timeout_app = app.clone();
    thread::spawn(move || {
        thread::sleep(Duration::from_secs(15));
        let state = timeout_app.state::<AppState>();
        if state.session.load(Ordering::SeqCst) == session
            && state.status.lock().unwrap().receiver == "starting"
        {
            let _ = stop_receiver_inner(timeout_app.clone());
            update_status(&timeout_app, |s| {
                s.receiver = "error".into();
                s.message = "接收端启动超时，请检查 Bonjour 服务和原生运行库".into();
            });
        }
    });
    let rect = *state.viewport.lock().unwrap();
    send(&app, json!({"cmd":"viewport","rect":rect}))?;
    let status = state.status.lock().unwrap().clone();
    send(
        &app,
        json!({"cmd":"audio","volume":status.volume,"muted":status.muted}),
    )?;
    Ok(())
}
fn update_receiver_status(app: &AppHandle, session: u64, f: impl FnOnce(&mut CastStatus)) {
    let state = app.state::<AppState>();
    let snapshot = {
        let mut status = state.status.lock().unwrap();
        if state.session.load(Ordering::SeqCst) != session {
            return;
        }
        f(&mut status);
        status.clone()
    };
    let _ = app.emit("cast-status", snapshot);
}
fn handle_event(app: &AppHandle, event: Value, session: u64) {
    if app.state::<AppState>().session.load(Ordering::SeqCst) != session {
        return;
    }
    match event["event"].as_str().unwrap_or("") {
        "ready" => update_receiver_status(app, session, |s| {
            s.receiver = "listening".into();
            s.message = "接收已开启，请在 iPhone 屏幕镜像中选择此电脑".into();
            s.decoder = event["decoder"].as_str().unwrap_or("").into();
        }),
        "video" => {
            let width = event["width"].as_u64().unwrap_or(0) as u32;
            let height = event["height"].as_u64().unwrap_or(0) as u32;
            update_receiver_status(app, session, |s| {
                s.receiver = "streaming".into();
                s.width = width;
                s.height = height;
                s.message = if s.test_mode {
                    "正在运行本机 60fps 测试源"
                } else {
                    "iPhone 已连接 · 音视频正在本地传输"
                }
                .into();
            });
            let _ = resize_focus(app);
        }
        "stats" => update_receiver_status(app, session, |s| {
            s.received_fps = event["receivedFps"].as_f64().unwrap_or(0.0);
            s.presented_fps = event["presentedFps"].as_f64().unwrap_or(0.0);
            s.dropped = event["dropped"].as_u64().unwrap_or(0);
        }),
        "disconnected" => {
            release_pointer(app);
            update_receiver_status(app, session, |s| {
                s.receiver = "listening".into();
                s.width = 0;
                s.height = 0;
                s.received_fps = 0.0;
                s.presented_fps = 0.0;
                s.audio.clear();
                s.message = "手机已断开，等待重新连接".into();
            });
        }
        "audio" => update_receiver_status(app, session, |s| {
            s.audio = event["state"].as_str().unwrap_or("").into()
        }),
        "error" => {
            let _ = set_focus(app.clone(), false);
            update_receiver_status(app, session, |s| {
                s.receiver = "error".into();
                s.message = event["message"].as_str().unwrap_or("接收端错误").into();
            });
        }
        "warning" => update_receiver_status(app, session, |s| {
            s.message = event["message"].as_str().unwrap_or("").into()
        }),
        "decoder-fallback" => {
            let app = app.clone();
            thread::spawn(move || {
                if app.state::<AppState>().session.load(Ordering::SeqCst) != session {
                    return;
                }
                let status = app.state::<AppState>().status.lock().unwrap().clone();
                let result = stop_receiver_inner(app.clone()).and_then(|_| {
                    if app.state::<AppState>().session.load(Ordering::SeqCst) != session + 1 {
                        return Ok(());
                    }
                    start_receiver_inner(app.clone(), status.receiver_name, false, Some(true))
                });
                match result {
                    Ok(()) => update_status(&app, |s| {
                        s.message = "硬件解码失败，已切换软件解码，请重新连接屏幕镜像".into()
                    }),
                    Err(error) => update_status(&app, |s| {
                        s.receiver = "error".into();
                        s.message = format!("软件解码重启失败：{error}");
                    }),
                }
            });
        }
        "pointer" => {
            if let Ok(input) = serde_json::from_value::<PointerInput>(event) {
                let state = app.state::<AppState>();
                let status = state.status.lock().unwrap().clone();
                if (status.receiver != "streaming" || status.test_mode)
                    && input.kind != "release"
                    && input.kind != "up"
                {
                    return;
                }
                if let Some(service) = state.bluetooth.lock().unwrap().as_ref() {
                    // Movement may be coalesced by the native renderer. Never drop button-up.
                    let _ = service.tx.send(Control::Input(input));
                };
            }
        }
        "focus" => {
            let state = app.state::<AppState>();
            let enabled = !state.status.lock().unwrap().focus;
            let _ = set_focus(app.clone(), enabled);
        }
        _ => {}
    }
}
fn release_pointer(app: &AppHandle) {
    let state = app.state::<AppState>();
    if let Some(service) = state.bluetooth.lock().unwrap().as_ref() {
        let _ = service.tx.send(Control::Input(PointerInput {
            kind: "release".into(),
            x: 0.0,
            y: 0.0,
            wheel: 0,
            dx: 0.0,
            dy: 0.0,
        }));
    };
}
#[tauri::command]
async fn stop_receiver(app: AppHandle) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || stop_receiver_inner(app))
        .await
        .map_err(|e| e.to_string())?
}
fn stop_receiver_inner(app: AppHandle) -> Result<(), String> {
    let state = app.state::<AppState>();
    state.session.fetch_add(1, Ordering::SeqCst);
    let process = {
        let mut receiver = state.receiver.lock().unwrap();
        receiver.take()
    };
    drop(process);
    release_pointer(&app);
    if state.status.lock().unwrap().focus {
        set_focus(app.clone(), false)?;
    }
    update_status(&app, |s| {
        s.receiver = "stopped".into();
        s.message = "接收已停止".into();
        s.width = 0;
        s.height = 0;
        s.received_fps = 0.0;
        s.presented_fps = 0.0;
        s.audio.clear();
    });
    Ok(())
}
#[tauri::command]
fn start_bluetooth(app: AppHandle) -> Result<(), String> {
    let state = app.state::<AppState>();
    let mut service = state.bluetooth.lock().unwrap();
    if service.is_some() {
        return Err("蓝牙服务正在运行或关闭，请稍后重试".into());
    }
    *service = Some(BluetoothService::start(app.clone()));
    Ok(())
}
#[tauri::command]
fn stop_bluetooth(app: AppHandle) -> Result<(), String> {
    release_pointer(&app);
    let state = app.state::<AppState>();
    if let Some(service) = state.bluetooth.lock().unwrap().as_ref() {
        service.tx.send(Control::Stop).map_err(|e| e.to_string())?;
    }
    Ok(())
}
#[tauri::command]
fn set_audio(app: AppHandle, volume: f64, muted: bool) -> Result<(), String> {
    if !volume.is_finite() || !(0.0..=1.0).contains(&volume) {
        return Err("音量必须为 0–1".into());
    }
    send(&app, json!({"cmd":"audio","volume":volume,"muted":muted}))?;
    update_status(&app, |s| {
        s.volume = volume;
        s.muted = muted;
    });
    Ok(())
}
#[tauri::command]
fn set_viewport(app: AppHandle, rect: Viewport) -> Result<(), String> {
    if [rect.x, rect.y, rect.width, rect.height, rect.scale]
        .iter()
        .any(|v| !v.is_finite())
        || rect.width < 0.0
        || rect.height < 0.0
        || rect.scale <= 0.0
        || rect.scale > 8.0
    {
        return Err("无效的视频区域".into());
    }
    *app.state::<AppState>().viewport.lock().unwrap() = rect;
    send(&app, json!({"cmd":"viewport","rect":rect}))
}
#[tauri::command]
fn set_video_visible(app: AppHandle, visible: bool) -> Result<(), String> {
    release_pointer(&app);
    send(&app, json!({"cmd":"visible","visible":visible}))
}
fn resize_focus(app: &AppHandle) -> Result<(), String> {
    let state = app.state::<AppState>();
    let status = state.status.lock().unwrap().clone();
    if !status.focus {
        return Ok(());
    }
    let window = app.get_webview_window("main").ok_or("找不到窗口")?;
    let current = window.inner_size().map_err(|e| e.to_string())?;
    let ratio = if status.width > 0 && status.height > 0 {
        status.width as f64 / status.height as f64
    } else {
        9.0 / 19.5
    };
    let monitor = window
        .current_monitor()
        .map_err(|e| e.to_string())?
        .ok_or("找不到显示器")?;
    let max_width = (monitor.size().width as f64 * 0.92) as u32;
    let max_height = (monitor.size().height as f64 * 0.90) as u32;
    let height = current
        .height
        .min(max_height)
        .min((max_width as f64 / ratio) as u32)
        .max(200);
    let width = (height as f64 * ratio).round() as u32;
    if current.width.abs_diff(width) > 1 || current.height.abs_diff(height) > 1 {
        window
            .set_size(PhysicalSize::new(width, height))
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}
#[tauri::command]
fn set_focus(app: AppHandle, enabled: bool) -> Result<(), String> {
    let state = app.state::<AppState>();
    if state.status.lock().unwrap().focus == enabled {
        return Ok(());
    }
    let window = app.get_webview_window("main").ok_or("找不到窗口")?;
    if enabled {
        let status = state.status.lock().unwrap().clone();
        if status.width == 0 || status.height == 0 {
            return Err("收到手机画面后才能进入专注模式".into());
        }
        *state.restore.lock().unwrap() = Some(RestoreWindow {
            size: window.inner_size().map_err(|e| e.to_string())?,
            position: window.outer_position().map_err(|e| e.to_string())?,
        });
        window
            .set_min_size(None::<PhysicalSize<u32>>)
            .map_err(|e| e.to_string())?;
        window.set_decorations(false).map_err(|e| e.to_string())?;
        update_status(&app, |s| s.focus = true);
        resize_focus(&app)?;
    } else {
        update_status(&app, |s| s.focus = false);
        window.set_decorations(true).map_err(|e| e.to_string())?;
        if let Some(restore) = state.restore.lock().unwrap().take() {
            window.set_size(restore.size).map_err(|e| e.to_string())?;
            window
                .set_position(restore.position)
                .map_err(|e| e.to_string())?;
        }
        window
            .set_min_size(Some(tauri::LogicalSize::new(800.0, 700.0)))
            .map_err(|e| e.to_string())?;
    }
    send(&app, json!({"cmd":"focus","enabled":enabled}))?;
    release_pointer(&app);
    Ok(())
}
#[tauri::command]
fn export_diagnostics(app: AppHandle) -> Result<String, String> {
    let state = app.state::<AppState>();
    let directory = app.path().app_log_dir().map_err(|e| e.to_string())?;
    fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
    let path = directory.join("diagnostics.json");
    let report = json!({"appVersion":env!("CARGO_PKG_VERSION"),"protocol":PROTOCOL,"platform":"Windows 11 x64","status":state.status.lock().unwrap().clone(),"validation":{"device":"iPhone 17 / iOS 27","absolutePointer":"not-verified-on-device","realDeviceFps":"not-verified-on-device"},"receiverLog":directory.join("receiver.log")});
    fs::write(
        &path,
        serde_json::to_vec_pretty(&report).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    update_status(&app, |s| {
        s.message = format!("诊断报告已保存到 {}", path.display())
    });
    Ok(path.to_string_lossy().into_owned())
}
fn show_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
        let _ = send(app, json!({"cmd":"relayout"}));
    }
}

fn shutdown(app: &AppHandle) {
    let state = app.state::<AppState>();
    state.volume_request.fetch_add(1, Ordering::SeqCst);
    state.session.fetch_add(1, Ordering::SeqCst);
    state.speaker.lock().unwrap().disconnect();
    save_preferences(app);
    release_pointer(app);
    if let Some(service) = state.bluetooth.lock().unwrap().take() {
        service.finish();
    }
    // Finish the receiver before the event loop and process exit.
    let process = state.receiver.lock().unwrap().take();
    drop(process);
}

fn main() {
    tauri::Builder::default()
        .manage(AppState::default())
        .setup(|app| {
            let open = MenuItem::with_id(app, "open", "打开澄镜", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&open, &quit])?;
            let icon = app.default_window_icon().cloned().ok_or("缺少应用图标")?;
            TrayIconBuilder::with_id("main")
                .icon(icon)
                .tooltip("澄镜 · iPhone Cast")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "open" => show_main_window(app),
                    "quit" => app.exit(0),
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if matches!(event, TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    }) {
                        show_main_window(tray.app_handle());
                    }
                })
                .build(app)?;
            if let Ok(directory) = app.path().app_config_dir() {
                if let Ok(bytes) = fs::read(directory.join("settings.json")) {
                    if let Ok(preferences) = serde_json::from_slice::<Preferences>(&bytes) {
                        let mut status = app.state::<AppState>().status.lock().unwrap().clone();
                        if !preferences.name.trim().is_empty()
                            && preferences.name.chars().count() <= 48
                            && !preferences.name.chars().any(char::is_control)
                        {
                            status.receiver_name = preferences.name;
                        }
                        if preferences.volume.is_finite()
                            && (0.0..=1.0).contains(&preferences.volume)
                        {
                            status.volume = preferences.volume;
                        }
                        status.muted = preferences.muted;
                        *app.state::<AppState>().status.lock().unwrap() = status;
                    }
                }
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_status,
            speaker::list_speaker_devices,
            speaker::get_speaker_status,
            speaker::connect_speaker,
            speaker::disconnect_speaker,
            speaker::send_media_control,
            speaker::set_phone_volume,
            start_receiver,
            stop_receiver,
            start_bluetooth,
            stop_bluetooth,
            set_audio,
            set_viewport,
            set_video_visible,
            set_focus,
            export_diagnostics
        ])
        .on_window_event(|window, event| {
            match event {
                WindowEvent::CloseRequested { api, .. } => {
                    if window.label() == "main" {
                        api.prevent_close();
                        release_pointer(window.app_handle());
                        save_preferences(window.app_handle());
                        let _ = window.hide();
                    }
                }
                WindowEvent::Focused(false) => {
                    // The renderer is a child HWND in another process. Its activation
                    // must not turn an in-progress phone drag into a release.
                    let still_in_window = window.hwnd().ok().is_some_and(|hwnd| unsafe {
                        use windows::Win32::UI::WindowsAndMessaging::{
                            GetAncestor, GetForegroundWindow, GA_ROOT,
                        };
                        GetAncestor(GetForegroundWindow(), GA_ROOT).0 == hwnd.0
                    });
                    if !still_in_window {
                        release_pointer(window.app_handle());
                    }
                }
                WindowEvent::Resized(_) => {
                    let app = window.app_handle().clone();
                    // Queue resize operations onto the UI thread rather than calling back into it synchronously.
                    let queued = app.clone();
                    let _ = app.run_on_main_thread(move || {
                        let _ = resize_focus(&queued);
                        let _ = send(&queued, json!({"cmd":"relayout"}));
                    });
                }
                _ => {}
            }
        })
        .build(tauri::generate_context!())
        .expect("运行 Tauri 应用失败")
        .run(|app, event| {
            if matches!(event, RunEvent::Exit) {
                shutdown(app);
            }
        });
}
