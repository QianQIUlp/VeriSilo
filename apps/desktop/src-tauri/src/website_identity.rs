//! Page-script identity read from a Camoufox Host session.
//!
//! The Host writes `observed.json` during launch. That file is observed
//! evidence from the probe page, not a verified product Gate.

use std::{
    fs,
    path::Path,
    time::SystemTime,
};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

const MAX_OBSERVED_JSON_BYTES: u64 = 8 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WebsiteIdentitySource {
    PageScript,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebsiteIdentityObservation {
    pub silo_id: Uuid,
    pub observed_at: DateTime<Utc>,
    pub source: WebsiteIdentitySource,
    pub user_agent: String,
    pub language: String,
    pub languages: Vec<String>,
    pub platform: String,
    pub oscpu: Option<String>,
    pub timezone: String,
    pub screen_width: u32,
    pub screen_height: u32,
    pub color_depth: Option<u32>,
    pub device_pixel_ratio: Option<f64>,
    pub hardware_concurrency: u32,
    pub webgl_vendor: String,
    pub webgl_renderer: String,
    pub do_not_track: Option<String>,
    pub max_touch_points: Option<u32>,
    pub webdriver: Option<bool>,
}

pub fn load_session_observation(
    state_root: &Path,
    session_id: &str,
    silo_id: Uuid,
) -> Option<WebsiteIdentityObservation> {
    if !valid_session_id(session_id) {
        return None;
    }
    parse_observed_file(&state_root.join(session_id).join("observed.json"), silo_id)
}

pub fn load_latest_observation(
    state_root: &Path,
    silo_id: Uuid,
) -> Option<WebsiteIdentityObservation> {
    let entries = fs::read_dir(state_root).ok()?;
    let mut latest: Option<WebsiteIdentityObservation> = None;
    for entry in entries.flatten() {
        let path = entry.path().join("observed.json");
        let Some(candidate) = parse_observed_file(&path, silo_id) else {
            continue;
        };
        if latest
            .as_ref()
            .is_none_or(|current| candidate.observed_at >= current.observed_at)
        {
            latest = Some(candidate);
        }
    }
    latest
}

fn valid_session_id(session_id: &str) -> bool {
    session_id.len() == 32
        && session_id
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

fn parse_observed_file(
    path: &Path,
    silo_id: Uuid,
) -> Option<WebsiteIdentityObservation> {
    let metadata = regular_file_metadata(path)?;
    if metadata.len() == 0 || metadata.len() > MAX_OBSERVED_JSON_BYTES {
        return None;
    }
    let raw = fs::read(path).ok()?;
    let value: serde_json::Value = serde_json::from_slice(&raw).ok()?;
    let observed = value.get("observedFull").unwrap_or(&value);
    let user_agent = json_string(observed, "userAgent")?;
    if user_agent.is_empty() {
        return None;
    }
    let screen = observed.get("screen")?;
    let session = observed.get("session").unwrap_or(&serde_json::Value::Null);
    let languages = json_string_list(observed.get("languages"));
    let language = json_string(observed, "language")
        .or_else(|| languages.first().cloned())?;
    let timezone = json_string(session, "timezone")
        .or_else(|| json_string(observed, "timezone"))?;
    let (screen_width, screen_height) = screen_size(screen)?;
    Some(WebsiteIdentityObservation {
        silo_id,
        observed_at: json_time(value.get("generatedAtUtc"))
            .or_else(|| file_modified_at(&metadata))
            .unwrap_or_else(Utc::now),
        source: WebsiteIdentitySource::PageScript,
        user_agent,
        language,
        languages,
        platform: json_string(observed, "platform").unwrap_or_else(|| "Win32".to_owned()),
        oscpu: json_string(observed, "oscpu"),
        timezone,
        screen_width,
        screen_height,
        color_depth: json_u32(screen, "colorDepth"),
        device_pixel_ratio: json_f64(observed, "devicePixelRatio"),
        hardware_concurrency: json_u32(observed, "hardwareConcurrency").unwrap_or(0),
        webgl_vendor: json_string(observed, "webglVendor")
            .or_else(|| json_string(observed, "webgl2Vendor"))
            .unwrap_or_else(|| "未读到".to_owned()),
        webgl_renderer: json_string(observed, "webglRenderer")
            .or_else(|| json_string(observed, "webgl2Renderer"))
            .unwrap_or_else(|| "未读到".to_owned()),
        do_not_track: json_string(observed, "doNotTrack"),
        max_touch_points: json_u32(observed, "maxTouchPoints"),
        webdriver: observed.get("webdriver").and_then(serde_json::Value::as_bool),
    })
}

fn regular_file_metadata(path: &Path) -> Option<fs::Metadata> {
    let metadata = fs::symlink_metadata(path).ok()?;
    metadata.file_type().is_file().then_some(metadata)
}

fn file_modified_at(metadata: &fs::Metadata) -> Option<DateTime<Utc>> {
    let modified = metadata.modified().ok()?;
    let duration = modified.duration_since(SystemTime::UNIX_EPOCH).ok()?;
    DateTime::<Utc>::from_timestamp(
        i64::try_from(duration.as_secs()).ok()?,
        duration.subsec_nanos(),
    )
}

fn screen_size(screen: &serde_json::Value) -> Option<(u32, u32)> {
    if let Some(values) = screen.as_array() {
        return Some((
            values.first().and_then(json_u32_value)?,
            values.get(1).and_then(json_u32_value)?,
        ));
    }
    Some((json_u32(screen, "width")?, json_u32(screen, "height")?))
}

fn json_string(value: &serde_json::Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_owned)
}

fn json_string_list(value: Option<&serde_json::Value>) -> Vec<String> {
    value
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_owned)
        .collect()
}

