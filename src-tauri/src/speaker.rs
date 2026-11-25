//! Windows A2DP sink. Windows owns decoding and the playback endpoint.
use serde::Serialize;
use tauri::{AppHandle, Manager};
use windows::{
    core::{HSTRING, Result as WinResult},
    Devices::Enumeration::DeviceInformation,
    Media::Audio::{AudioPlaybackConnection, AudioPlaybackConnectionOpenResultStatus, AudioPlaybackConnectionState},
    Win32::System::WinRT::{RoInitialize, RoUninitialize, RO_INIT_MULTITHREADED},
};

struct Apartment;
impl Apartment {
    fn new() -> WinResult<Self> {
        unsafe { RoInitialize(RO_INIT_MULTITHREADED)?; }
        Ok(Self)
    }
}
impl Drop for Apartment {
    fn drop(&mut self) { unsafe { RoUninitialize(); } }
}

#[derive(Clone, Serialize)]
pub struct SpeakerDevice { id: String, name: String }
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpeakerStatus { state: String, device_id: String, device_name: String, message: String }

#[derive(Default)]
pub struct Speaker {
    connection: Option<AudioPlaybackConnection>,
    device: Option<SpeakerDevice>,
    error: Option<String>,
}
impl Speaker {
    pub fn disconnect(&mut self) {
        if let Some(connection) = self.connection.take() { let _ = connection.Close(); }
        self.device = None;
        self.error = None;
    }
    fn status(&self) -> SpeakerStatus {
        let (state, message) = if let Some(error) = &self.error {
            ("error", error.clone())
        } else if let Some(connection) = &self.connection {
            match connection.State() {
                Ok(AudioPlaybackConnectionState::Opened) => ("connected", "音频连接已建立，请在手机上播放音乐".into()),
                Ok(_) => ("disconnected", "音频连接已断开，请重新连接".into()),
                Err(e) => ("error", format!("读取音频连接失败：{e}")),
            }
        } else { ("off", "选择已配对的手机以接收声音".into()) };
        SpeakerStatus { state: state.into(), device_id: self.device.as_ref().map(|d| d.id.clone()).unwrap_or_default(), device_name: self.device.as_ref().map(|d| d.name.clone()).unwrap_or_default(), message }
    }
}
impl Drop for Speaker { fn drop(&mut self) { self.disconnect(); } }

fn devices() -> WinResult<Vec<SpeakerDevice>> {
    let found = DeviceInformation::FindAllAsyncAqsFilter(&AudioPlaybackConnection::GetDeviceSelector()?)?.get()?;
    let mut result = Vec::new();
    for index in 0..found.Size()? {
        let item = found.GetAt(index)?;
        result.push(SpeakerDevice { id: item.Id()?.to_string(), name: item.Name()?.to_string() });
    }
    result.sort_by(|a, b| a.name.cmp(&b.name).then(a.id.cmp(&b.id)));
    Ok(result)
}

