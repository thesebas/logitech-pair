//! Pair Logitech wireless headsets (H800, H600, UE3500) with their USB Nano
//! receiver (VID 0x046D, PID 0x0A29/0x0A2D).
//!
//! Protocol reverse-engineered from Logitech's 2012 "Wireless Headset
//! Pairing Assistant": the receiver's vendor HID interface (usage page
//! 0xFFA0) takes 3-byte output reports [0x02, CMD, 0x00] and answers with
//! [0x02, CMD, STATUS]. See the repo wiki for details.

use std::io::Write;
use std::process::ExitCode;
use std::time::{Duration, Instant};

use hidapi::HidApi;

const VENDOR: u16 = 0x046D;
const PRODUCTS: [u16; 2] = [0x0A29, 0x0A2D];
const USAGE_PAGE: u16 = 0xFFA0;

const OPEN_WINDOW: u8 = 0x08;
const POLL: u8 = 0x80;
const RESET: u8 = 0x01;

fn status_label(s: u8) -> Option<&'static str> {
    match s {
        0x04 => Some("pairing window open, waiting for a headset"),
        0x10 => Some("headset detected, linking..."),
        0x40 => Some("headset paired and connected"),
        _ => None,
    }
}

/// A `0x40` only means a *fresh* pairing if the window (0x04) or a linking
/// phase (0x10) was observed first — otherwise it is the pre-existing
/// connection of an already-paired headset.
fn pairing_done(armed: bool, status: u8) -> bool {
    armed && status == 0x40
}

fn arms(status: u8) -> bool {
    status == 0x04 || status == 0x10
}

struct Receiver {
    dev: hidapi::HidDevice,
    debug: bool,
}

impl Receiver {
    fn open(api: &HidApi, debug: bool) -> Result<Receiver, String> {
        let target = api
            .device_list()
            .find(|d| {
                d.vendor_id() == VENDOR
                    && PRODUCTS.contains(&d.product_id())
                    && d.usage_page() == USAGE_PAGE
            })
            .ok_or_else(|| {
                "ERROR: Logitech headset receiver not found. Plug in the USB \
                 Nano receiver and try again."
                    .to_string()
            })?;
        let dev = api.open_path(target.path()).map_err(|e| e.to_string())?;
        Ok(Receiver { dev, debug })
    }

    /// Send a 3-byte command and read the reply. Returns raw reply bytes.
    fn send(&self, cmd: u8) -> Result<Option<[u8; 64]>, String> {
        let report = [0x02, cmd, 0x00];
        let n = self.dev.write(&report).map_err(|e| e.to_string())?;
        if self.debug {
            println!("  [debug] -> {} ({n} bytes)", hex(&report));
        }
        let mut buf = [0u8; 64];
        let n = self
            .dev
            .read_timeout(&mut buf, 1000)
            .map_err(|e| e.to_string())?;
        if self.debug && n > 0 {
            println!("  [debug] <- {}", hex(&buf[..n]));
        }
        Ok(if n > 0 { Some(buf) } else { None })
    }

    /// Poll pairing status, report state transitions. Returns current status.
    fn poll(&mut self, state: &mut Option<u8>) -> Result<Option<u8>, String> {
        if let Some(reply) = self.send(POLL)? {
            if reply[0] == 0x02 {
                let new = reply[2];
                if Some(new) != *state {
                    if let Some(label) = status_label(new) {
                        println!("\n{:>8}  Dongle: {label}", stamp());
                    }
                    *state = Some(new);
                }
            }
        }
        Ok(*state)
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x} ")).collect()
}

/// Seconds elapsed since start, as "[  12s]" for log lines.
fn stamp() -> String {
    format!(
        "[{:>4}s]",
        START
            .get()
            .map(Instant::elapsed)
            .unwrap_or_default()
            .as_secs()
    )
}

use std::sync::OnceLock;
static START: OnceLock<Instant> = OnceLock::new();

