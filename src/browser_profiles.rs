use std::path::{Path, PathBuf};

/// Separator used to encode an optional profile selection onto a browser
/// path. Chosen as an ASCII control character (unit separator) since it
/// cannot appear in a real filesystem path or flatpak application ID, so
/// encoded specs stay silently compatible everywhere a plain browser path
/// is expected (rule matching, storage, spawning).
const PROFILE_SEPARATOR: char = '\u{1f}';

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowserProfile {
    pub id: String,
    pub display_name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BrowserFamily {
    Chromium,
    Gecko,
}

const CHROMIUM_NAMES: &[&str] = &[
    "chrome",
    "google-chrome",
    "google-chrome-stable",
    "chromium",
    "chromium-browser",
    "brave",
    "brave-browser",
    "microsoft-edge",
    "microsoft-edge-stable",
    "microsoft-edge-dev",
    "microsoft-edge-beta",
    "msedge",
    "vivaldi",
    "opera",
    "thorium-browser",
    "thorium",
];

const GECKO_NAMES: &[&str] = &["firefox", "firefox-esr", "librewolf", "floorp", "waterfox"];

fn executable_basename(browser_path: &str) -> Option<String> {
    // Flatpak sandboxing keeps profile storage layout implementation-defined
    // per app, so profile switching is intentionally not attempted there.
    if browser_path.starts_with("flatpak:") {
        return None;
    }
    Path::new(browser_path)
        .file_name()
        .and_then(|n| n.to_str())
        .map(|n| n.trim_end_matches(".exe").to_ascii_lowercase())
}

fn classify_family(basename: &str) -> Option<BrowserFamily> {
    if CHROMIUM_NAMES.contains(&basename) {
        Some(BrowserFamily::Chromium)
    } else if GECKO_NAMES.contains(&basename) {
        Some(BrowserFamily::Gecko)
    } else {
        None
    }
}

/// Returns the profiles a browser install exposes, or an empty vec when the
/// browser has no discoverable multi-profile setup (or its family isn't
/// covered by a documented profile-switching mechanism).
pub fn detect_profiles(browser_path: &str) -> Vec<BrowserProfile> {
    let Some(basename) = executable_basename(browser_path) else {
        return Vec::new();
    };
    match classify_family(&basename) {
        Some(BrowserFamily::Chromium) => detect_chromium_profiles(&basename),
        Some(BrowserFamily::Gecko) => detect_gecko_profiles(&basename),
        None => Vec::new(),
    }
}

fn chromium_user_data_dir(basename: &str) -> Option<PathBuf> {
    #[cfg(target_os = "linux")]
    {
        let config_dir = dirs::config_dir()?;
        let sub = match basename {
            "chrome" | "google-chrome" | "google-chrome-stable" => "google-chrome",
            "chromium" | "chromium-browser" => "chromium",
            "brave" | "brave-browser" => "BraveSoftware/Brave-Browser",
            "microsoft-edge"
            | "microsoft-edge-stable"
            | "microsoft-edge-dev"
            | "microsoft-edge-beta"
            | "msedge" => "microsoft-edge",
            "vivaldi" => "vivaldi",
            "opera" => "opera",
            "thorium-browser" | "thorium" => "Thorium",
            _ => return None,
        };
        Some(config_dir.join(sub))
    }
    #[cfg(target_os = "windows")]
    {
        let local_data = dirs::data_local_dir()?;
        let sub = match basename {
            "chrome" | "google-chrome" | "google-chrome-stable" => "Google/Chrome/User Data",
            "chromium" | "chromium-browser" => "Chromium/User Data",
            "brave" | "brave-browser" => "BraveSoftware/Brave-Browser/User Data",
            "microsoft-edge"
            | "microsoft-edge-stable"
            | "microsoft-edge-dev"
            | "microsoft-edge-beta"
            | "msedge" => "Microsoft/Edge/User Data",
            "vivaldi" => "Vivaldi/User Data",
            "opera" => "Opera Software/Opera Stable",
            "thorium-browser" | "thorium" => "Thorium/User Data",
            _ => return None,
        };
        Some(local_data.join(sub))
    }
    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        let _ = basename;
        None
    }
}

/// Parses a Chromium-family `Local State` file's `profile.info_cache` map
/// (documented internal structure Chromium itself uses to populate its
/// profile picker) into a profile list, with the `Default` directory first.
fn parse_local_state(content: &str) -> Vec<BrowserProfile> {
    let Ok(json) = serde_json::from_str::<serde_json::Value>(content) else {
        return Vec::new();
    };

    let Some(info_cache) = json
        .get("profile")
        .and_then(|p| p.get("info_cache"))
        .and_then(|c| c.as_object())
    else {
        return Vec::new();
    };

    let mut profiles: Vec<BrowserProfile> = info_cache
        .iter()
        .map(|(dir_name, info)| {
            let display_name = info
                .get("name")
                .and_then(|n| n.as_str())
                .filter(|n| !n.is_empty())
                .unwrap_or(dir_name)
                .to_string();
            BrowserProfile {
                id: dir_name.clone(),
                display_name,
            }
        })
        .collect();

    profiles.sort_by(|a, b| match (a.id == "Default", b.id == "Default") {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => a
            .display_name
            .to_lowercase()
            .cmp(&b.display_name.to_lowercase()),
    });

    profiles
}

