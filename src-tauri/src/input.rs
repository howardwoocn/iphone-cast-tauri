//! Absolute pointer position and a separate relative mouse button report.
//! Coordinates are normalized against the visible video image, never the desktop.
pub const ABSOLUTE_MAX: u16 = 32767;
pub const ABSOLUTE_ID: u8 = 1;
pub const MOUSE_ID: u8 = 2;

// Standard HID mouse collections. Input reports exclude report ID; the GATT
// Report Reference descriptor identifies each characteristic.
pub const REPORT_MAP: &[u8] = &[
    0x05,
    0x01,
    0x09,
    0x02,
    0xA1,
    0x01,
    0x85,
    ABSOLUTE_ID,
    0x09,
    0x01,
    0xA1,
    0x00,
    0x05,
    0x09,
    0x19,
    0x01,
    0x29,
    0x03,
    0x15,
    0x00,
    0x25,
    0x01,
    0x75,
    0x01,
    0x95,
    0x03,
    0x81,
    0x02,
    0x75,
    0x05,
    0x95,
    0x01,
    0x81,
    0x01,
    0x05,
    0x01,
    0x09,
    0x30,
    0x09,
    0x31,
    0x15,
    0x00,
    0x26,
    0xFF,
    0x7F,
    0x75,
    0x10,
    0x95,
    0x02,
    0x81,
    0x02,
    0xC0,
    0xC0,
    0x05,
    0x01,
    0x09,
    0x02,
    0xA1,
    0x01,
    0x85,
    MOUSE_ID,
    0x09,
    0x01,
    0xA1,
    0x00,
    0x05,
    0x09,
    0x19,
    0x01,
    0x29,
    0x03,
    0x15,
    0x00,
    0x25,
    0x01,
    0x75,
    0x01,
    0x95,
    0x03,
    0x81,
    0x02,
    0x75,
    0x05,
    0x95,
    0x01,
    0x81,
    0x01,
    0x05,
    0x01,
    0x09,
    0x30,
    0x09,
    0x31,
    0x09,
    0x38,
    0x15,
    0x81,
    0x25,
    0x7F,
    0x75,
    0x08,
    0x95,
    0x03,
    0x81,
    0x06,
    0xC0,
    0xC0,
];

pub fn absolute_report(x: f64, y: f64) -> Option<[u8; 5]> {
    if !x.is_finite() || !y.is_finite() || !(0.0..=1.0).contains(&x) || !(0.0..=1.0).contains(&y) {
        return None;
    }
    let x = (x * ABSOLUTE_MAX as f64).round() as u16;
    let y = (y * ABSOLUTE_MAX as f64).round() as u16;
    Some([0, x as u8, (x >> 8) as u8, y as u8, (y >> 8) as u8])
}

#[cfg(test)]
mod tests {
    use super::*;
    fn event(s: &mut PointerState, kind: &str, x: f64, y: f64, dx: f64, dy: f64) -> Vec<Report> {
        s.apply(kind, x, y, 0, dx, dy)
    }
    #[test]
    fn absolute_edges_and_invalid_coordinates() {
        assert_eq!(absolute_report(0.0, 1.0), Some([0, 0, 0, 255, 127]));
        for (x, y) in [
            (f64::NAN, 0.0),
            (0.0, f64::INFINITY),
            (-0.1, 0.0),
            (1.01, 0.0),
        ] {
            assert!(absolute_report(x, y).is_none());
        }
    }
    #[test]
    fn click_positions_before_press() {
        let mut s = PointerState::default();
        let r = event(&mut s, "down", 0.5, 0.25, 0.0, 0.0);
        assert_eq!(r[0].id, ABSOLUTE_ID);
        assert_eq!(
            r[1],
            Report {
                id: MOUSE_ID,
                bytes: vec![1, 0, 0, 0],
                delay_ms: 45
            }
        );
    }
    #[test]
    fn drag_motion_and_buttons_share_the_same_report() {
        let mut s = PointerState::default();
        event(&mut s, "down", 0.1, 0.2, 0.0, 0.0);
        let r = event(&mut s, "move", 0.9, 0.8, 24.0, -19.0);
        assert_eq!(
            r,
            vec![Report {
                id: MOUSE_ID,
                bytes: vec![1, 24, (-19i8) as u8, 0],
                delay_ms: 0
            }]
        );
        assert!(r.iter().all(|r| r.id != ABSOLUTE_ID));
    }
    #[test]
    fn large_motion_is_split_without_losing_distance_or_releasing() {
        let mut s = PointerState::default();
        event(&mut s, "down", 0.1, 0.2, 0.0, 0.0);
        let r = event(&mut s, "move", 0.9, 0.8, 500.0, -300.0);
        assert_eq!(r.iter().map(|r| r.bytes[1] as i8 as i32).sum::<i32>(), 500);
        assert_eq!(r.iter().map(|r| r.bytes[2] as i8 as i32).sum::<i32>(), -300);
        assert!(r.iter().all(|r| r.id == MOUSE_ID
            && r.bytes[0] == 1
            && r.bytes[1] != 128
            && r.bytes[2] != 128));
    }
    #[test]
    fn fractional_dpi_motion_is_accumulated() {
        let mut s = PointerState::default();
        event(&mut s, "down", 0.1, 0.2, 0.0, 0.0);
        assert_eq!(
            event(&mut s, "move", 0.2, 0.2, 0.5, -0.5)[0].bytes,
            vec![1, 0, 0, 0]
        );
        assert_eq!(
            event(&mut s, "move", 0.2, 0.2, 0.5, -0.5)[0].bytes,
            vec![1, 1, 255, 0]
        );
    }
    #[test]
    fn stationary_hold_keeps_the_button_pressed() {
        let mut s = PointerState::default();
        assert!(event(&mut s, "hold", 0.0, 0.0, 0.0, 0.0).is_empty());
        event(&mut s, "down", 0.1, 0.2, 0.0, 0.0);
        for _ in 0..10 {
            assert_eq!(
                event(&mut s, "hold", 0.0, 0.0, 0.0, 0.0)[0].bytes,
                vec![1, 0, 0, 0]
            );
        }
    }
    #[test]
    fn release_flushes_the_final_motion_before_button_up() {
        let mut s = PointerState::default();
        event(&mut s, "down", 0.1, 0.2, 0.0, 0.0);
        let r = event(&mut s, "up", 0.9, 0.8, 3.0, 4.0);
        assert_eq!(r[0].bytes, vec![1, 3, 4, 0]);
        assert_eq!(r[1].bytes, vec![0; 4]);
        assert!(!s.is_pressed());
        assert_eq!(event(&mut s, "move", 0.9, 0.8, 0.0, 0.0)[0].id, ABSOLUTE_ID);
    }
    #[test]
    fn emergency_release_never_replays_stale_movement() {
        let mut s = PointerState::default();
        event(&mut s, "down", 0.1, 0.2, 0.0, 0.0);
        assert_eq!(
            event(&mut s, "release", f64::NAN, f64::NAN, 90.0, 90.0),
            vec![Report {
                id: MOUSE_ID,
                bytes: vec![0; 4],
                delay_ms: 0
            }]
        );
        assert!(event(&mut s, "hold", 0.0, 0.0, 0.0, 0.0).is_empty());
    }
    #[test]
    fn invalid_press_and_motion_cannot_corrupt_the_button_state() {
        let mut s = PointerState::default();
        assert!(event(&mut s, "down", 2.0, 0.0, 0.0, 0.0).is_empty());
        assert!(!s.is_pressed());
        event(&mut s, "down", 0.1, 0.2, 0.0, 0.0);
        assert!(event(&mut s, "move", 0.2, 0.2, f64::NAN, 3.0).is_empty());
        assert!(s.is_pressed());
    }
}

