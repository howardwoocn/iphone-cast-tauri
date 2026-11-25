//! Reads actual iPhone playback state from Apple's Media Service (AMS).
//! Discovery runs separately so unavailable AMS cannot delay mouse input.
use std::{sync::{Arc, atomic::{AtomicBool, Ordering}}, thread, time::Duration};
use tauri::{AppHandle, Manager};
use windows::{
    core::{GUID, HSTRING, IInspectable, Result as WinResult},
    Devices::Bluetooth::{BluetoothCacheMode, BluetoothConnectionStatus, BluetoothLEDevice},
    Devices::Bluetooth::GenericAttributeProfile::{GattCharacteristic, GattValueChangedEventArgs, GattClientCharacteristicConfigurationDescriptorValue, GattCommunicationStatus, GattDeviceService, GattWriteOption},
    Foundation::TypedEventHandler,
    Storage::Streams::{DataReader, DataWriter},
    Win32::System::WinRT::{RoInitialize, RoUninitialize, RO_INIT_MULTITHREADED},
};

const AMS: GUID = GUID::from_u128(0x89d3502b_0f36_433a_8ef4_c502ad55f8dc);
const ENTITY_UPDATE: GUID = GUID::from_u128(0x2f7cabce_808d_411f_9a0c_bb92ba96c102);

pub fn playback_state(bytes: &[u8]) -> Option<&'static str> {
    if bytes.len() < 4 || bytes[0] != 0 || bytes[1] != 1 || bytes[2] != 0 { return None; }
    let value = std::str::from_utf8(&bytes[3..]).ok()?;
    let parts: Vec<_> = value.split(',').collect();
    if parts.len() != 3 { return None; }
    let rate: f64 = parts[1].parse().ok()?;
    let elapsed: f64 = parts[2].parse().ok()?;
    if !rate.is_finite() || !elapsed.is_finite() || elapsed < 0.0 { return None; }
    match parts[0] { "0" => Some("paused"), "1" | "2" | "3" => Some("playing"), _ => None }
}

pub fn phone_volume(bytes: &[u8]) -> Option<f64> {
    if bytes.len() < 4 || bytes[..3] != [0, 2, 0] { return None; }
    let value: f64 = std::str::from_utf8(&bytes[3..]).ok()?.parse().ok()?;
    (value.is_finite() && (0.0..=1.0).contains(&value)).then_some(value)
}

fn publish_volume(app: &AppHandle, generation: u64, volume: Option<f64>) {
    crate::update_status(app, |status| {
        if app.state::<crate::AppState>().media_generation.load(Ordering::SeqCst) == generation {
            status.media_volume = volume;
        }
    });
}

fn publish(app: &AppHandle, generation: u64, playback: &str) {
    crate::update_status(app, |status| {
        if app.state::<crate::AppState>().media_generation.load(Ordering::SeqCst) == generation {
            status.media_playback = playback.into();
            if playback == "unknown" { status.media_volume = None; }
        }
    });
}

pub struct MediaMonitor { stopped: Arc<AtomicBool>, app: AppHandle, generation: u64 }
impl MediaMonitor {
    pub fn start(app: AppHandle, device_id: String) -> Self {
        let generation = app.state::<crate::AppState>().media_generation.fetch_add(1, Ordering::SeqCst) + 1;
        publish(&app, generation, "unknown");
        let stopped = Arc::new(AtomicBool::new(false));
        let worker_stop = stopped.clone();
        let worker_app = app.clone();
        thread::spawn(move || {
            if unsafe { RoInitialize(RO_INIT_MULTITHREADED) }.is_err() { return; }
            while !worker_stop.load(Ordering::SeqCst) {
                let _ = subscribe(&worker_app, generation, &device_id, &worker_stop);
                publish(&worker_app, generation, "unknown");
                for _ in 0..100 {
                    if worker_stop.load(Ordering::SeqCst) { break; }
                    thread::sleep(Duration::from_millis(100));
                }
            }
            unsafe { RoUninitialize(); }
        });
        Self { stopped, app, generation }
    }
}
impl Drop for MediaMonitor {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::SeqCst);
        let current = self.app.state::<crate::AppState>().media_generation.compare_exchange(self.generation, self.generation + 1, Ordering::SeqCst, Ordering::SeqCst);
        if current.is_ok() { publish(&self.app, self.generation + 1, "unknown"); }
    }
}

struct Phone(BluetoothLEDevice);
impl Drop for Phone { fn drop(&mut self) { let _ = self.0.Close(); } }
struct Service(GattDeviceService);
impl Drop for Service { fn drop(&mut self) { let _ = self.0.Close(); } }