fn detect_chromium_profiles(basename: &str) -> Vec<BrowserProfile> {
    let Some(user_data_dir) = chromium_user_data_dir(basename) else {
        return Vec::new();
    };
    let Ok(content) = std::fs::read_to_string(user_data_dir.join("Local State")) else {
        return Vec::new();
    };
    parse_local_state(&content)
}

fn gecko_profiles_ini_path(basename: &str) -> Option<PathBuf> {
    #[cfg(target_os = "linux")]
    {
        let home = dirs::home_dir()?;
        let sub = match basename {
            "firefox" | "firefox-esr" => ".mozilla/firefox",
            "librewolf" => ".librewolf",
            "floorp" => ".floorp",
            "waterfox" => ".waterfox",
            _ => return None,
        };
        Some(home.join(sub).join("profiles.ini"))
    }
    #[cfg(target_os = "windows")]
    {
        let roaming_app_data = dirs::config_dir()?;
        let sub = match basename {
            "firefox" | "firefox-esr" => "Mozilla/Firefox",
            "librewolf" => "LibreWolf",
            "floorp" => "Floorp",
            "waterfox" => "Waterfox",
            _ => return None,
        };
        Some(roaming_app_data.join(sub).join("profiles.ini"))
    }
    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        let _ = basename;
        None
    }
}

/// Minimal `profiles.ini` parser. Firefox-family profile files are plain INI
/// with one `[ProfileN]` section per profile (`Name=`, and on newer layouts
/// `Default=1` marking the default); a full ini crate would be overkill for
/// the two keys we need.
fn parse_profiles_ini(content: &str) -> Vec<BrowserProfile> {
    struct Section {
        is_profile: bool,
        name: Option<String>,
        is_default: bool,
    }

    let mut sections: Vec<Section> = Vec::new();
    let mut current: Option<Section> = None;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            if let Some(section) = current.take() {
                sections.push(section);
            }
            current = Some(Section {
                is_profile: trimmed.starts_with("[Profile"),
                name: None,
                is_default: false,
            });
            continue;
        }

        let Some(section) = current.as_mut() else {
            continue;
        };
        if let Some((key, value)) = trimmed.split_once('=') {
            match key.trim() {
                "Name" => section.name = Some(value.trim().to_string()),
                "Default" => section.is_default = value.trim() == "1",
                _ => {}
            }
        }
    }
    if let Some(section) = current.take() {
        sections.push(section);
    }

    let mut profiles: Vec<(String, bool)> = sections
        .into_iter()
        .filter(|s| s.is_profile)
        .filter_map(|s| s.name.map(|name| (name, s.is_default)))
        .collect();

    // Stable sort: keeps file order among equally-ranked entries, so when no
    // section is explicitly marked default the first profile in the file
    // (conventionally the original/default one) stays first.
    profiles.sort_by_key(|(_, is_default)| !*is_default);

    profiles
        .into_iter()
        .map(|(name, _)| BrowserProfile {
            id: name.clone(),
            display_name: name,
        })
        .collect()
}

fn detect_gecko_profiles(basename: &str) -> Vec<BrowserProfile> {
    let Some(ini_path) = gecko_profiles_ini_path(basename) else {
        return Vec::new();
    };
    let Ok(content) = std::fs::read_to_string(&ini_path) else {
        return Vec::new();
    };
    parse_profiles_ini(&content)
}

pub fn encode_browser_spec(browser_path: &str, profile_id: &str) -> String {
    format!("{browser_path}{PROFILE_SEPARATOR}{profile_id}")
}

pub fn decode_browser_spec(spec: &str) -> (&str, Option<&str>) {
    match spec.split_once(PROFILE_SEPARATOR) {
        Some((path, profile_id)) => (path, Some(profile_id)),
        None => (spec, None),
    }
}

/// Expands each detected browser into one selectable entry per profile when
/// its family exposes more than one. Browsers with a single (or
/// undetectable) profile are kept as a plain-path entry, so their behavior
/// and any existing saved rules referencing the bare path are unaffected.
pub fn expand_with_profiles(browsers: &[String]) -> Vec<String> {
    let mut expanded = Vec::with_capacity(browsers.len() * 2);
    for browser_path in browsers {
        let profiles = detect_profiles(browser_path);
        if profiles.len() <= 1 {
            expanded.push(browser_path.clone());
        } else {
            for profile in profiles {
                expanded.push(encode_browser_spec(browser_path, &profile.id));
            }
        }
    }
    expanded
}

