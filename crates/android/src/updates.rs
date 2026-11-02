//! Signed Android release metadata. This trust key is independent of account/server settings.
use base64::{Engine, engine::general_purpose::STANDARD};
use ed25519_dalek::{Signature, VerifyingKey};
use serde::Deserialize;
use serde_json::{Value, json};

const PUBLIC_KEY: &str = include_str!("update-public-key.hex");
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    payload: String,
    signature: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Release {
    package_name: String,
    version: String,
    version_code: u64,
    url: String,
    sha256: String,
    size: u64,
    notes: String,
}

pub fn verify(envelope: &str, installed: u64) -> Result<Value, String> {
    let key: [u8; 32] = hex::decode(PUBLIC_KEY.trim())
        .map_err(|_| "更新公钥无效。")?
        .try_into()
        .map_err(|_| "更新公钥无效。")?;
    verify_key(envelope, installed, &key)
}
fn verify_key(envelope: &str, installed: u64, key: &[u8; 32]) -> Result<Value, String> {
    if envelope.len() > 32_768 {
        return Err("更新清单过大。".into());
    }
    let envelope: Envelope = serde_json::from_str(envelope).map_err(|_| "更新清单无效。")?;
    let payload = STANDARD
        .decode(envelope.payload)
        .map_err(|_| "更新清单无效。")?;
    if payload.len() > 16_384 {
        return Err("更新清单过大。".into());
    }
    let signature = STANDARD
        .decode(envelope.signature)
        .map_err(|_| "更新签名无效。")?;
    let signature = Signature::from_slice(&signature).map_err(|_| "更新签名无效。")?;
    VerifyingKey::from_bytes(key)
        .map_err(|_| "更新公钥无效。")?
        .verify_strict(&payload, &signature)
        .map_err(|_| "更新签名验证失败。")?;
    let r: Release = serde_json::from_slice(&payload).map_err(|_| "更新内容无效。")?;
    let version: Vec<_> = r.version.split('.').collect();
    if version.len() != 3
        || !version.iter().all(|p| {
            !p.is_empty()
                && p.len() <= 6
                && p.bytes().all(|b| b.is_ascii_digit())
                && (p.len() == 1 || !p.starts_with('0'))
        })
        || r.version_code == 0
        || r.version_code > i32::MAX as u64
        || r.package_name != "app.farsail.android"
        || r.url
            != format!(
                "https://github.com/wanghao9103/farsail/releases/download/android-v{}/FarSail_Android_{}_universal.apk",
                r.version, r.version
            )
        || r.sha256.len() != 64
        || !r
            .sha256
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        || r.size == 0
        || r.size > 192 * 1024 * 1024
        || r.notes.len() > 2000
    {
        return Err("更新版本、来源或摘要无效。".into());
    }
    Ok(
        json!({"available":r.version_code>installed,"version":r.version,"versionCode":r.version_code,"url":r.url,"sha256":r.sha256,"size":r.size,"notes":r.notes}),
    )
}
#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};
    fn metadata() -> Value {
        json!({"packageName":"app.farsail.android","version":"0.1.1","versionCode":2,"url":"https://github.com/wanghao9103/farsail/releases/download/android-v0.1.1/FarSail_Android_0.1.1_universal.apk","sha256":"ab".repeat(32),"size":4000,"notes":"test fixture"})
    }
    fn sign(value: &Value) -> (String, [u8; 32]) {
        let key = SigningKey::from_bytes(&[17; 32]);
        let bytes = serde_json::to_vec(value).unwrap();
        (json!({"payload":STANDARD.encode(&bytes),"signature":STANDARD.encode(key.sign(&bytes).to_bytes())}).to_string(),key.verifying_key().to_bytes())
    }
    #[test]
    fn signed_versions_are_monotonic_and_payload_is_authenticated() {
        let (raw, key) = sign(&metadata());
        assert_eq!(verify_key(&raw, 1, &key).unwrap()["available"], true);
        assert_eq!(verify_key(&raw, 2, &key).unwrap()["available"], false);
        assert_eq!(verify_key(&raw, 3, &key).unwrap()["available"], false);
        let mut forged: Value = serde_json::from_str(&raw).unwrap();
        let mut bytes = STANDARD
            .decode(forged["payload"].as_str().unwrap())
            .unwrap();
        bytes[4] ^= 1;
        forged["payload"] = json!(STANDARD.encode(bytes));
        assert!(verify_key(&forged.to_string(), 1, &key).is_err());
        assert!(
            verify_key(
                &raw,
                1,
                &SigningKey::from_bytes(&[18; 32]).verifying_key().to_bytes()
            )
            .is_err()
        );
    }
    #[test]
    fn signed_metadata_cannot_escape_android_release_boundary() {
        for (field, value) in [
            ("url", json!("https://evil.invalid/a.apk")),
            ("packageName", json!("other.app")),
            ("size", json!(300 * 1024 * 1024u64)),
            ("sha256", json!("ab")),
            ("version", json!("0.1.1-beta")),
            ("versionCode", json!(0)),
        ] {
            let mut m = metadata();
            m[field] = value;
            let (raw, key) = sign(&m);
            assert!(verify_key(&raw, 1, &key).is_err(), "{field}");
        }
        let mut m = metadata();
        m["command"] = json!("not allowed");
        let (raw, key) = sign(&m);
        assert!(verify_key(&raw, 1, &key).is_err());
    }
}