fn subscribe(app: &AppHandle, generation: u64, id: &str, stopped: &Arc<AtomicBool>) -> WinResult<()> {
    let phone = Phone(BluetoothLEDevice::FromIdAsync(&HSTRING::from(id))?.get()?);
    let services = phone.0.GetGattServicesForUuidWithCacheModeAsync(AMS, BluetoothCacheMode::Uncached)?.get()?;
    if services.Status()? != GattCommunicationStatus::Success || services.Services()?.Size()? == 0 { return Ok(()); }
    let service = Service(services.Services()?.GetAt(0)?);
    let chars = service.0.GetCharacteristicsForUuidAsync(ENTITY_UPDATE)?.get()?;
    if chars.Status()? != GattCommunicationStatus::Success || chars.Characteristics()?.Size()? == 0 { return Ok(()); }
    let entity = chars.Characteristics()?.GetAt(0)?;
    let invalidated = Arc::new(AtomicBool::new(false));
    let changed = invalidated.clone();
    let services_token = phone.0.GattServicesChanged(&TypedEventHandler::<BluetoothLEDevice, IInspectable>::new(move |_, _| {
        changed.store(true, Ordering::SeqCst); Ok(())
    }))?;
    let callback_app = app.clone();
    let callback_stop = stopped.clone();
    let token = entity.ValueChanged(&TypedEventHandler::<GattCharacteristic, GattValueChangedEventArgs>::new(move |_, args| {
        if callback_stop.load(Ordering::SeqCst) { return Ok(()); }
        if let Some(args) = args.as_ref() {
            let reader = DataReader::FromBuffer(&args.CharacteristicValue()?)?;
            let mut bytes = vec![0; reader.UnconsumedBufferLength()? as usize];
            reader.ReadBytes(&mut bytes)?;
            // Ignore unrelated attributes, but invalidate malformed playback data.
            if bytes.starts_with(&[0, 2]) {
                publish_volume(&callback_app, generation, phone_volume(&bytes));
            }
            if bytes.starts_with(&[0, 1]) {
                publish(&callback_app, generation, playback_state(&bytes).unwrap_or("unknown"));
            }
        }
        Ok(())
    }))?;
    let result = (|| -> WinResult<()> {
        let status = entity.WriteClientCharacteristicConfigurationDescriptorAsync(GattClientCharacteristicConfigurationDescriptorValue::Notify)?.get()?;
        if status != GattCommunicationStatus::Success { return Ok(()); }
        let writer = DataWriter::new()?;
        writer.WriteBytes(&[0, 1, 2])?; // Player / PlaybackInfo and Volume; includes initial values.
        let status = entity.WriteValueWithOptionAsync(&writer.DetachBuffer()?, GattWriteOption::WriteWithResponse)?.get()?;
        if status != GattCommunicationStatus::Success { return Ok(()); }
        while !stopped.load(Ordering::SeqCst) && !invalidated.load(Ordering::SeqCst)
            && phone.0.ConnectionStatus()? == BluetoothConnectionStatus::Connected {
            thread::sleep(Duration::from_millis(100));
        }
        Ok(())
    })();
    let _ = entity.RemoveValueChanged(token);
    let _ = phone.0.RemoveGattServicesChanged(services_token);
    // Closing the service releases its notification subscription.
    result
}

#[cfg(test)]
mod tests {
    use super::{playback_state, phone_volume};
    #[test]
    fn validates_phone_volume_feedback() {
        assert_eq!(phone_volume(b"\x00\x02\x000.625"), Some(0.625));
        assert_eq!(phone_volume(b"\x00\x02\x000"), Some(0.0));
        assert_eq!(phone_volume(b"\x00\x02\x001"), Some(1.0));
        for value in [b"\x00\x02\x00NaN".as_slice(), b"\x00\x02\x001.1", b"\x00\x02\x00-0.1", b"\x00\x02\x010.5", b"\x00\x01\x000.5"] { assert_eq!(phone_volume(value), None); }
    }
    #[test]
    fn decodes_real_ams_states_and_scanning() {
        assert_eq!(playback_state(b"\x00\x01\x000,0,12.5"), Some("paused"));
        for state in ["1", "2", "3"] {
            let mut value = vec![0, 1, 0]; value.extend_from_slice(format!("{state},1,12.5").as_bytes());
            assert_eq!(playback_state(&value), Some("playing"));
        }
    }
    #[test]
    fn rejects_unrelated_truncated_and_invalid_updates() {
        for value in [b"\x01\x01\x001,1,0".as_slice(), b"\x00\x01\x011,1,0", b"\x00\x01\x009,1,0", b"\x00\x01\x001,NaN,0", b"\x00\x01\x001,1,-1", b"\x00\x01\x001", b"\x00\x01\x00\xff"] {
            assert_eq!(playback_state(value), None);
        }
    }
}
