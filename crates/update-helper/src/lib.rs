//! The privileged boundary accepts signed bytes, never a user-controlled file or command.
use base64::{Engine, engine::general_purpose::STANDARD};
use minisign_verify::{PublicKey, Signature};
use serde::Deserialize;
use std::io::{BufRead, Read};

pub const HELPER: &str = "/usr/libexec/farsail-update-helper";
pub const ACTION: &str = "app.farsail.update.install";
pub const MAX_PACKAGE: u64 = 128 * 1024 * 1024;
const CONFIG: &str = include_str!("../../../apps/desktop/src-tauri/tauri.conf.json");

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Header {
    signature: String,
}

pub fn read_request(reader: &mut impl BufRead) -> Result<(String, Vec<u8>), String> {
    let mut header = Vec::new();
    reader
        .take(16_385)
        .read_until(b'\n', &mut header)
        .map_err(err)?;
    if header.len() > 16_384 || header.last() != Some(&b'\n') {
        return Err("更新请求无效。".into());
    }
    let header: Header = serde_json::from_slice(&header).map_err(err)?;
    let mut bytes = Vec::new();
    reader
        .take(MAX_PACKAGE + 1)
        .read_to_end(&mut bytes)
        .map_err(err)?;
    if bytes.is_empty() || bytes.len() as u64 > MAX_PACKAGE {
        return Err("更新包大小无效。".into());
    }
    Ok((header.signature, bytes))
}

fn decode(value: &str) -> Result<String, String> {
    String::from_utf8(STANDARD.decode(value.trim()).map_err(err)?).map_err(err)
}

pub fn verify_package(bytes: &[u8], encoded_signature: &str) -> Result<String, String> {
    let config: serde_json::Value = serde_json::from_str(CONFIG).map_err(err)?;
    let key = config["plugins"]["updater"]["pubkey"]
        .as_str()
        .ok_or("缺少更新公钥。")?;
    verify_with_key(bytes, encoded_signature, key)
}

fn verify_with_key(bytes: &[u8], encoded_signature: &str, key: &str) -> Result<String, String> {
    let key = PublicKey::decode(&decode(key)?).map_err(err)?;
    let signature = Signature::decode(&decode(encoded_signature)?).map_err(err)?;
    key.verify(bytes, &signature, true).map_err(err)?;
    let version = signature
        .trusted_comment()
        .split('\t')
        .find_map(|field| field.strip_prefix("version:"))
        .ok_or("签名缺少版本信息。")?;
    let parsed = semver::Version::parse(version).map_err(err)?;
    if parsed.to_string() != version || !parsed.pre.is_empty() || !parsed.build.is_empty() {
        return Err("更新版本无效。".into());
    }
    Ok(version.to_owned())
}

pub fn validate_metadata(
    fields: &str,
    signed: &str,
    arch: &str,
    installed: &str,
) -> Result<(), String> {
    let parts: Vec<_> = fields.lines().collect();
    if parts != ["far-sail", signed, arch] {
        return Err("更新包名称、版本或架构不匹配。".into());
    }
    let incoming = semver::Version::parse(signed).map_err(err)?;
    let current = semver::Version::parse(installed.trim()).map_err(err)?;
    if incoming <= current {
        return Err("拒绝安装相同或更早版本。".into());
    }
    Ok(())
}

pub fn rule_text(username: &str) -> String {
    let user = serde_json::to_string(username)
        .unwrap()
        .replace('\u{2028}', "\\u2028")
        .replace('\u{2029}', "\\u2029");
    format!(
        "// FarSail signed updates for this account only.\npolkit.addRule(function(action, subject) {{\n  if (action.id === {action:?} && action.lookup(\"program\") === {helper:?} &&\n      subject.user === {user} && subject.local && subject.active) {{\n    return polkit.Result.YES;\n  }}\n}});\n",
        action = ACTION,
        helper = HELPER
    )
}

pub fn err(error: impl std::fmt::Display) -> String {
    error.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn request_is_bounded_and_cannot_supply_a_path_or_command() {
        let request = b"{\"signature\":\"abc\"}\nbytes";
        assert_eq!(
            read_request(&mut &request[..]).unwrap(),
            ("abc".into(), b"bytes".to_vec())
        );
        for request in [
            b"{\"signature\":\"abc\",\"path\":\"/tmp/file\"}\nx".as_slice(),
            b"{}\nx",
            b"{\"signature\":\"x\"}\n",
        ] {
            assert!(read_request(&mut &request[..]).is_err());
        }
        assert!(read_request(&mut &vec![b'x'; 16_385][..]).is_err());
    }
    #[test]
    fn metadata_rejects_foreign_package_architecture_version_and_replay() {
        assert!(
            validate_metadata("far-sail\n0.1.25\namd64\n", "0.1.25", "amd64", "0.1.24").is_ok()
        );
        for fields in [
            "other\n0.1.25\namd64",
            "far-sail\n0.1.24\namd64",
            "far-sail\n0.1.25\narm64",
            "far-sail\n0.1.25\namd64\nextra",
        ] {
            assert!(validate_metadata(fields, "0.1.25", "amd64", "0.1.24").is_err());
        }
        for current in ["0.1.25", "0.1.26", "invalid"] {
            assert!(
                validate_metadata("far-sail\n0.1.25\namd64", "0.1.25", "amd64", current).is_err()
            );
        }
    }
    #[test]
    fn signed_payload_is_verified_before_using_the_version() {
        let fixture: serde_json::Value =
            serde_json::from_str(include_str!("../tests/fixtures/signatures.json")).unwrap();
        let key = fixture["publicKey"].as_str().unwrap();
        let signature = fixture["signature"].as_str().unwrap();
        let bytes = b"FarSail helper test payload";
        assert_eq!(verify_with_key(bytes, signature, key).unwrap(), "0.1.25");
        assert!(verify_with_key(b"tampered payload", signature, key).is_err());
        let tampered = STANDARD.encode(
            decode(signature)
                .unwrap()
                .replace("version:0.1.25", "version:0.1.26"),
        );
        assert!(verify_with_key(bytes, &tampered, key).is_err());
        for field in ["missingVersion", "prerelease"] {
            assert!(verify_with_key(bytes, fixture[field].as_str().unwrap(), key).is_err());
        }
        assert!(
            verify_package(bytes, signature).is_err(),
            "Production helper must reject a test signing key"
        );
    }
    #[test]
    fn cannot_trust_unsigned_version_comments() {
        assert!(verify_package(b"anything", "not a signature").is_err());
    }
    #[test]
    fn username_is_quoted_in_the_rule() {
        let rule = rule_text("test\";return polkit.Result.YES;//");
        assert!(rule.contains("subject.user === \"test\\\";return polkit.Result.YES;//\""));
        assert!(rule.contains("subject.local && subject.active"));
    }
}
