use crate::input::{PointerState, ABSOLUTE_ID, MEDIA_ID, MOUSE_ID, REPORT_MAP};
use serde::Deserialize;
use std::{
    sync::mpsc::{self, Receiver, Sender},
    thread,
    time::Duration,
};
use tauri::{AppHandle, Manager};
use windows::{
    core::{Result as WinResult, GUID},
    Devices::Bluetooth::GenericAttributeProfile::*,
    Devices::Bluetooth::{BluetoothAdapter, BluetoothError},
    Foundation::TypedEventHandler,
    Storage::Streams::{DataWriter, IBuffer},
};

#[derive(Clone, Debug, Deserialize)]
pub struct PointerInput {
    pub kind: String,
    #[serde(default)]
    pub x: f64,
    #[serde(default)]
    pub y: f64,
    #[serde(default)]
    pub wheel: i8,
    #[serde(default)]
    pub dx: f64,
    #[serde(default)]
    pub dy: f64,
}
impl PointerInput {
    fn merge_move(&mut self, next: Self) {
        let dx = self.dx + next.dx;
        let dy = self.dy + next.dy;
        *self = next;
        self.dx = dx;
        self.dy = dy;
    }
}

pub enum Control {
    Input(PointerInput),
    Media(u16, Sender<Result<(), String>>),
    Stop,
}
pub struct BluetoothService {
    pub tx: Sender<Control>,
    done: Receiver<()>,
}

fn uuid(short: u16) -> GUID {
    GUID::from_u128(((short as u128) << 96) | 0x00001000800000805f9b34fb)
}
fn buffer(bytes: &[u8]) -> WinResult<IBuffer> {
    let writer = DataWriter::new()?;
    writer.WriteBytes(bytes)?;
    writer.DetachBuffer()
}
fn update(app: &AppHandle, state: &str, message: &str) {
    crate::update_status(app, |s| {
        s.bluetooth = state.into();
        if state == "off" || state == "error" { s.media_ready = false; }
        s.bluetooth_message = message.into();
    });
}

async fn characteristic(
    service: &GattLocalService,
    id: u16,
    properties: GattCharacteristicProperties,
    bytes: &[u8],
) -> WinResult<GattLocalCharacteristic> {
    let parameters = GattLocalCharacteristicParameters::new()?;
    parameters.SetCharacteristicProperties(properties)?;
    parameters.SetReadProtectionLevel(GattProtectionLevel::EncryptionRequired)?;
    parameters.SetWriteProtectionLevel(GattProtectionLevel::EncryptionRequired)?;
    if !bytes.is_empty() {
        parameters.SetStaticValue(&buffer(bytes)?)?;
    }
    let result = service
        .CreateCharacteristicAsync(uuid(id), &parameters)?
        .await?;
    if result.Error()? != BluetoothError::Success {
        return Err(windows::core::Error::new(
            windows::core::HRESULT(0x80004005u32 as i32),
            "无法创建蓝牙 HID 特征",
        ));
    }
    result.Characteristic()
}

async fn report(
    service: &GattLocalService,
    id: u8,
    initial: &[u8],
) -> WinResult<GattLocalCharacteristic> {
    let item = characteristic(
        service,
        0x2a4d,
        GattCharacteristicProperties::Read | GattCharacteristicProperties::Notify,
        initial,
    )
    .await?;
    let parameters = GattLocalDescriptorParameters::new()?;
    parameters.SetStaticValue(&buffer(&[id, 1])?)?;
    parameters.SetReadProtectionLevel(GattProtectionLevel::EncryptionRequired)?;
    let result = item
        .CreateDescriptorAsync(uuid(0x2908), &parameters)?
        .await?;
    if result.Error()? != BluetoothError::Success {
        return Err(windows::core::Error::new(
            windows::core::HRESULT(0x80004005u32 as i32),
            "无法创建 HID Report Reference",
        ));
    }
    Ok(item)
}

impl BluetoothService {
    pub fn start(app: AppHandle) -> Self {
        let (tx, rx) = mpsc::channel();
        let (done_tx, done) = mpsc::channel();
        thread::spawn(move || {
            let initialized = unsafe {
                windows::Win32::System::WinRT::RoInitialize(
                    windows::Win32::System::WinRT::RO_INIT_MULTITHREADED,
                )
            };
            let com_initialized = initialized.is_ok();
            let result = match initialized {
                Ok(()) => tauri::async_runtime::block_on(run(&app, rx)),
                Err(e) => Err(e),
            };
            if let Err(e) = result {
                update(
                    &app,
                    "error",
                    &format!("蓝牙控制启动失败：{e}。检查蓝牙是否支持 BLE 外设模式。"),
                );
            }
            if com_initialized {
                unsafe {
                    windows::Win32::System::WinRT::RoUninitialize();
                }
            }
            let _ = done_tx.send(());
            let state = app.state::<crate::AppState>();
            state.bluetooth.lock().unwrap().take();
        });
        Self { tx, done }
    }
    pub fn finish(&self) {
        let _ = self.tx.send(Control::Stop);
        let _ = self.done.recv_timeout(Duration::from_secs(2));
    }
}

