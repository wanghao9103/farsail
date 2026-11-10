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
    Displays {
        displays: Vec<Display>,
    },
    Frame {
        bytes: Vec<u8>,
        desktop: Vec<u16>,
    },
    Empty,
    Ok,
    Reset,
    Geometry,
    UnsupportedInput,
    InputDenied,
    Injection {
        inserted: u32,
        expected: u32,
        code: u32,
    },
    Error,
}
pub fn input_result(response: Response) -> crate::Result<()> {
    match response {
        Response::Ok => Ok(()),
        Response::Reset | Response::Geometry => Err(crate::Error::Geometry),
        Response::UnsupportedInput => Err(crate::Error::UnsupportedInput),
        Response::Injection {
            inserted,
            expected,
            code,
        } => Err(crate::Error::Injection {
            inserted,
            expected,
            code,
        }),
        _ => Err(crate::Error::InputDenied),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn input_errors_keep_geometry_and_unsupported_actions_separate_from_permission_failure() {
        assert!(matches!(
            input_result(Response::Geometry),
            Err(crate::Error::Geometry)
        ));
        assert!(matches!(
            input_result(Response::UnsupportedInput),
            Err(crate::Error::UnsupportedInput)
        ));
        assert!(matches!(
            input_result(Response::InputDenied),
            Err(crate::Error::InputDenied)
        ));
        assert!(matches!(
            input_result(Response::Injection {
                inserted: 1,
                expected: 2,
                code: 5
            }),
            Err(crate::Error::Injection {
                inserted: 1,
                expected: 2,
                code: 5
            })
        ));
    }
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