fn json_u32(value: &serde_json::Value, key: &str) -> Option<u32> {
    value.get(key).and_then(json_u32_value)
}

fn json_u32_value(value: &serde_json::Value) -> Option<u32> {
    value
        .as_u64()
        .and_then(|number| u32::try_from(number).ok())
        .or_else(|| {
            value
                .as_f64()
                .filter(|number| number.is_finite() && *number >= 0.0)
                .and_then(|number| u32::try_from(number.round() as u64).ok())
        })
}

fn json_f64(value: &serde_json::Value, key: &str) -> Option<f64> {
    value
        .get(key)
        .and_then(serde_json::Value::as_f64)
        .filter(|number| number.is_finite())
}

fn json_time(value: Option<&serde_json::Value>) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value?.as_str()?)
        .ok()
        .map(|time| time.with_timezone(&Utc))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_observed(dir: &Path, generated_at: &str, user_agent: &str) {
        let session = dir.join("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
        fs::create_dir_all(&session).expect("session dir");
        fs::write(
            session.join("observed.json"),
            format!(
                r#"{{
  "generatedAtUtc": "{generated_at}",
  "observedFull": {{
    "userAgent": "{user_agent}",
    "language": "zh-CN",
    "languages": ["zh-CN", "zh"],
    "platform": "Win32",
    "oscpu": "Windows NT 10.0; Win64; x64",
    "doNotTrack": "unspecified",
    "screen": {{"width": 1920, "height": 1080, "colorDepth": 24}},
    "devicePixelRatio": 1,
    "hardwareConcurrency": 8,
    "webglVendor": "Google Inc. (NVIDIA)",
    "webglRenderer": "ANGLE (NVIDIA, NVIDIA GeForce GTX 1080 Direct3D11 vs_5_0 ps_5_0)",
    "maxTouchPoints": 0,
    "webdriver": false,
    "session": {{"timezone": "Asia/Shanghai", "utcOffsetMinutes": -480}}
  }}
}}"#
            ),
        )
        .expect("write observed.json");
    }

    #[test]
    fn reads_page_script_identity_from_host_observed_json() {
        let dir = std::env::temp_dir().join(format!(
            "verisilo-website-identity-{}",
            Uuid::new_v4().simple()
        ));
        fs::create_dir_all(&dir).expect("temp dir");
        write_observed(
            &dir,
            "2026-09-04T01:02:03Z",
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:142.0) Gecko/20100101 Firefox/142.0",
        );
        let silo_id = Uuid::nil();
        let observation = load_session_observation(
            &dir,
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            silo_id,
        )
        .expect("observation");
        assert_eq!(observation.source, WebsiteIdentitySource::PageScript);
        assert_eq!(observation.language, "zh-CN");
        assert_eq!(observation.timezone, "Asia/Shanghai");
        assert_eq!(observation.screen_width, 1920);
        assert_eq!(observation.hardware_concurrency, 8);
        assert_eq!(observation.webdriver, Some(false));
        assert_eq!(
            load_latest_observation(&dir, silo_id)
                .expect("latest")
                .user_agent,
            observation.user_agent
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn rejects_missing_or_oversized_observed_files() {
        let dir = std::env::temp_dir().join(format!(
            "verisilo-website-identity-missing-{}",
            Uuid::new_v4().simple()
        ));
        fs::create_dir_all(&dir).expect("temp dir");
        assert!(load_session_observation(
            &dir,
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            Uuid::nil()
        )
        .is_none());
        assert!(load_session_observation(&dir, "not-a-session", Uuid::nil()).is_none());
        let _ = fs::remove_dir_all(&dir);
    }
}