async fn run(app: &AppHandle, rx: Receiver<Control>) -> WinResult<()> {
    update(app, "starting", "正在检测 BLE 外设模式…");
    let adapter = BluetoothAdapter::GetDefaultAsync()?.await?;
    if !adapter.IsPeripheralRoleSupported()? {
        update(
            app,
            "error",
            "此电脑蓝牙不支持 BLE 外设模式，投屏与声音仍可使用。",
        );
        return Ok(());
    }
    let result = GattServiceProvider::CreateAsync(uuid(0x1812))?.await?;
    if result.Error()? != BluetoothError::Success {
        update(
            app,
            "error",
            &format!("无法创建 HID 服务：{:?}", result.Error()?),
        );
        return Ok(());
    }
    let provider = result.ServiceProvider()?;
    let service = provider.Service()?;
    let read = GattCharacteristicProperties::Read;
    characteristic(&service, 0x2a4a, read, &[0x11, 0x01, 0x00, 0x02]).await?;
    characteristic(&service, 0x2a4b, read, REPORT_MAP).await?;
    let protocol = characteristic(
        &service,
        0x2a4e,
        read | GattCharacteristicProperties::WriteWithoutResponse,
        &[1],
    )
    .await?;
    let control = characteristic(
        &service,
        0x2a4c,
        GattCharacteristicProperties::WriteWithoutResponse,
        &[],
    )
    .await?;
    // Acknowledge protocol/control writes; the only implemented protocol is Report.
    let handler = TypedEventHandler::<GattLocalCharacteristic, GattWriteRequestedEventArgs>::new(
        |_, args| {
            if let Some(args) = args.as_ref() {
                let deferral = args.GetDeferral()?;
                let args = args.clone();
                tauri::async_runtime::spawn(async move {
                    if let Ok(op) = args.GetRequestAsync() {
                        if let Ok(request) = op.await {
                            if request.Option().ok() == Some(GattWriteOption::WriteWithResponse) {
                                let _ = request.Respond();
                            }
                        }
                    }
                    let _ = deferral.Complete();
                });
            }
            Ok(())
        },
    );
    protocol.WriteRequested(&handler)?;
    control.WriteRequested(&handler)?;
    let absolute = report(&service, ABSOLUTE_ID, &[0; 5]).await?;
    let mouse = report(&service, MOUSE_ID, &[0; 4]).await?;
    let media = report(&service, MEDIA_ID, &[0; 2]).await?;
    let params = GattServiceProviderAdvertisingParameters::new()?;
    params.SetIsConnectable(true)?;
    params.SetIsDiscoverable(true)?;
    provider.StartAdvertisingWithParameters(&params)?;
    update(
        app,
        "advertising",
        "在 iPhone 蓝牙列表中配对此电脑，然后开启辅助触控；准确点击尚待真机验证。",
    );

    let mut connected = false;
    let mut pointer = PointerState::default();
    let mut sequence_error: Option<windows::core::Error> = None;
    let mut pending: Option<Control> = None;
    let mut media_ready = false;
    let mut media_monitor = None;
    let mut monitored_device = None;
    loop {
        let media_clients = media.SubscribedClients()?;
        let subscribed = media_clients.Size()? == 1;
        let device = if subscribed { Some(media_clients.GetAt(0)?.Session()?.DeviceId()?.Id()?.to_string()) } else { None };
        if device != monitored_device {
            drop(media_monitor.take());
            if let Some(id) = &device {
                media_monitor = Some(crate::media_state::MediaMonitor::start(app.clone(), id.clone()));
            }
            monitored_device = device;
        }
        if subscribed != media_ready {
            media_ready = subscribed;
            crate::update_status(app, |s| s.media_ready = subscribed);
        }
        let absolute_clients = absolute.SubscribedClients()?;
        let mouse_clients = mouse.SubscribedClients()?;
        let subscribers = absolute_clients.Size()? == 1
            && mouse_clients.Size()? == 1
            && absolute_clients.GetAt(0)?.Session()?.DeviceId()?.Id()?
                == mouse_clients.GetAt(0)?.Session()?.DeviceId()?.Id()?;
        if subscribers != connected {
            connected = subscribers;
            update(
                app,
                if connected {
                    "connected"
                } else {
                    "advertising"
                },
                if connected {
                    "HID 报告已订阅；请确认投屏和蓝牙连接的是同一台 iPhone。准确点击尚待真机验证。"
                } else {
                    "等待 iPhone 配对并订阅 HID 报告"
                },
            );
            pointer = PointerState::default();
            if connected {
                let _ = mouse.NotifyValueAsync(&buffer(&[0; 4])?)?.await;
            }
        }
        let next = pending
            .take()
            .map(Ok)
            .unwrap_or_else(|| rx.recv_timeout(Duration::from_millis(100)));
        match next {
            Ok(Control::Stop) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if connected && pointer.is_pressed() {
                    for report in pointer.apply("hold", 0.0, 0.0, 0, 0.0, 0.0) {
                        if let Err(e) = notify(&mouse, &report.bytes).await {
                            sequence_error = Some(e);
                            break;
                        }
                    }
                    if sequence_error.is_some() {
                        break;
                    }
                }
                continue;
            }
            Ok(Control::Media(usage, reply)) => {
                let sent = async {
                    let media_clients = media.SubscribedClients()?;
                    let mouse_clients = mouse.SubscribedClients()?;
                    if media_clients.Size()? != 1 || (mouse_clients.Size()? == 1
                        && media_clients.GetAt(0)?.Session()?.DeviceId()?.Id()?
                        != mouse_clients.GetAt(0)?.Session()?.DeviceId()?.Id()?) {
                        return Err(windows::core::Error::new(windows::core::HRESULT(0x80004005u32 as i32), "媒体控制与鼠标必须连接同一台手机"));
                    }
                    notify(&media, &usage.to_le_bytes()).await?;
                    thread::sleep(Duration::from_millis(40));
                    notify(&media, &[0; 2]).await
                }.await;
                // Always try to release after an interrupted press as well.
                if sent.is_err() { let _ = notify(&media, &[0; 2]).await; }
                let _ = reply.send(sent.map_err(|e| e.to_string()));
            }
            Ok(Control::Input(mut input)) => {
                if !connected {
                    continue;
                }
                // Keep only the latest contiguous move, preserving every button transition.
                if input.kind == "move" {
                    while let Ok(next) = rx.try_recv() {
                        match next {
                            Control::Input(next) if next.kind == "move" => {
                                input.merge_move(next);
                            }
                            other => {
                                pending = Some(other);
                                break;
                            }
                        }
                    }
                }
                let sent: WinResult<()> = async {
                    for report in pointer.apply(
                        &input.kind,
                        input.x,
                        input.y,
                        input.wheel,
                        input.dx,
                        input.dy,
                    ) {
                        if report.delay_ms > 0 {
                            thread::sleep(Duration::from_millis(report.delay_ms));
                        }
                        notify(
                            if report.id == ABSOLUTE_ID {
                                &absolute
                            } else {
                                &mouse
                            },
                            &report.bytes,
                        )
                        .await?;
                    }
                    Ok(())
                }
                .await;
                if let Err(e) = sent {
                    sequence_error = Some(e);
                    break;
                }
            }
        }
    }
    if connected {
        let _ = notify(&mouse, &[0; 4]).await;
    }
    let _ = notify(&media, &[0; 2]).await;
    provider.StopAdvertising()?;
    if let Some(error) = sequence_error {
        return Err(error);
    }
    update(app, "off", "蓝牙控制已关闭");
    Ok(())
}

