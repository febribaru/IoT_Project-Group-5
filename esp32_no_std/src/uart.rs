//! UART Serial Communication Handler (GUI Command Reader & Log Output)

use esp_hal::uart::Uart;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SerialCommand {
    Start,
    Stop,
    Purge,
}

pub struct UartHandler;

impl UartHandler {
    /// Membaca stream UART secara non-blocking dan memeriksa perintah START/STOP/PURGE
    pub fn poll_command(
        uart: &mut Uart<'_, esp_hal::Blocking>,
        cmd_buf: &mut [u8; 64],
        cmd_len: &mut usize,
    ) -> Option<SerialCommand> {
        let mut rx_buf = [0u8; 1];
        while let Ok(1) = uart.read_buffered(&mut rx_buf) {
            let b = rx_buf[0];
            if b == b'\n' || b == b'\r' {
                if *cmd_len > 0 {
                    if let Ok(cmd_str) = core::str::from_utf8(&cmd_buf[..*cmd_len]) {
                        let command = cmd_str.trim();
                        *cmd_len = 0;
                        if command.eq_ignore_ascii_case("START") {
                            return Some(SerialCommand::Start);
                        } else if command.eq_ignore_ascii_case("STOP") {
                            return Some(SerialCommand::Stop);
                        } else if command.eq_ignore_ascii_case("PURGE") {
                            return Some(SerialCommand::Purge);
                        } else if command.len() > 5 && (command.starts_with("WIFI:") || command.starts_with("wifi:")) {
                            let rest = &command[5..];
                            if let Some(colon_pos) = rest.find(':') {
                                let ssid = &rest[..colon_pos];
                                let pass = &rest[colon_pos + 1..];
                                crate::iot::wifi::set_dynamic_wifi(ssid, pass);
                            } else {
                                crate::iot::wifi::set_dynamic_wifi(rest, "");
                            }
                        }
                    }
                    *cmd_len = 0;
                }
            } else if *cmd_len < cmd_buf.len() {
                cmd_buf[*cmd_len] = b;
                *cmd_len += 1;
            }
        }
        None
    }
}