#[derive(Debug, PartialEq)]
pub struct Report {
    pub id: u8,
    pub bytes: Vec<u8>,
    pub delay_ms: u64,
}
#[derive(Default)]
pub struct PointerState {
    buttons: u8,
    remainder_x: f64,
    remainder_y: f64,
}
impl PointerState {
    pub fn is_pressed(&self) -> bool {
        self.buttons != 0
    }
    fn motion(&mut self, dx: f64, dy: f64) -> Vec<Report> {
        if !dx.is_finite() || !dy.is_finite() {
            return Vec::new();
        }
        self.remainder_x += dx.clamp(-32767.0, 32767.0);
        self.remainder_y += dy.clamp(-32767.0, 32767.0);
        let mut x = self.remainder_x.trunc() as i32;
        let mut y = self.remainder_y.trunc() as i32;
        self.remainder_x -= x as f64;
        self.remainder_y -= y as f64;
        let mut reports = Vec::new();
        while x != 0 || y != 0 {
            let sx = x.clamp(-127, 127);
            let sy = y.clamp(-127, 127);
            reports.push(Report {
                id: MOUSE_ID,
                bytes: vec![self.buttons, sx as i8 as u8, sy as i8 as u8, 0],
                delay_ms: 0,
            });
            x -= sx;
            y -= sy;
        }
        reports
    }
    pub fn apply(
        &mut self,
        kind: &str,
        x: f64,
        y: f64,
        wheel: i8,
        dx: f64,
        dy: f64,
    ) -> Vec<Report> {
        let mut reports = Vec::new();
        if kind == "move" && self.is_pressed() {
            let mut motion = self.motion(dx, dy);
            // Clamped edge motion and subpixel DPI motion still keep the press alive.
            if motion.is_empty() && dx.is_finite() && dy.is_finite() {
                motion.push(Report {
                    id: MOUSE_ID,
                    bytes: vec![self.buttons, 0, 0, 0],
                    delay_ms: 0,
                });
            }
            return motion;
        }
        if kind == "move" || (kind == "down" && !self.is_pressed()) {
            let Some(mut position) = absolute_report(x, y) else {
                return reports;
            };
            position[0] = self.buttons;
            reports.push(Report {
                id: ABSOLUTE_ID,
                bytes: position.to_vec(),
                delay_ms: 0,
            });
        }
        if kind == "down" {
            if self.is_pressed() {
                return reports;
            }
            self.buttons = 1;
            self.remainder_x = 0.0;
            self.remainder_y = 0.0;
            reports.push(Report {
                id: MOUSE_ID,
                bytes: vec![1, 0, 0, 0],
                delay_ms: 45,
            });
        } else if kind == "up" || kind == "release" {
            if kind == "up" && self.is_pressed() {
                reports.extend(self.motion(dx, dy));
            }
            self.buttons = 0;
            self.remainder_x = 0.0;
            self.remainder_y = 0.0;
            reports.push(Report {
                id: MOUSE_ID,
                bytes: vec![0; 4],
                delay_ms: 0,
            });
        } else if kind == "hold" && self.is_pressed() {
            reports.push(Report {
                id: MOUSE_ID,
                bytes: vec![self.buttons, 0, 0, 0],
                delay_ms: 0,
            });
        } else if kind == "wheel" {
            reports.push(Report {
                id: MOUSE_ID,
                bytes: vec![self.buttons, 0, 0, wheel as u8],
                delay_ms: 0,
            });
        }
        reports
    }
}
