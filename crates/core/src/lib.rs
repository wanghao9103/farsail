//! Shared wire concepts. The coordinator owns persistence and policy decisions.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RemotePermission {
    View,
    Control,
    Files,
}

impl RemotePermission {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::View => "view",
            Self::Control => "control",
            Self::Files => "files",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DevicePlatform {
    Windows,
    Android,
    Ios,
}

impl DevicePlatform {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Windows => "windows",
            Self::Android => "android",
            Self::Ios => "ios",
        }
    }
}

pub const ACCESS_SECONDS: i64 = 15 * 60;
pub const REFRESH_SECONDS: i64 = 30 * 24 * 60 * 60;
pub const CHALLENGE_SECONDS: i64 = 5 * 60;
pub const DEVICE_LEASE_SECONDS: i64 = 60;
pub const GRANT_SECONDS: i64 = 30;