/// Command-line arguments to append so a browser spec launches the selected
/// profile, using each family's documented profile switch. Firefox-family
/// browsers additionally need `-no-remote`: without it, launching with `-P`
/// while another instance is already running just forwards the URL to that
/// instance's existing profile instead of switching (documented in Mozilla's
/// profile-manager knowledge base).
pub fn profile_launch_args(browser_path: &str, profile_id: &str) -> Vec<String> {
    let Some(basename) = executable_basename(browser_path) else {
        return Vec::new();
    };
    match classify_family(&basename) {
        Some(BrowserFamily::Chromium) => vec![format!("--profile-directory={profile_id}")],
        Some(BrowserFamily::Gecko) => vec![
            "-P".to_string(),
            profile_id.to_string(),
            "-no-remote".to_string(),
        ],
        None => Vec::new(),
    }
}

fn profile_display_name(browser_path: &str, profile_id: &str) -> String {
    detect_profiles(browser_path)
        .into_iter()
        .find(|p| p.id == profile_id)
        .map(|p| p.display_name)
        .unwrap_or_else(|| profile_id.to_string())
}

/// Human-readable display name for a (possibly profile-encoded) browser
/// spec, e.g. "Google Chrome" or "Google Chrome — Work".
pub fn spec_display_name(spec: &str) -> String {
    let (base_path, profile_id) = decode_browser_spec(spec);
    let base_name = crate::browser_discovery::get_browser_name_from_path(base_path);
    match profile_id {
        Some(profile_id) => format!(
            "{} — {}",
            base_name,
            profile_display_name(base_path, profile_id)
        ),
        None => base_name,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_known_chromium_and_gecko_executables() {
        assert_eq!(classify_family("chrome"), Some(BrowserFamily::Chromium));
        assert_eq!(
            classify_family("microsoft-edge-stable"),
            Some(BrowserFamily::Chromium)
        );
        assert_eq!(classify_family("firefox"), Some(BrowserFamily::Gecko));
        assert_eq!(classify_family("librewolf"), Some(BrowserFamily::Gecko));
        assert_eq!(classify_family("some-unknown-browser"), None);
    }

    #[test]
    fn executable_basename_strips_exe_and_lowercases() {
        assert_eq!(
            executable_basename("C:/Program Files/Google/Chrome/Application/chrome.exe"),
            Some("chrome".to_string())
        );
        assert_eq!(
            executable_basename("/usr/bin/Firefox"),
            Some("firefox".to_string())
        );
        assert_eq!(executable_basename("flatpak:org.mozilla.firefox"), None);
    }

    #[test]
    fn encode_decode_round_trip() {
        let spec = encode_browser_spec("/usr/bin/chrome", "Profile 1");
        assert_eq!(
            decode_browser_spec(&spec),
            ("/usr/bin/chrome", Some("Profile 1"))
        );
        assert_eq!(
            decode_browser_spec("/usr/bin/chrome"),
            ("/usr/bin/chrome", None)
        );
    }

    #[test]
    fn parse_local_state_orders_default_first_and_reads_names() {
        let content = r#"{
            "profile": {
                "info_cache": {
                    "Profile 1": { "name": "Work" },
                    "Default": { "name": "Person 1" }
                }
            }
        }"#;
        let profiles = parse_local_state(content);
        assert_eq!(profiles.len(), 2);
        assert_eq!(profiles[0].id, "Default");
        assert_eq!(profiles[0].display_name, "Person 1");
        assert_eq!(profiles[1].id, "Profile 1");
        assert_eq!(profiles[1].display_name, "Work");
    }

    #[test]
    fn parse_local_state_handles_missing_or_malformed_data() {
        assert!(parse_local_state("not json").is_empty());
        assert!(parse_local_state(r#"{"profile": {}}"#).is_empty());
    }

    #[test]
    fn parse_profiles_ini_reads_names_and_orders_default_first() {
        let content = "\
[Profile1]
Name=Work
IsRelative=1
Path=Profiles/work.default

[Profile0]
Name=default
IsRelative=1
Path=Profiles/xxxx.default
Default=1

[General]
StartWithLastProfile=1
";
        let profiles = parse_profiles_ini(content);
        assert_eq!(profiles.len(), 2);
        assert_eq!(profiles[0].id, "default");
        assert_eq!(profiles[1].id, "Work");
    }

    #[test]
    fn profile_launch_args_match_documented_family_switches() {
        assert_eq!(
            profile_launch_args("/usr/bin/chrome", "Profile 1"),
            vec!["--profile-directory=Profile 1".to_string()]
        );
        assert_eq!(
            profile_launch_args("/usr/bin/firefox", "work"),
            vec![
                "-P".to_string(),
                "work".to_string(),
                "-no-remote".to_string()
            ]
        );
        assert!(profile_launch_args("/usr/bin/unknown-browser", "x").is_empty());
    }

    #[test]
    fn spec_display_name_falls_back_to_base_name_without_profile() {
        assert_eq!(spec_display_name("/usr/bin/firefox"), "Mozilla Firefox");
    }
}
