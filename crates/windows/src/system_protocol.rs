//! Bounded, local-only broker protocol. Network permission remains in RemoteRuntime.
use crate::{Display, Input};
use serde::{Deserialize, Serialize};
pub const MAX_PACKET: usize = farsail_media::MAX_BYTES * 4 + 4096;
#[derive(Serialize, Deserialize)]
#[serde(tag = "op", deny_unknown_fields)]
pub enum Request {
    Displays {},
    Capture {
        display: u32,
        profile: u8,
        layout: u64,
        sequence: u64,
    },
    Input {
        input: Input,
        display: Option<Display>,
        layout: u64,
        desktop: Vec<u16>,
    },
    Release {},
}
#[derive(Serialize, Deserialize)]
#[serde(tag = "status", deny_unknown_fields)]
pub enum Response {
    Displays { displays: Vec<Display> },
    Frame { bytes: Vec<u8>, desktop: Vec<u16> },
    Empty,
    Ok,
    Reset,
    Error,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn protocol_preserves_desktop_witness_and_rejects_unrecognized_commands() {
        let request = Request::Input {
            input: Input::Key {
                vk: 0x1b,
                down: true,
                repeat: false,
            },
            display: None,
            layout: 7,
            desktop: "Winlogon".encode_utf16().collect(),
        };
        let bytes = serde_json::to_vec(&request).unwrap();
        assert!(bytes.len() < 4096);
        let decoded: Request = serde_json::from_slice(&bytes).unwrap();
        assert!(
            matches!(decoded, Request::Input { layout: 7, desktop, .. } if desktop == "Winlogon".encode_utf16().collect::<Vec<_>>())
        );
        for json in [
            r#"{"op":"Run","command":"cmd.exe"}"#,
            r#"{"op":"Release","extra":1}"#,
        ] {
            assert!(serde_json::from_str::<Request>(json).is_err());
        }
        let reply = Response::Frame {
            bytes: vec![0, 255, 17],
            desktop: vec![87],
        };
        let data = serde_json::to_vec(&reply).unwrap();
        assert!(data.len() < MAX_PACKET);
        assert!(
            matches!(serde_json::from_slice::<Response>(&data).unwrap(), Response::Frame { bytes, desktop } if bytes == [0,255,17] && desktop == [87])
        );
    }
}
