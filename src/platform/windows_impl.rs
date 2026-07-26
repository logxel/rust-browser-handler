use super::PlatformHandler;
use log::{info, warn};
use std::collections::HashSet;
use std::env;
use std::io;
use std::path::PathBuf;
use winreg::{RegKey, enums::*};

const PROG_ID: &str = "rust-browser-handler";
const CANONICAL_NAME: &str = "rust_browser_handler.exe";

/// Helper function to extract executable path from a command string
fn extract_executable_path_from_command(command: String) -> Option<String> {
    let trimmed_command = command.trim();
    if let Some(stripped) = trimmed_command.strip_prefix('"') {
        if let Some(end_quote_index) = stripped.find('"') {
            let path = &stripped[..end_quote_index];
            Some(path.to_string())
        } else {
            None
        }
    } else {
        trimmed_command
            .split_whitespace()
            .next()
            .map(|s| s.to_string())
    }
}

/// Helper function to check if a path likely points to a browser executable
fn is_browser_executable(path: &str) -> bool {
    let lower_path = path.to_ascii_lowercase();
    // Only check the file name part
    if let Some(file_name) = std::path::Path::new(&lower_path)
        .file_name()
        .and_then(|n| n.to_str())
    {
        get_browser_map().contains_key(file_name)
    } else {
        false
    }
}

/// Mapping from executable file names to official browser names
fn get_browser_map() -> &'static std::collections::HashMap<&'static str, &'static str> {
    use std::collections::HashMap;
    use std::sync::OnceLock;

    static BROWSER_NAME_MAP: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();
    BROWSER_NAME_MAP.get_or_init(|| {
        let mut m = HashMap::new();
        m.insert("chrome.exe", "Google Chrome");
        m.insert("firefox.exe", "Mozilla Firefox");
        m.insert("msedge.exe", "Microsoft Edge");
        m.insert("brave.exe", "Brave");
        m.insert("opera.exe", "Opera");
        m.insert("launcher.exe", "Opera"); // Opera's launcher
        m.insert("vivaldi.exe", "Vivaldi");
        m.insert("thorium.exe", "Thorium");
        m.insert("librewolf.exe", "LibreWolf");
        m.insert("waterfox.exe", "Waterfox");
        m.insert("floorp.exe", "Floorp");
        m
    })
}

pub struct WindowsHandler;

impl PlatformHandler for WindowsHandler {
    fn find_browsers(&self) -> Vec<String> {
        let mut browsers = HashSet::new();

        // Check common installation paths first
        for path in generate_common_browser_paths() {
            if path.exists()
                && let Some(path_str) = path.to_str()
            {
                browsers.insert(crate::browser_discovery::normalize_path(path_str));
            }
        }

        // Check registry
        let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);

        let registry_paths = [
            (&hklm, "SOFTWARE\\Clients\\StartMenuInternet"),
            (
                &hklm,
                "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\App Paths",
            ),
            (&hkcu, "SOFTWARE\\Clients\\StartMenuInternet"),
            (
                &hkcu,
                "SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\App Paths",
            ),
        ];

        for (hive, path) in &registry_paths {
            match hive.open_subkey(path) {
                Ok(base_key) => {
                    for entry_result in base_key.enum_keys() {
                        match entry_result {
                            Ok(entry) => {
                                match base_key.open_subkey(&entry) {
                                    Ok(entry_key) => {
                                        // Check for shell/open/command
                                        if let Ok(command_key) =
                                            entry_key.open_subkey("shell\\open\\command")
                                            && let Ok(command_string) =
                                                command_key.get_value::<String, _>("")
                                            && let Some(executable_path) =
                                                extract_executable_path_from_command(command_string)
                                            && !executable_path.is_empty()
                                            && is_browser_executable(&executable_path)
                                        {
                                            browsers.insert(
                                                crate::browser_discovery::normalize_path(
                                                    &executable_path,
                                                ),
                                            );
                                        }

                                        // Check direct value
                                        if let Ok(command_string) =
                                            entry_key.get_value::<String, _>("")
                                            && let Some(executable_path) =
                                                extract_executable_path_from_command(command_string)
                                            && !executable_path.is_empty()
                                            && is_browser_executable(&executable_path)
                                        {
                                            browsers.insert(
                                                crate::browser_discovery::normalize_path(
                                                    &executable_path,
                                                ),
                                            );
                                        }
                                    }
                                    Err(e) => {
                                        warn!("Failed to open registry entry '{}': {}", entry, e)
                                    }
                                }
                            }
                            Err(e) => warn!("Failed to enumerate registry entry: {}", e),
                        }
                    }
                }
                Err(e) => warn!("Failed to open registry path '{}': {}", path, e),
            }
        }