#[tauri::command]
pub async fn list_speaker_devices() -> Result<Vec<SpeakerDevice>, String> {
    tauri::async_runtime::spawn_blocking(|| {
        let _apartment = Apartment::new().map_err(|e| e.to_string())?;
        devices().map_err(|e| format!("无法获取蓝牙音频设备：{e}。请检查电脑蓝牙并先在 Windows 设置中配对手机。"))
    }).await.map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn get_speaker_status(app: AppHandle) -> Result<SpeakerStatus, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let _apartment = Apartment::new().map_err(|e| e.to_string())?;
        Ok(app.state::<crate::AppState>().speaker.lock().unwrap().status())
    }).await.map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn connect_speaker(app: AppHandle, device_id: String) -> Result<SpeakerStatus, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let _apartment = Apartment::new().map_err(|e| e.to_string())?;
        let state = app.state::<crate::AppState>();
        let volume_request = state.volume_request.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
        let mut speaker = state.speaker.lock().unwrap();
        speaker.disconnect();
        let result = (|| -> Result<(), String> {
            let device = devices().map_err(|e| e.to_string())?.into_iter().find(|d| d.id == device_id)
                .ok_or_else(|| "设备已不可用，请重新配对并刷新列表".to_string())?;
            let connection = AudioPlaybackConnection::TryCreateFromId(&HSTRING::from(&device.id)).map_err(|e| format!("此设备无法建立蓝牙音频连接：{e}"))?;
            // Store immediately so any failed start/open path releases the A2DP sink.
            speaker.device = Some(device);
            speaker.connection = Some(connection.clone());
            connection.StartAsync().and_then(|op| op.get()).map_err(|e| e.to_string())?;
            let result = connection.OpenAsync().and_then(|op| op.get()).map_err(|e| e.to_string())?;
            if result.Status().map_err(|e| e.to_string())? != AudioPlaybackConnectionOpenResultStatus::Success {
                return Err("手机未建立音频连接，请确认手机蓝牙已开启，并重试连接".into());
            }
            Ok(())
        })();
        if let Err(error) = result {
            speaker.disconnect();
            speaker.error = Some(error.clone());
            return Err(error);
        }
        let status = speaker.status();
        drop(speaker);
        if status.state == "connected" {
            let app = app.clone();
            std::thread::spawn(move || synchronize_volume(&app, volume_request));
        }
        Ok(status)
    }).await.map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn disconnect_speaker(app: AppHandle) -> Result<SpeakerStatus, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let _apartment = Apartment::new().map_err(|e| e.to_string())?;
        let state = app.state::<crate::AppState>();
        state.volume_request.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let mut speaker = state.speaker.lock().unwrap();
        speaker.disconnect();
        Ok(speaker.status())
    }).await.map_err(|e| e.to_string())?
}


/// Send a momentary Consumer Control key; completion confirms delivery, not playback.
#[tauri::command]
pub async fn send_media_control(app: AppHandle, action: String) -> Result<(), String> {
    let usage = match action.as_str() {
        "previous" => 0x00b6,
        "play" => 0x00b0,
        "pause" => 0x00b1,
        "toggle" => 0x00cd,
        "next" => 0x00b5,
        "volume_down" => 0x00ea,
        "volume_up" => 0x00e9,
        _ => return Err("不支持的媒体操作".into()),
    };
    tauri::async_runtime::spawn_blocking(move || {
        let _apartment = Apartment::new().map_err(|e| e.to_string())?;
        let state = app.state::<crate::AppState>();
        if state.speaker.lock().unwrap().status().state != "connected" {
            return Err("请先连接手机音频".into());
        }
        send_usage(&app, usage)
    }).await.map_err(|e| e.to_string())?
}


fn send_usage(app: &AppHandle, usage: u16) -> Result<(), String> {
    let state = app.state::<crate::AppState>();
    let (tx, rx) = std::sync::mpsc::channel();
    {
        let service = state.bluetooth.lock().unwrap();
        let service = service.as_ref().ok_or("请先开启并配对蓝牙媒体控制")?;
        service.tx.send(crate::bluetooth::Control::Media(usage, tx)).map_err(|_| "蓝牙控制服务已停止")?;
    }
    rx.recv_timeout(std::time::Duration::from_secs(5)).map_err(|_| "媒体按键发送超时，请检查蓝牙连接".to_string())?
}

