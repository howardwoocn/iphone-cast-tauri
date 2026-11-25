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
        Ok(speaker.status())
    }).await.map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn disconnect_speaker(app: AppHandle) -> Result<SpeakerStatus, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let _apartment = Apartment::new().map_err(|e| e.to_string())?;
        let state = app.state::<crate::AppState>();
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
        _ => return Err("不支持的播放操作".into()),
    };
    tauri::async_runtime::spawn_blocking(move || {
        let _apartment = Apartment::new().map_err(|e| e.to_string())?;
        let state = app.state::<crate::AppState>();
        if state.speaker.lock().unwrap().status().state != "connected" {
            return Err("请先连接手机音频".into());
        }
        let (tx, rx) = std::sync::mpsc::channel();
        {
            let service = state.bluetooth.lock().unwrap();
            let service = service.as_ref().ok_or("请先开启并配对蓝牙媒体控制")?;
            service.tx.send(crate::bluetooth::Control::Media(usage, tx)).map_err(|_| "蓝牙控制服务已停止")?;
        }
        rx.recv_timeout(std::time::Duration::from_secs(5)).map_err(|_| "媒体按键发送超时，请检查蓝牙连接".to_string())?
    }).await.map_err(|e| e.to_string())?
}
