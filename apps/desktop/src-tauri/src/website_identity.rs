//! Page-script identity read from a Camoufox Host session.
//!
//! The Host writes `observed.json` during launch. That file is observed
//! evidence from the probe page, not a verified product Gate.

use std::{fs, path::Path, time::SystemTime};

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
    let session_dir = state_root.join(session_id);
    // A fresh identity re-observation writes reobserved.json next to the
    // launch-time observed.json. The session's current website-visible
    // identity is the newest of the two observations, never a mixture.
    let launch = parse_observed_file(&session_dir.join("observed.json"), silo_id);
    let reobserved = parse_observed_file(&session_dir.join("reobserved.json"), silo_id);
    match (launch, reobserved) {
        (Some(launch), Some(reobserved)) => Some(if reobserved.observed_at > launch.observed_at {
            reobserved
        } else {
            launch
        }),
        (launch, reobserved) => launch.or(reobserved),
    }
}

pub fn load_latest_observation(
    state_root: &Path,
    silo_id: Uuid,
) -> Option<WebsiteIdentityObservation> {
    let entries = fs::read_dir(state_root).ok()?;
    let mut latest: Option<WebsiteIdentityObservation> = None;
    for entry in entries.flatten() {
        // Recovery paths without a live session must resolve each session the
        // same way load_session_observation does: the newest valid member of
        // that session's observed/reobserved pair. The Host names session
        // directories with uuid4().hex, so anything else on disk (quarantine,
        // logs, caches) never contributes an observation.
        let file_name = entry.file_name();
        let Some(session_id) = file_name.to_str() else {
            continue;
        };
        let Some(candidate) = load_session_observation(state_root, session_id, silo_id) else {
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

fn parse_observed_file(path: &Path, silo_id: Uuid) -> Option<WebsiteIdentityObservation> {
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
    let language = json_string(observed, "language").or_else(|| languages.first().cloned())?;
    let timezone =
        json_string(session, "timezone").or_else(|| json_string(observed, "timezone"))?;
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
        webdriver: observed
            .get("webdriver")
            .and_then(serde_json::Value::as_bool),
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

    const SESSION_LAUNCH_ONLY: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

    fn write_observed(dir: &Path, generated_at: &str, user_agent: &str) {
        write_observation(
            &dir.join(SESSION_LAUNCH_ONLY),
            "observed.json",
            generated_at,
            user_agent,
            "Asia/Shanghai",
        );
    }

    fn write_observation(
        session_dir: &Path,
        file_name: &str,
        generated_at: &str,
        user_agent: &str,
        timezone: &str,
    ) {
        fs::create_dir_all(session_dir).expect("session dir");
        fs::write(
            session_dir.join(file_name),
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
    "session": {{"timezone": "{timezone}", "utcOffsetMinutes": -480}}
  }}
}}"#
            ),
        )
        .expect("write observation file");
    }

    fn temp_state_root(label: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "verisilo-website-identity-{label}-{}",
            Uuid::new_v4().simple()
        ));
        fs::create_dir_all(&dir).expect("temp dir");
        dir
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
        let observation =
            load_session_observation(&dir, "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", silo_id)
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
        assert!(
            load_session_observation(&dir, "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", Uuid::nil())
                .is_none()
        );
        assert!(load_session_observation(&dir, "not-a-session", Uuid::nil()).is_none());
        let _ = fs::remove_dir_all(&dir);
    }

    fn observation_at(literal: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(literal)
            .expect("rfc3339 timestamp")
            .with_timezone(&Utc)
    }

    fn latest_user_agent(dir: &Path, silo_id: Uuid) -> String {
        load_latest_observation(dir, silo_id)
            .expect("a valid latest observation")
            .user_agent
    }

    #[test]
    fn latest_observation_returns_the_launch_observation_without_a_recheck() {
        let dir = temp_state_root("launch-only");
        write_observed(&dir, "2026-09-12T01:02:03Z", "launch user agent");
        let silo_id = Uuid::nil();
        let latest = load_latest_observation(&dir, silo_id).expect("launch observation");
        assert_eq!(latest.user_agent, "launch user agent");
        assert_eq!(latest.observed_at, observation_at("2026-09-12T01:02:03Z"));
        assert_eq!(latest.timezone, "Asia/Shanghai");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn latest_observation_prefers_a_newer_reobserved_observation() {
        let dir = temp_state_root("newer-recheck");
        write_observed(&dir, "2026-09-12T01:00:00Z", "launch user agent");
        write_observation(
            &dir.join(SESSION_LAUNCH_ONLY),
            "reobserved.json",
            "2026-09-12T02:00:00Z",
            "recheck user agent",
            "Europe/Berlin",
        );
        let silo_id = Uuid::nil();
        let session =
            load_session_observation(&dir, SESSION_LAUNCH_ONLY, silo_id).expect("session");
        assert_eq!(session.user_agent, "recheck user agent");
        assert_eq!(session.timezone, "Europe/Berlin");
        // The recovery path must surface exactly the session's current
        // observation, never a mixture of the launch and recheck fields.
        assert_eq!(
            load_latest_observation(&dir, silo_id).expect("latest"),
            session
        );
        assert_eq!(latest_user_agent(&dir, silo_id), "recheck user agent");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn latest_observation_keeps_the_launch_observation_when_the_recheck_is_older() {
        let dir = temp_state_root("older-recheck");
        write_observed(&dir, "2026-09-12T02:00:00Z", "launch user agent");
        write_observation(
            &dir.join(SESSION_LAUNCH_ONLY),
            "reobserved.json",
            "2026-09-12T01:00:00Z",
            "recheck user agent",
            "Europe/Berlin",
        );
        let silo_id = Uuid::nil();
        assert_eq!(
            load_session_observation(&dir, SESSION_LAUNCH_ONLY, silo_id)
                .expect("session")
                .user_agent,
            "launch user agent"
        );
        assert_eq!(latest_user_agent(&dir, silo_id), "launch user agent");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn latest_observation_selects_the_newest_per_session_before_comparing_sessions() {
        let dir = temp_state_root("cross-session");
        let earlier = dir.join("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
        let later = dir.join("bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb");
        write_observation(
            &earlier,
            "observed.json",
            "2026-09-12T01:00:00Z",
            "a launch",
            "Asia/Shanghai",
        );
        write_observation(
            &earlier,
            "reobserved.json",
            "2026-09-12T04:00:00Z",
            "a recheck",
            "Europe/Berlin",
        );
        write_observation(
            &later,
            "observed.json",
            "2026-09-12T02:00:00Z",
            "b launch",
            "Asia/Shanghai",
        );
        write_observation(
            &later,
            "reobserved.json",
            "2026-09-12T03:00:00Z",
            "b recheck",
            "America/New_York",
        );
        let silo_id = Uuid::nil();
        // Each session resolves to its own newest observation (a: 04:00,
        // b: 03:00), and the latest is the newest of those.
        let session_a = load_session_observation(&dir, "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa", silo_id)
            .expect("session a");
        assert_eq!(session_a.user_agent, "a recheck");
        let latest = load_latest_observation(&dir, silo_id).expect("latest");
        assert_eq!(latest, session_a);
        assert_eq!(latest.observed_at, observation_at("2026-09-12T04:00:00Z"));
        assert_eq!(latest.timezone, "Europe/Berlin");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn latest_observation_falls_back_to_observed_when_the_recheck_is_unusable() {
        let dir = temp_state_root("unusable-recheck");
        let silo_id = Uuid::nil();
        let empty_recheck = dir.join("cccccccccccccccccccccccccccccccc");
        write_observation(
            &empty_recheck,
            "observed.json",
            "2026-09-12T01:00:00Z",
            "c launch",
            "Asia/Shanghai",
        );
        fs::write(empty_recheck.join("reobserved.json"), []).expect("empty reobserved");
        let malformed_recheck = dir.join("dddddddddddddddddddddddddddddddd");
        write_observation(
            &malformed_recheck,
            "observed.json",
            "2026-09-12T02:00:00Z",
            "d launch",
            "Asia/Shanghai",
        );
        fs::write(
            malformed_recheck.join("reobserved.json"),
            r#"{"observedFull": {"#.as_bytes(),
        )
        .expect("malformed reobserved");
        let oversized_recheck = dir.join("eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee");
        write_observation(
            &oversized_recheck,
            "observed.json",
            "2026-09-12T03:00:00Z",
            "e launch",
            "Asia/Shanghai",
        );
        fs::write(
            oversized_recheck.join("reobserved.json"),
            vec![b' '; (MAX_OBSERVED_JSON_BYTES + 1) as usize],
        )
        .expect("oversized reobserved");
        // Each session honestly falls back to its own launch observation
        // instead of failing or picking up the unusable recheck file.
        for (session_id, user_agent) in [
            ("cccccccccccccccccccccccccccccccc", "c launch"),
            ("dddddddddddddddddddddddddddddddd", "d launch"),
            ("eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee", "e launch"),
        ] {
            assert_eq!(
                load_session_observation(&dir, session_id, silo_id)
                    .expect("launch fallback")
                    .user_agent,
                user_agent
            );
        }
        assert_eq!(latest_user_agent(&dir, silo_id), "e launch");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn latest_observation_uses_the_recheck_when_the_launch_observation_is_unusable() {
        let dir = temp_state_root("unusable-launch");
        let session_dir = dir.join("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
        fs::create_dir_all(&session_dir).expect("session dir");
        fs::write(session_dir.join("observed.json"), "not json at all").expect("malformed launch");
        write_observation(
            &session_dir,
            "reobserved.json",
            "2026-09-12T02:00:00Z",
            "recheck user agent",
            "Europe/Berlin",
        );
        let silo_id = Uuid::nil();
        assert_eq!(
            load_session_observation(&dir, SESSION_LAUNCH_ONLY, silo_id)
                .expect("recheck fallback")
                .user_agent,
            "recheck user agent"
        );
        assert_eq!(latest_user_agent(&dir, silo_id), "recheck user agent");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn latest_observation_is_none_without_a_valid_session_observation() {
        let dir = temp_state_root("no-valid-observation");
        let silo_id = Uuid::nil();
        assert!(load_latest_observation(&dir, silo_id).is_none());
        // The Host names session directories with uuid4().hex; any other entry
        // (a quarantine directory, a log file, a temp file) must never be
        // mistaken for a session and must not forge an identity attribution.
        let quarantine = dir.join("quarantine");
        write_observation(
            &quarantine,
            "observed.json",
            "2026-09-12T09:00:00Z",
            "forged",
            "Asia/Shanghai",
        );
        fs::write(dir.join("host-stderr.log"), b"log line").expect("log file");
        let broken_session = dir.join("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
        fs::create_dir_all(&broken_session).expect("session dir");
        fs::write(broken_session.join("observed.json"), "not json at all")
            .expect("malformed launch");
        fs::write(broken_session.join("reobserved.json"), "]\nbroken")
            .expect("malformed reobserved");
        assert!(load_latest_observation(&dir, silo_id).is_none());
        let _ = fs::remove_dir_all(&dir);
    }
}