fn main() -> ExitCode {
    START.set(Instant::now()).ok().unwrap();

    let mut seconds: u64 = 90;
    let mut debug = false;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--debug" => debug = true,
            "--seconds" => match args.next().and_then(|v| v.parse().ok()) {
                Some(v) => seconds = v,
                None => {
                    eprintln!("--seconds needs a number");
                    return ExitCode::from(2);
                }
            },
            other => {
                eprintln!("Usage: h800pair [--seconds N] [--debug]");
                eprintln!("Unknown argument: {other}");
                return ExitCode::from(2);
            }
        }
    }

    let api = match HidApi::new() {
        Ok(api) => api,
        Err(e) => {
            eprintln!("ERROR: HID init failed: {e}");
            return ExitCode::FAILURE;
        }
    };
    let mut rx = match Receiver::open(&api, debug) {
        Ok(rx) => rx,
        Err(e) => {
            println!("{e}");
            return ExitCode::FAILURE;
        }
    };
    println!("Found Logitech headset receiver.");

    let mut state: Option<u8> = None;
    if let Err(e) = rx.poll(&mut state) {
        println!("ERROR: cannot talk to the receiver: {e}");
        return ExitCode::FAILURE;
    }

    if let Err(e) = rx.send(OPEN_WINDOW) {
        println!("ERROR: cannot open the pairing window: {e}");
        return ExitCode::FAILURE;
    }
    println!("\nPairing window is OPEN.");
    println!(
        "While the window is open:\n  \
           1. Turn the headset ON.\n  \
           2. If this headset was paired before, set its device-selection \
           switch\n     to the OTHER position first - an already-paired slot \
           will NOT re-pair.\n  \
           3. Enter pairing mode (hold until the status light blinks fast):\n  \
                H800:   hold NEXT-TRACK + VOLUME-UP\n  \
                H600:   hold VOLUME-UP + MIC-MUTE\n  \
                UE3500: press and hold the POWER BUTTON for 9 seconds\n  \
           4. Keep the headset near the receiver until the light goes solid."
    );
    let _ = std::io::stdout().flush();

    let started = Instant::now();
    let mut armed = false;
    while started.elapsed().as_secs() < seconds {
        match rx.poll(&mut state) {
            Ok(Some(s)) => {
                armed = armed || arms(s);
                if pairing_done(armed, s) {
                    println!(
                        "\nSUCCESS: headset paired and connected ({}s).",
                        started.elapsed().as_secs()
                    );
                    return ExitCode::SUCCESS;
                }
            }
            Ok(_) => {}
            Err(e) => {
                println!("ERROR: lost contact with the receiver: {e}");
                return ExitCode::FAILURE;
            }
        }
        let nap = if state == Some(0x10) { 1 } else { 3 };
        std::thread::sleep(Duration::from_secs(nap));
    }

    println!(
        "\nERROR: no headset paired within {seconds}s. Make sure the headset \
         has battery charge and that you follow the pairing ritual above for \
         your model, then retry."
    );
    ExitCode::FAILURE
}

impl Drop for Receiver {
    fn drop(&mut self) {
        // Close the pairing window, best effort.
        let _ = self.dev.write(&[0x02, RESET, 0x00]);
        if self.debug {
            println!("  [debug] -> 02 01 00 (window closed)");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{arms, pairing_done, status_label};

    #[test]
    fn decodes_known_status_bytes() {
        assert_eq!(
            status_label(0x04),
            Some("pairing window open, waiting for a headset")
        );
        assert_eq!(status_label(0x10), Some("headset detected, linking..."));
        assert_eq!(status_label(0x40), Some("headset paired and connected"));
    }

    #[test]
    fn unknown_status_bytes_have_no_label() {
        assert_eq!(status_label(0x00), None);
        assert_eq!(status_label(0x20), None);
        assert_eq!(status_label(0xFF), None);
    }

    #[test]
    fn success_requires_armed_state() {
        // Pre-existing connection without a window: not a fresh pairing.
        assert!(!pairing_done(false, 0x40));
        // Window seen, then linked: success.
        assert!(pairing_done(true, 0x40));
        // Window open or linking alone never finishes.
        assert!(!pairing_done(true, 0x04));
        assert!(!pairing_done(true, 0x10));
        // These statuses arm the success condition.
        assert!(arms(0x04));
        assert!(arms(0x10));
        assert!(!arms(0x40));
        assert!(!arms(0x20));
    }
}