        browsers.into_iter().collect()
    }

    fn set_as_default_handler(&self) -> Result<(), Box<dyn std::error::Error>> {
        info!("Attempting to register as default browser handler...");

        let exe_path = env::current_exe()?
            .to_str()
            .ok_or_else(|| io::Error::other("Failed to get executable path"))?
            .to_string();

        // Register custom URL scheme (rust-browser-handler)
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let classes_key = hkcu.open_subkey_with_flags("Software\\Classes", KEY_ALL_ACCESS)?;

        let scheme_key = classes_key.create_subkey("rust-browser-handler")?;
        scheme_key
            .0
            .set_value("", &"URL:Rust Browser Handler Protocol".to_string())?;
        scheme_key.0.set_value("URL Protocol", &"")?;

        let default_icon_key = scheme_key.0.create_subkey("DefaultIcon")?;
        default_icon_key
            .0
            .set_value("", &format!("{},0", exe_path))?;

        let command_key = scheme_key.0.create_subkey("shell\\open\\command")?;
        command_key
            .0
            .set_value("", &format!("\"{}\" \"%1\"", exe_path))?;

        // Register as a capable application
        let registered_apps_key =
            hkcu.open_subkey_with_flags("Software\\RegisteredApplications", KEY_ALL_ACCESS)?;
        registered_apps_key.set_value(
            CANONICAL_NAME,
            &format!(
                "Software\\Clients\\StartMenuInternet\\{}\\Capabilities",
                PROG_ID
            ),
        )?;

        let capabilities_key = hkcu.create_subkey(format!(
            "Software\\Clients\\StartMenuInternet\\{}\\Capabilities",
            PROG_ID
        ))?;
        capabilities_key.0.set_value(
            "ApplicationDescription",
            &"Handles URLs based on defined rules.",
        )?;
        capabilities_key
            .0
            .set_value("ApplicationIcon", &format!("{},0", exe_path))?;
        capabilities_key
            .0
            .set_value("ApplicationName", &"Rust Browser Handler")?;

        let url_associations_key = capabilities_key.0.create_subkey("URLAssociations")?;
        url_associations_key.0.set_value("http", &PROG_ID)?;
        url_associations_key.0.set_value("https", &PROG_ID)?;

        // Set as default for Start Menu Internet
        let start_menu_internet_key = hkcu.create_subkey("Software\\Clients\\StartMenuInternet")?;
        start_menu_internet_key.0.set_value("", &PROG_ID)?;

        // Set ProgIDs for http and https to make it the default handler
        let http_key = hkcu.create_subkey("Software\\Classes\\http")?;
        http_key.0.set_value("", &"rust-browser-handler")?;
        let https_key = hkcu.create_subkey("Software\\Classes\\https")?;
        https_key.0.set_value("", &"rust-browser-handler")?;

        info!("Registry entries created/updated.");
        println!("Registry entries created/updated.");
        println!("The application is now registered as the default browser handler.");

        Ok(())
    }

    fn unregister_handler(&self) -> Result<(), Box<dyn std::error::Error>> {
        info!("Attempting to unregister as default browser handler...");

        let hkcu = RegKey::predef(HKEY_CURRENT_USER);

        // Remove URLAssociations
        if let Ok(capabilities_key) = hkcu.open_subkey_with_flags(
            format!(
                "Software\\Clients\\StartMenuInternet\\{}\\Capabilities",
                PROG_ID
            ),
            KEY_ALL_ACCESS,
        ) {
            let _ = capabilities_key.delete_subkey_all("URLAssociations");
            info!("URLAssociations key removed or did not exist.");
        } else {
            warn!("Could not open Capabilities key to remove URLAssociations.");
        }

        // Remove Capabilities key itself (which is ProgID under StartMenuInternet)
        if let Ok(start_menu_internet_key) =
            hkcu.open_subkey_with_flags("Software\\Clients\\StartMenuInternet", KEY_ALL_ACCESS)
        {
            let _ = start_menu_internet_key.delete_subkey_all(PROG_ID);
            let _ = start_menu_internet_key.set_value("", &""); // Clear the default
            info!(
                "Software\\Clients\\StartMenuInternet\\{} key removed or did not exist.",
                PROG_ID
            );
        } else {
            warn!("Could not open Software\\Clients\\StartMenuInternet key.");
        }

        // Remove from RegisteredApplications
        if let Ok(registered_apps_key) =
            hkcu.open_subkey_with_flags("Software\\RegisteredApplications", KEY_ALL_ACCESS)
        {
            let _ = registered_apps_key.delete_value(CANONICAL_NAME);
            info!(
                "{} value removed from RegisteredApplications or did not exist.",
                CANONICAL_NAME
            );
        } else {
            warn!("Could not open RegisteredApplications key.");
        }

        // Clear ProgID settings for http and https
        if let Ok(http_key) = hkcu.open_subkey_with_flags("Software\\Classes\\http", KEY_ALL_ACCESS)
        {
            let _ = http_key.set_value("", &"");
            info!("Cleared default ProgID for http.");
        } else {
            warn!("Could not open Software\\Classes\\http key.");
        }
        if let Ok(https_key) =
            hkcu.open_subkey_with_flags("Software\\Classes\\https", KEY_ALL_ACCESS)
        {
            let _ = https_key.set_value("", &"");
            info!("Cleared default ProgID for https.");
        } else {
            warn!("Could not open Software\\Classes\\https key.");
        }

        // Remove custom URL scheme (rust-browser-handler)
        if let Ok(classes_key) = hkcu.open_subkey_with_flags("Software\\Classes", KEY_ALL_ACCESS) {
            let _ = classes_key.delete_subkey_all("rust-browser-handler");
            info!("Software\\Classes\\rust-browser-handler key removed or did not exist.");
        } else {
            warn!("Could not open Software\\Classes key.");
        }

        info!("Unregistration process completed. Some keys/values might have already been absent.");
        println!("Unregistration process completed.");
        println!(
            "You may need to manually select a new default browser in Windows Settings if Rust Browser Handler was previously set."
        );

        Ok(())
    }

    fn is_default_handler(&self) -> bool {
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        if let Ok(http_key) = hkcu.open_subkey("Software\\Classes\\http")
            && let Ok(value) = http_key.get_value::<String, _>("")
        {
            return value == "rust-browser-handler";
        }
        false
    }

    fn default_browser_path(&self) -> Option<String> {
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);

        // Modern Windows records the user's browser choice as a ProgId under
        // the documented UrlAssociations\<scheme>\UserChoice key.
        let prog_id = hkcu
            .open_subkey(
                "Software\\Microsoft\\Windows\\CurrentVersion\\UrlAssociations\\https\\UserChoice",
            )
            .or_else(|_| {
                hkcu.open_subkey(
                    "Software\\Microsoft\\Windows\\CurrentVersion\\UrlAssociations\\http\\UserChoice",
                )
            })
            .ok()
            .and_then(|key| key.get_value::<String, _>("ProgId").ok())?;

        if prog_id.eq_ignore_ascii_case(PROG_ID) {
            // We're already the registered default; nothing to float to the top.
            return None;
        }

        let command = hkcu
            .open_subkey(format!(
                "Software\\Classes\\{prog_id}\\shell\\open\\command"
            ))
            .ok()
            .and_then(|key| key.get_value::<String, _>("").ok())
            .or_else(|| {
                RegKey::predef(HKEY_CLASSES_ROOT)
                    .open_subkey(format!("{prog_id}\\shell\\open\\command"))
                    .ok()
                    .and_then(|key| key.get_value::<String, _>("").ok())
            })?;

        let exe_path = extract_executable_path_from_command(command)?;
        if exe_path.is_empty() {
            return None;
        }
        Some(crate::browser_discovery::normalize_path(&exe_path))
    }
}