/// Nudge once after connecting when AMS has not supplied its initial volume.
/// The inverse key is best-effort: an unknown starting level cannot be recovered
/// exactly, especially if the first key saturated at the upper volume limit.
fn synchronize_volume(app: &AppHandle, request: u64) {
    use std::{sync::atomic::Ordering, thread, time::{Duration, Instant}};
    let Ok(_apartment) = Apartment::new() else { return; };
    let state = app.state::<crate::AppState>();
    let active = || state.volume_request.load(Ordering::SeqCst) == request
        && state.speaker.lock().unwrap().status().state == "connected";
    let started = Instant::now();
    // HID can become ready after the A2DP connection. Do not delay connect_speaker.
    loop {
        if !active() { return; }
        let status = state.status.lock().unwrap().clone();
        if status.media_volume.is_some() { return; }
        if status.media_ready { break; }
        if started.elapsed() >= Duration::from_secs(10) { return; }
        thread::sleep(Duration::from_millis(100));
    }
    let _control = state.volume_control.lock().unwrap();
    let generation = state.media_generation.load(Ordering::SeqCst);
    let same_connection = || active() && state.media_generation.load(Ordering::SeqCst) == generation
        && state.status.lock().unwrap().media_ready;
    // Allow the normal AMS subscription's initial update to arrive first.
    for _ in 0..8 {
        if !same_connection() || state.status.lock().unwrap().media_volume.is_some() { return; }
        thread::sleep(Duration::from_millis(100));
    }
    if !same_connection() { return; }
    if send_usage(app, 0x00e9).is_err() { return; }
    let sent = Instant::now();
    let after = loop {
        if !same_connection() { return; }
        let volume = state.status.lock().unwrap().media_volume;
        if volume.is_some() || sent.elapsed() >= Duration::from_millis(1500) { break volume; }
        thread::sleep(Duration::from_millis(40));
    };
    // At the upper limit the original volume may already have been 100%.
    if after.is_some_and(|volume| volume >= 1.0) { return; }
    // A manual slider request or disconnect supersedes this automatic operation.
    if same_connection() && send_usage(app, 0x00ea).is_err() {
        crate::update_status(app, |status| {
            status.message = "手机音量恢复指令发送失败，请在手机上确认音量".into();
        });
    }
}

/// The phone exposes incremental volume keys, so converge on a slider target
/// using actual AMS feedback. Never assume a fixed iOS step size.
#[tauri::command]
pub async fn set_phone_volume(app: AppHandle, volume: f64) -> Result<Option<f64>, String> {
    use std::{sync::atomic::Ordering, time::{Duration, Instant}};
    if !volume.is_finite() || !(0.0..=1.0).contains(&volume) { return Err("无效的手机音量".into()); }
    let request = app.state::<crate::AppState>().volume_request.fetch_add(1, Ordering::SeqCst) + 1;
    tauri::async_runtime::spawn_blocking(move || {
        let _apartment = Apartment::new().map_err(|e| e.to_string())?;
        let state = app.state::<crate::AppState>();
        let _control = state.volume_control.lock().unwrap();
        let generation = state.media_generation.load(Ordering::SeqCst);
        let started = Instant::now();
        let current_volume = || -> Result<f64, String> {
            if state.media_generation.load(Ordering::SeqCst) != generation {
                return Err("手机媒体连接已变化，请重新调节".into());
            }
            if state.speaker.lock().unwrap().status().state != "connected" { return Err("手机音频已断开".into()); }
            let status = state.status.lock().unwrap();
            if !status.media_ready { return Err("手机媒体控制已断开".into()); }
            status.media_volume.ok_or_else(|| "手机尚未回传音量，暂时无法使用滑杆".into())
        };
        for _ in 0..32 {
            if state.volume_request.load(Ordering::SeqCst) != request { return Ok(None); }
            let before = current_volume()?;
            if (before - volume).abs() <= 0.01 { return Ok(Some(before)); }
            if started.elapsed() > Duration::from_secs(8) { return Err("手机音量调节超时".into()); }
            send_usage(&app, if volume > before { 0x00e9 } else { 0x00ea })?;
            let feedback = Instant::now();
            let after = loop {
                if state.volume_request.load(Ordering::SeqCst) != request { return Ok(None); }
                let value = current_volume()?;
                if (value - before).abs() > 0.0001 { break value; }
                if feedback.elapsed() > Duration::from_secs(1) { return Err("未收到手机音量变化，请检查手机媒体连接".into()); }
                std::thread::sleep(Duration::from_millis(40));
            };
            let step = (after - before).abs();
            // Snap to a realizable phone volume, including nonstandard step sizes.
            if (after - volume).abs() <= step / 2.0 + 0.001 { return Ok(Some(after)); }
            if (volume - before).signum() != (volume - after).signum() { return Ok(Some(after)); }
        }
        Err("手机音量无法到达指定位置".into())
    }).await.map_err(|e| e.to_string())?
}