async fn notify(item: &GattLocalCharacteristic, bytes: &[u8]) -> WinResult<()> {
    let clients = item.SubscribedClients()?;
    if clients.Size()? != 1 {
        return Err(windows::core::Error::new(
            windows::core::HRESULT(0x80004005u32 as i32),
            "鼠标控制仅支持一个蓝牙订阅设备",
        ));
    }
    let result = item
        .NotifyValueForSubscribedClientAsync(&buffer(bytes)?, &clients.GetAt(0)?)?
        .await?;
    if result.Status()? != GattCommunicationStatus::Success {
        return Err(windows::core::Error::new(
            windows::core::HRESULT(0x80004005u32 as i32),
            "蓝牙报告发送失败，已释放控制状态，请重新配对",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn coalescing_keeps_total_motion_and_latest_absolute_position() {
        let mut first = PointerInput {
            kind: "move".into(),
            x: 0.1,
            y: 0.2,
            wheel: 0,
            dx: 3.5,
            dy: -2.0,
        };
        first.merge_move(PointerInput {
            kind: "move".into(),
            x: 0.7,
            y: 0.8,
            wheel: 0,
            dx: 7.0,
            dy: 4.5,
        });
        assert_eq!((first.dx, first.dy), (10.5, 2.5));
        assert_eq!((first.x, first.y), (0.7, 0.8));
    }
}