/// Generate possible Windows paths for browser executables
fn generate_common_browser_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();

    // Standard installation locations
    let prefixes = vec![
        env::var("PROGRAMFILES").unwrap_or_default(),
        env::var("PROGRAMFILES(X86)").unwrap_or_default(),
        env::var("LOCALAPPDATA").unwrap_or_default(),
    ];

    // Common browser paths
    let browser_paths = [
        "Google/Chrome/Application/chrome.exe",
        "Mozilla Firefox/firefox.exe",
        "Microsoft/Edge/Application/msedge.exe",
        "BraveSoftware/Brave-Browser/Application/brave.exe",
        "Opera/launcher.exe",
        "Opera/opera.exe",
        "Vivaldi/Application/vivaldi.exe",
        "LibreWolf/librewolf.exe",
        "Waterfox/waterfox.exe",
        "Thorium/Application/thorium.exe",
        "Ablaze Floorp/floorp.exe",
    ];

    for prefix in prefixes {
        if !prefix.is_empty() {
            for &browser_path in &browser_paths {
                let mut full_path = PathBuf::from(&prefix);
                full_path.push(browser_path);
                paths.push(full_path);
            }
        }
    }

    // Scoop installations
    if let Ok(user_profile) = env::var("USERPROFILE") {
        paths.push(PathBuf::from(format!(
            "{}/scoop/apps/googlechrome/current/chrome.exe",
            user_profile
        )));
        paths.push(PathBuf::from(format!(
            "{}/scoop/apps/firefox/current/firefox.exe",
            user_profile
        )));
        paths.push(PathBuf::from(format!(
            "{}/scoop/apps/brave/current/brave.exe",
            user_profile
        )));
        paths.push(PathBuf::from(format!(
            "{}/scoop/apps/opera/current/opera.exe",
            user_profile
        )));
        paths.push(PathBuf::from(format!(
            "{}/scoop/apps/vivaldi/current/vivaldi.exe",
            user_profile
        )));
    }

    paths
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_executable_path_from_command() {
        assert_eq!(
            extract_executable_path_from_command(
                r#""C:\Program Files\Browser\browser.exe""#.to_string()
            ),
            Some(r#"C:\Program Files\Browser\browser.exe"#.to_string())
        );
        assert_eq!(
            extract_executable_path_from_command(
                "\"C:\\Program Files (x86)\\Other Browser\\other_browser.exe\" %1".to_string()
            ),
            Some("C:\\Program Files (x86)\\Other Browser\\other_browser.exe".to_string())
        );
        assert_eq!(
            extract_executable_path_from_command("browser.exe --arg".to_string()),
            Some("browser.exe".to_string())
        );
        assert_eq!(
            extract_executable_path_from_command(
                "\"browser with spaces.exe\" %1 --profile default".to_string()
            ),
            Some("browser with spaces.exe".to_string())
        );
        assert_eq!(extract_executable_path_from_command("".to_string()), None);
        assert_eq!(
            extract_executable_path_from_command("   ".to_string()),
            None
        );
    }

    #[test]
    fn test_is_browser_executable() {
        assert!(is_browser_executable(
            "C:/Program Files/Google/Chrome/Application/chrome.exe"
        ));
        assert!(is_browser_executable(
            "C:/Program Files/Mozilla Firefox/firefox.exe"
        ));
        assert!(!is_browser_executable(
            "C:/Program Files/Browser/browser.dll"
        ));
        assert!(!is_browser_executable(""));
        assert!(!is_browser_executable(
            "C:/Program Files/Browser/browser.exe.txt"
        ));
    }
}
