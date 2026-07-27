use super::PlatformHandler;
use log::{info, warn};
use std::collections::HashSet;
use std::fs;
use std::io;
use std::io::ErrorKind;
use std::path::Path;
use std::process::{Command, Output};
use std::sync::Mutex;
use std::time::{Duration, Instant};

pub struct LinuxHandler;

static BROWSER_CACHE: Mutex<Option<(Vec<String>, Instant)>> = Mutex::new(None);
const CACHE_TTL: Duration = Duration::from_secs(300);

fn render_desktop_entry(exe_path: &str) -> String {
    include_str!("../../packaging/desktop/rust-browser-handler.desktop.in")
        .replace("@EXECUTABLE@", exe_path)
}

fn extract_stderr(output: &Output) -> Option<String> {
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    if stderr.is_empty() {
        None
    } else {
        Some(stderr)
    }
}

fn format_xdg_mime_io_error(prefix: &str, err: &io::Error) -> String {
    match err.kind() {
        ErrorKind::NotFound => {
            format!("{prefix}: command not found (is `xdg-mime` installed and on PATH?)")
        }
        ErrorKind::PermissionDenied => {
            format!("{prefix}: permission denied when executing `xdg-mime`")
        }
        other => format!("{prefix}: I/O error ({other:?}): {err}"),
    }
}

fn purge_desktop_entry_from_mimeapps(contents: &str, desktop_id: &str) -> String {
    let mut output = Vec::new();
    let mut current_section: Option<String> = None;

    for line in contents.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') && trimmed.len() > 2 {
            current_section = Some(trimmed[1..trimmed.len() - 1].to_string());
            output.push(line.to_string());
            continue;
        }

        if let Some((key, value)) = line.split_once('=') {
            let mut entries: Vec<&str> = value
                .split(';')
                .filter(|entry| !entry.is_empty() && *entry != desktop_id)
                .collect();

            let in_default_applications = matches!(
                current_section.as_deref(),
                Some("Default Applications") | Some("DefaultApplications")
            );

            if entries.is_empty() {
                if in_default_applications {
                    continue;
                }
                output.push(format!("{}=", key));
                continue;
            }

            entries.push("");
            output.push(format!("{}={}", key, entries.join(";")));
        } else {
            output.push(line.to_string());
        }
    }

    let mut normalized = output.join("\n");
    if contents.ends_with('\n') {
        normalized.push('\n');
    }
    normalized
}

fn cleanup_mimeapps_defaults(desktop_id: &str) -> io::Result<bool> {
    let mut changed_any = false;
    let mut paths = Vec::new();

    if let Some(config_dir) = dirs::config_dir() {
        paths.push(config_dir.join("mimeapps.list"));
    }
    if let Some(data_dir) = dirs::data_dir() {
        paths.push(data_dir.join("applications/mimeapps.list"));
    }

    for path in paths {
        if !path.exists() {
            continue;
        }

        let contents = fs::read_to_string(&path)?;
        let updated = purge_desktop_entry_from_mimeapps(&contents, desktop_id);

        if updated != contents {
            fs::write(path, updated)?;
            changed_any = true;
        }
    }

    Ok(changed_any)
}

/// Parse `mimeapps.list` content and set (or replace) `x-scheme-handler/http`
/// and `x-scheme-handler/https` entries inside the `[Default Applications]`
/// section, preserving all other sections and entries intact.
/// If no `[Default Applications]` section exists, one is appended.
fn set_mime_default_entries(contents: &str, desktop_id: &str) -> String {
    let mut in_default_applications = false;
    let mut http_found = false;
    let mut https_found = false;
    let mut output: Vec<String> = Vec::new();

    for line in contents.lines() {
        let trimmed = line.trim();

        if trimmed.starts_with('[') && trimmed.ends_with(']') && trimmed.len() > 2 {
            // Leaving the [Default Applications] section: insert any missing entries.
            if in_default_applications && (!http_found || !https_found) {
                if !http_found {
                    output.push(format!("x-scheme-handler/http={}", desktop_id));
                    http_found = true;
                }
                if !https_found {
                    output.push(format!("x-scheme-handler/https={}", desktop_id));
                    https_found = true;
                }
            }

            let section_name = &trimmed[1..trimmed.len() - 1];
            in_default_applications =
                section_name == "Default Applications" || section_name == "DefaultApplications";
            output.push(line.to_string());
            continue;
        }

        if in_default_applications {
            if let Some((key, _)) = line.split_once('=') {
                let key = key.trim();
                if key == "x-scheme-handler/http" {
                    output.push(format!("x-scheme-handler/http={}", desktop_id));
                    http_found = true;
                    continue;
                }
                if key == "x-scheme-handler/https" {
                    output.push(format!("x-scheme-handler/https={}", desktop_id));
                    https_found = true;
                    continue;
                }
            }
        }

        output.push(line.to_string());
    }

    // At EOF: if still inside [Default Applications], append missing entries.
    if in_default_applications && (!http_found || !https_found) {
        if !http_found {
            output.push(format!("x-scheme-handler/http={}", desktop_id));
        }
        if !https_found {
            output.push(format!("x-scheme-handler/https={}", desktop_id));
        }
    }

    // No [Default Applications] section was found: add one at the end.
    if !output.iter().any(|l| {
        let t = l.trim();
        (t.starts_with("[Default Applications]") || t.starts_with("[DefaultApplications]"))
            && t.ends_with(']')
    }) {
        if !output.is_empty() && !output.last().map_or(true, |l| l.is_empty()) {
            output.push(String::new());
        }
        output.push("[Default Applications]".to_string());
        output.push(format!("x-scheme-handler/http={}", desktop_id));
        output.push(format!("x-scheme-handler/https={}", desktop_id));
    }

    let mut result = output.join("\n");
    if contents.ends_with('\n') {
        result.push('\n');
    }
    result
}

/// Directly write `[Default Applications]` entries for HTTP and HTTPS
/// scheme handlers into `~/.config/mimeapps.list`.  This is the primary
/// registration mechanism — it avoids GLib-GIO warnings on COSMIC and
/// other DEs that forbid anything but `[Default Applications]` in
/// desktop-specific mimeapps files.
fn write_mime_defaults(desktop_id: &str) -> io::Result<()> {
    let config_dir = dirs::config_dir()
        .ok_or_else(|| io::Error::other("Could not find config directory"))?;
    fs::create_dir_all(&config_dir)?;

    let mimeapps_path = config_dir.join("mimeapps.list");

    let contents = if mimeapps_path.exists() {
        fs::read_to_string(&mimeapps_path)?
    } else {
        String::new()
    };

    let updated = set_mime_default_entries(&contents, desktop_id);

    if updated != contents {
        fs::write(&mimeapps_path, &updated)?;
    }

    Ok(())
}

fn is_flatpak_browser_id(app_id: &str) -> bool {
    let lower = app_id.to_ascii_lowercase();
    let known_app_ids = [
        "org.mozilla.firefox",
        "org.mozilla.firefox.developeredition",
        "org.mozilla.firefox.esr",
        "org.chromium.Chromium",
        "com.github.brave.Brave",
        "com.microsoft.Edge",
        "com.microsoft.EdgeDev",
        "com.microsoft.EdgeBeta",
        "com.operasoftware.Opera",
        "com.vivaldi.Vivaldi",
        "org.librewolf-community.LibreWolf",
    ];

    known_app_ids.contains(&app_id)
        || lower.contains("firefox")
        || lower.contains("chromium")
        || lower.contains("brave")
        || lower.contains("microsoft.edge")
        || lower.contains("edge")
        || lower.contains("opera")
        || lower.contains("vivaldi")
        || lower.contains("librewolf")
}

fn parse_flatpak_browser_list(output: &str) -> Vec<String> {
    output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .filter(|app_id| is_flatpak_browser_id(app_id))
        .map(|app_id| format!("flatpak:{}", app_id))
        .collect()
}

fn detect_flatpak_browsers() -> Vec<String> {
    if let Ok(output) = Command::new("flatpak")
        .args(["list", "--app", "--columns=application"])
        .output()
        && output.status.success()
    {
        return parse_flatpak_browser_list(&String::from_utf8_lossy(&output.stdout));
    }

    Vec::new()
}

/// Extracts the launch command from a desktop entry's `Exec=` line, resolving
/// it to the same path format `find_browsers` produces: a flatpak
/// application ID (as `flatpak:<app-id>`) when the entry launches one, or an
/// absolute, normalized executable path otherwise.
fn resolve_exec_line(exec_value: &str) -> Option<String> {
    // Desktop entry Exec fields can carry %u/%f-style field codes; only the
    // command and its non-placeholder arguments matter here.
    let tokens: Vec<&str> = exec_value
        .split_whitespace()
        .filter(|t| !t.starts_with('%'))
        .collect();

    let first = *tokens.first()?;
    let first_basename = Path::new(first).file_name().and_then(|n| n.to_str())?;

    if first_basename == "flatpak" {
        let app_id = tokens
            .iter()
            .skip_while(|t| **t != "run")
            .nth(1)
            .filter(|t| !t.starts_with('-'))?;
        return Some(format!("flatpak:{app_id}"));
    }

    resolve_command_to_path(first)
}

fn resolve_command_to_path(cmd: &str) -> Option<String> {
    let path = Path::new(cmd);
    if path.is_absolute() {
        return if path.exists() {
            Some(crate::browser_discovery::normalize_path(cmd))
        } else {
            None
        };
    }

    let output = Command::new("which").arg(cmd).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let resolved = String::from_utf8(output.stdout).ok()?.trim().to_string();
    if resolved.is_empty() {
        None
    } else {
        Some(crate::browser_discovery::normalize_path(&resolved))
    }
}

/// Resolves a `.desktop` file ID (as returned by `xdg-mime query default`)
/// to a launchable path by searching the standard XDG application
/// directories, including Flatpak's exports, for the matching entry.
fn resolve_desktop_entry_exec(desktop_id: &str) -> Option<String> {
    let mut search_dirs = Vec::new();
    if let Some(data_dir) = dirs::data_dir() {
        search_dirs.push(data_dir.join("applications"));
        search_dirs.push(data_dir.join("flatpak/exports/share/applications"));
    }
    search_dirs.push(Path::new("/usr/local/share/applications").to_path_buf());
    search_dirs.push(Path::new("/usr/share/applications").to_path_buf());
    search_dirs.push(Path::new("/var/lib/flatpak/exports/share/applications").to_path_buf());

    for dir in search_dirs {
        let content = match fs::read_to_string(dir.join(desktop_id)) {
            Ok(content) => content,
            Err(_) => continue,
        };
        let Some(exec_line) = content.lines().find(|l| l.starts_with("Exec=")) else {
            continue;
        };
        if let Some(resolved) = resolve_exec_line(exec_line.trim_start_matches("Exec=").trim()) {
            return Some(resolved);
        }
    }

    None
}

fn detect_system_mimeapps_references(desktop_id: &str) -> Vec<String> {
    let candidates = [
        "/etc/xdg/mimeapps.list",
        "/usr/local/share/applications/mimeapps.list",
        "/usr/share/applications/mimeapps.list",
    ];

    candidates
        .iter()
        .filter_map(|path| {
            let content = fs::read_to_string(path).ok()?;
            if content.contains(desktop_id) {
                Some((*path).to_string())
            } else {
                None
            }
        })
        .collect()
}

impl PlatformHandler for LinuxHandler {
    fn find_browsers(&self) -> Vec<String> {
        // Check cache first
        if let Ok(guard) = BROWSER_CACHE.lock() {
            if let Some((ref cached, timestamp)) = *guard {
                if timestamp.elapsed() < CACHE_TTL {
                    return cached.clone();
                }
            }
        }

        let mut browsers = HashSet::new();

        // Common browsers to check
        let browser_commands = [
            "google-chrome",
            "google-chrome-stable",
            "chromium",
            "chromium-browser",
            "firefox",
            "firefox-esr",
            "librewolf",
            "brave-browser",
            "opera",
            "vivaldi",
            "thorium-browser",
            "floorp",
            "microsoft-edge",
            "microsoft-edge-stable",
            "microsoft-edge-dev",
            "microsoft-edge-beta",
        ];

        for cmd in &browser_commands {
            if let Ok(output) = Command::new("which").arg(cmd).output()
                && output.status.success()
                && let Ok(path_str) = String::from_utf8(output.stdout)
            {
                let path = path_str.trim();
                if !path.is_empty() && Path::new(path).exists() {
                    browsers.insert(crate::browser_discovery::normalize_path(path));
                }
            }
        }

        // Also check common installation paths
        let common_paths = [
            "/usr/bin/google-chrome",
            "/usr/bin/google-chrome-stable",
            "/usr/bin/chromium",
            "/usr/bin/chromium-browser",
            "/usr/bin/firefox",
            "/usr/bin/firefox-esr",
            "/usr/bin/librewolf",
            "/usr/bin/brave-browser",
            "/usr/bin/opera",
            "/usr/bin/vivaldi",
            "/usr/bin/thorium-browser",
            "/usr/bin/floorp",
            "/usr/bin/microsoft-edge",
            "/usr/bin/microsoft-edge-stable",
            "/usr/bin/microsoft-edge-dev",
            "/opt/google/chrome/chrome",
            "/opt/brave.com/brave/brave",
            "/opt/opera/opera",
            "/opt/vivaldi/vivaldi",
            "/opt/microsoft/msedge/msedge",
        ];

        for path in &common_paths {
            if Path::new(path).exists() {
                browsers.insert(crate::browser_discovery::normalize_path(path));
            }
        }

        for flatpak_browser in detect_flatpak_browsers() {
            browsers.insert(flatpak_browser);
        }

        // Deduplicate native entries that resolve to the same display name,
        // while keeping all Flatpak entries (they are already unique by app ID).
        let mut all_paths: Vec<String> = browsers.into_iter().collect();
        all_paths.sort();

        let mut seen_names: HashSet<String> = HashSet::new();
        let mut deduped: Vec<String> = Vec::new();
        for path in all_paths {
            if path.starts_with("flatpak:") {
                deduped.push(path);
            } else {
                let name = crate::browser_discovery::get_browser_name_from_path(&path);
                if seen_names.insert(name) {
                    deduped.push(path);
                }
            }
        }

        let deduped = deduped;

        // Update cache
        if let Ok(mut guard) = BROWSER_CACHE.lock() {
            *guard = Some((deduped.clone(), Instant::now()));
        }

        deduped
    }

    fn set_as_default_handler(&self) -> Result<(), Box<dyn std::error::Error>> {
        // Get the executable path
        let exe_path = std::env::current_exe()?
            .to_str()
            .ok_or("Failed to get executable path")?
            .to_string();

        // Create .desktop file content
        let desktop_content = render_desktop_entry(&exe_path);

        // Create applications directory if it doesn't exist
        let desktop_dir = dirs::data_dir()
            .ok_or("Could not find data directory")?
            .join("applications");
        fs::create_dir_all(&desktop_dir)?;

        // Write the .desktop file
        let desktop_file = desktop_dir.join("rust-browser-handler.desktop");
        fs::write(&desktop_file, desktop_content)?;

        // Update desktop database so DE settings can discover this entry.
        let update_db_status = Command::new("update-desktop-database")
            .arg(&desktop_dir)
            .status();

        match update_db_status {
            Ok(status) if !status.success() => {
                warn!(
                    "warning: update-desktop-database exited with status: {}",
                    status
                );
            }
            Err(e) => {
                warn!(
                    "warning: update-desktop-database is not available or could not be executed ({}); desktop database was not refreshed.",
                    e
                );
            }
            _ => {}
        }

        // Primary: write MIME defaults directly into mimeapps.list.
        // This avoids GLib-GIO warnings on COSMIC and other DEs that
        // restrict desktop-specific mimeapps files to [Default Applications].
        match write_mime_defaults("rust-browser-handler.desktop") {
            Ok(()) => {
                info!(
                    "Set HTTP/HTTPS defaults in mimeapps.list (primary registration)."
                );
            }
            Err(e) => {
                warn!(
                    "Could not write MIME defaults directly to mimeapps.list: {}.
                    Falling back to xdg-mime.",
                    e
                );
            }
        }

        // Set MIME type associations using xdg-mime (fallback)
        let http_status = Command::new("xdg-mime")
            .args([
                "default",
                "rust-browser-handler.desktop",
                "x-scheme-handler/http",
            ])
            .output();

        let https_status = Command::new("xdg-mime")
            .args([
                "default",
                "rust-browser-handler.desktop",
                "x-scheme-handler/https",
            ])
            .output();

        match (http_status, https_status) {
            (Ok(http), Ok(https)) if http.status.success() && https.status.success() => {
                // Set default web browser via xdg-settings for DE integration.
                let xdg_settings_set = Command::new("xdg-settings")
                    .args(["set", "default-web-browser", "rust-browser-handler.desktop"])
                    .output();

                let xdg_settings_get = Command::new("xdg-settings")
                    .args(["get", "default-web-browser"])
                    .output();

                let xdg_settings_matches = xdg_settings_get
                    .as_ref()
                    .ok()
                    .and_then(|output| String::from_utf8(output.stdout.clone()).ok())
                    .map(|value| value.trim().to_string())
                    .as_deref()
                    == Some("rust-browser-handler.desktop");

                if xdg_settings_matches {
                    match xdg_settings_set {
                        Ok(output) if !output.status.success() => {
                            eprintln!(
                                "warning: xdg-settings set default-web-browser exited with {} but xdg-settings get already reports rust-browser-handler.desktop. Some desktop environments, mail fail set operations even when the setting is correct. The MIME associations were still applied.",
                                output.status
                            );
                        }
                        Err(e) => {
                            eprintln!(
                                "warning: xdg-settings set default-web-browser could not be executed ({}), but xdg-settings get already reports rust-browser-handler.desktop. Some desktop environments, including Pop!_OS COSMIC, do not use set as a reliable signal. The MIME associations were still applied.",
                                e
                            );
                        }
                        _ => {}
                    }

                    println!("Successfully registered as default browser using XDG standards.");
                    println!(
                        "The application is now set as the default handler for HTTP and HTTPS URLs."
                    );
                    Ok(())
                } else {
                    match xdg_settings_set {
                        Ok(output) if output.status.success() => {
                            println!(
                                "Successfully registered as default browser using XDG standards."
                            );
                            println!(
                                "The application is now set as the default handler for HTTP and HTTPS URLs."
                            );
                            Ok(())
                        }
                        Ok(output) => {
                            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
                            eprintln!(
                                "warning: xdg-settings set default-web-browser failed (status: {}{}). The MIME associations were still applied.",
                                output.status,
                                if stderr.is_empty() {
                                    String::new()
                                } else {
                                    format!(", stderr: {}", stderr)
                                }
                            );
                            println!(
                                "Successfully registered default HTTP and HTTPS associations using XDG MIME data."
                            );
                            println!(
                                "Note: xdg-settings did not report the desktop entry back, so the MIME association is the authoritative setting here."
                            );
                            Ok(())
                        }
                        Err(e) => {
                            eprintln!(
                                "warning: xdg-settings is unavailable or could not be executed ({}). The MIME associations were still applied.",
                                e
                            );
                            println!(
                                "Successfully registered default HTTP and HTTPS associations using XDG MIME data."
                            );
                            println!(
                                "Note: xdg-settings was skipped, so the MIME association is the authoritative setting here."
                            );
                            Ok(())
                        }
                    }
                }
            }
            (Err(http_err), Err(https_err)) => {
                let _ = fs::remove_file(&desktop_file);
                Err(io::Error::other(format!(
                    "{}; {}",
                    format_xdg_mime_io_error(
                        "Failed to run xdg-mime for HTTP association",
                        &http_err
                    ),
                    format_xdg_mime_io_error(
                        "Failed to run xdg-mime for HTTPS association",
                        &https_err
                    )
                ))
                .into())
            }
            (Err(http_err), _) => {
                let _ = fs::remove_file(&desktop_file);
                Err(io::Error::other(format_xdg_mime_io_error(
                    "Failed to run xdg-mime for HTTP association",
                    &http_err,
                ))
                .into())
            }
            (_, Err(https_err)) => {
                let _ = fs::remove_file(&desktop_file);
                Err(io::Error::other(format_xdg_mime_io_error(
                    "Failed to run xdg-mime for HTTPS association",
                    &https_err,
                ))
                .into())
            }
            (Ok(http), Ok(https)) => {
                let http_stderr = extract_stderr(&http)
                    .map(|value| format!(", http stderr: {}", value))
                    .unwrap_or_default();
                let https_stderr = extract_stderr(&https)
                    .map(|value| format!(", https stderr: {}", value))
                    .unwrap_or_default();

                let _ = fs::remove_file(&desktop_file);
                Err(io::Error::other(format!(
                    "Failed to set MIME associations (http status: {}, https status: {}){}{}. Ensure xdg-mime is available.",
                    http.status, https.status, http_stderr, https_stderr
                ))
                .into())
            }
        }
    }

    fn unregister_handler(&self) -> Result<(), Box<dyn std::error::Error>> {
        // Remove the .desktop file
        let desktop_file = dirs::data_dir()
            .ok_or("Could not find data directory")?
            .join("applications/rust-browser-handler.desktop");

        if desktop_file.exists() {
            fs::remove_file(desktop_file)?;
            println!("Removed desktop file and unregistered as default browser.");
        } else {
            println!("Desktop file not found - application may not be registered.");
        }

        // Remove stale default entries from user mimeapps lists.
        match cleanup_mimeapps_defaults("rust-browser-handler.desktop") {
            Ok(true) => {
                println!("Removed rust-browser-handler.desktop references from mimeapps.list.")
            }
            Ok(false) => {}
            Err(e) => eprintln!(
                "warning: Failed to clean MIME defaults from mimeapps.list: {}",
                e
            ),
        }

        let system_references = detect_system_mimeapps_references("rust-browser-handler.desktop");
        if !system_references.is_empty() {
            eprintln!(
                "warning: Found system-level MIME defaults still referencing rust-browser-handler.desktop in: {}. You may need manual cleanup with administrator privileges.",
                system_references.join(", ")
            );
        }

        // Note: We can't easily reset to the previous default browser
        // The system will fall back to whatever was previously configured
        // or the distribution's default browser
        println!("Browser associations have been reset. The system will use its default browser.");

        Ok(())
    }

    fn is_default_handler(&self) -> bool {
        let query_default = |mime_type: &str| -> Result<String, ()> {
            let output = Command::new("xdg-mime")
                .args(["query", "default", mime_type])
                .output()
                .map_err(|_| ())?;

            if !output.status.success() {
                return Err(());
            }

            String::from_utf8(output.stdout)
                .map(|value| value.trim().to_string())
                .map_err(|_| ())
        };

        // Check both HTTP and HTTPS associations first.
        let http_default = query_default("x-scheme-handler/http");
        let https_default = query_default("x-scheme-handler/https");

        if matches!(
            http_default.as_deref(),
            Ok(default) if default != "rust-browser-handler.desktop"
        ) {
            return false;
        }
        if matches!(
            https_default.as_deref(),
            Ok(default) if default != "rust-browser-handler.desktop"
        ) {
            return false;
        }

        if matches!(http_default.as_deref(), Ok("rust-browser-handler.desktop"))
            && matches!(https_default.as_deref(), Ok("rust-browser-handler.desktop"))
        {
            return true;
        }

        // Fallback only when xdg-mime failed for at least one lookup.
        if let Some(data_dir) = dirs::data_dir() {
            let desktop_file = data_dir.join("applications/rust-browser-handler.desktop");
            return desktop_file.exists();
        }

        false
    }

    fn default_browser_path(&self) -> Option<String> {
        let query_default = |mime_type: &str| -> Option<String> {
            let output = Command::new("xdg-mime")
                .args(["query", "default", mime_type])
                .output()
                .ok()?;
            if !output.status.success() {
                return None;
            }
            let value = String::from_utf8(output.stdout).ok()?.trim().to_string();
            if value.is_empty() { None } else { Some(value) }
        };

        let desktop_id = query_default("x-scheme-handler/https")
            .or_else(|| query_default("x-scheme-handler/http"))?;

        // If we're already the registered default, there's nothing useful to
        // float to the top over the rest of the detected browsers.
        if desktop_id == "rust-browser-handler.desktop" {
            return None;
        }

        resolve_desktop_entry_exec(&desktop_id)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        parse_flatpak_browser_list, purge_desktop_entry_from_mimeapps, render_desktop_entry,
        set_mime_default_entries, write_mime_defaults,
    };
    use std::fs;

    #[test]
    fn desktop_entry_uses_unquoted_exec_path() {
        let desktop_entry = render_desktop_entry("/tmp/rust browser handler");

        assert!(desktop_entry.contains("Exec=/tmp/rust browser handler %u"));
        assert!(!desktop_entry.contains("Exec=\"/tmp/rust browser handler\" %u"));
        assert!(!desktop_entry.contains("@EXECUTABLE@"));
    }

    #[test]
    fn parse_flatpak_browser_list_outputs_known_browsers() {
        let output = "org.mozilla.firefox\norg.chromium.Chromium\ncom.spotify.Client\ncom.github.brave.Brave\n";
        let parsed = parse_flatpak_browser_list(output);

        assert_eq!(
            parsed,
            vec![
                "flatpak:org.mozilla.firefox".to_string(),
                "flatpak:org.chromium.Chromium".to_string(),
                "flatpak:com.github.brave.Brave".to_string(),
            ]
        );
    }

    #[test]
    fn purge_mimeapps_removes_desktop_id_entries() {
        let input = "[Default Applications]\nx-scheme-handler/http=rust-browser-handler.desktop;firefox.desktop;\nx-scheme-handler/https=rust-browser-handler.desktop;\ntext/html=firefox.desktop;\n";
        let output = purge_desktop_entry_from_mimeapps(input, "rust-browser-handler.desktop");

        assert!(output.contains("x-scheme-handler/http=firefox.desktop;"));
        assert!(!output.contains("x-scheme-handler/https="));
        assert!(output.contains("text/html=firefox.desktop;"));
        assert!(!output.contains("rust-browser-handler.desktop"));
    }

    #[test]
    fn purge_mimeapps_keeps_non_default_sections_with_empty_assignment() {
        let input = "[Added Associations]\nx-scheme-handler/http=rust-browser-handler.desktop;\n";
        let output = purge_desktop_entry_from_mimeapps(input, "rust-browser-handler.desktop");

        assert!(output.contains("[Added Associations]"));
        assert!(output.contains("x-scheme-handler/http="));
    }

    #[test]
    fn set_mime_defaults_creates_section_when_no_file() {
        let output = set_mime_default_entries("", "rust-browser-handler.desktop");
        assert!(output.contains("[Default Applications]"));
        assert!(output.contains("x-scheme-handler/http=rust-browser-handler.desktop"));
        assert!(output.contains("x-scheme-handler/https=rust-browser-handler.desktop"));
    }

    #[test]
    fn set_mime_defaults_updates_existing_entries() {
        let input = "[Default Applications]\nx-scheme-handler/http=firefox.desktop\nx-scheme-handler/https=firefox.desktop\ntext/html=firefox.desktop\n";
        let output = set_mime_default_entries(input, "rust-browser-handler.desktop");
        assert!(output.contains("x-scheme-handler/http=rust-browser-handler.desktop"));
        assert!(output.contains("x-scheme-handler/https=rust-browser-handler.desktop"));
        assert!(output.contains("text/html=firefox.desktop"));
        assert!(!output.contains("firefox.desktop\nx-scheme-handler/http"));
        assert!(!output.contains("firefox.desktop\nx-scheme-handler/https"));
    }

    #[test]
    fn set_mime_defaults_preserves_other_sections() {
        let input = "[Default Applications]\nx-scheme-handler/http=firefox.desktop\n\n[Added Associations]\ntext/html=firefox.desktop;chromium.desktop;\n";
        let output = set_mime_default_entries(input, "rust-browser-handler.desktop");
        assert!(output.contains("[Added Associations]"));
        assert!(output.contains("text/html=firefox.desktop;chromium.desktop;"));
        assert!(output.contains("x-scheme-handler/http=rust-browser-handler.desktop"));
    }

    #[test]
    fn set_mime_defaults_adds_section_when_none_exists() {
        let input = "[Added Associations]\ntext/html=firefox.desktop;\n";
        let output = set_mime_default_entries(input, "rust-browser-handler.desktop");
        // [Default Applications] section appears, with entries
        let default_idx = output.find("[Default Applications]").unwrap();
        assert!(output[default_idx..].contains("x-scheme-handler/http=rust-browser-handler.desktop"));
        assert!(output[default_idx..].contains("x-scheme-handler/https=rust-browser-handler.desktop"));
        // Original section preserved
        assert!(output.contains("[Added Associations]"));
        assert!(output.contains("text/html=firefox.desktop;"));
    }

    #[test]
    fn set_mime_defaults_adds_missing_entries_to_existing_section() {
        let input = "[Default Applications]\ntext/html=firefox.desktop\n\n[Added Associations]\nx-scheme-handler/http=firefox.desktop;\n";
        let output = set_mime_default_entries(input, "rust-browser-handler.desktop");
        // Both entries should be present in [Default Applications]
        assert!(output.contains("x-scheme-handler/http=rust-browser-handler.desktop"));
        assert!(output.contains("x-scheme-handler/https=rust-browser-handler.desktop"));
        // [Added Associations] untouched
        assert!(output.contains("[Added Associations]"));
        assert!(output.contains("x-scheme-handler/http=firefox.desktop;"));
    }

    #[test]
    fn set_mime_defaults_handles_trailing_newline() {
        let input = "[Default Applications]\nx-scheme-handler/http=firefox.desktop\n";
        let output = set_mime_default_entries(input, "rust-browser-handler.desktop");
        assert!(output.ends_with('\n'));
    }

    #[test]
    fn set_mime_defaults_handles_defaultapplications_variant() {
        let input = "[DefaultApplications]\nx-scheme-handler/http=firefox.desktop\n";
        let output = set_mime_default_entries(input, "rust-browser-handler.desktop");
        assert!(output.contains("[DefaultApplications]"));
        assert!(output.contains("x-scheme-handler/http=rust-browser-handler.desktop"));
    }

    #[test]
    fn set_mime_defaults_idempotent() {
        let input = "[Default Applications]\nx-scheme-handler/http=rust-browser-handler.desktop\nx-scheme-handler/https=rust-browser-handler.desktop\n";
        let output = set_mime_default_entries(input, "rust-browser-handler.desktop");
        assert_eq!(output, input);
    }

    #[test]
    fn write_mime_defaults_creates_new_file() {
        let dir = tempfile::tempdir().unwrap();
        let mimeapps = dir.path().join("mimeapps.list");
        assert!(!mimeapps.exists());

        // Override HOME so dirs::config_dir() resolves to our temp dir.
        // dirs v6 uses XDG_CONFIG_HOME; if unset it falls back to
        // $HOME/.config.
        let saved_home = std::env::var("HOME").ok();
        let home = dir.path().to_str().unwrap();
        unsafe { std::env::set_var("HOME", home) };
        unsafe { std::env::remove_var("XDG_CONFIG_HOME") };

        // run the function
        let result = write_mime_defaults("rust-browser-handler.desktop");

        // restore env
        if let Some(prev) = saved_home {
            unsafe { std::env::set_var("HOME", prev) };
        } else {
            unsafe { std::env::remove_var("HOME") };
        }

        assert!(result.is_ok());
        let config_dir = dirs::config_dir().unwrap();
        let written = config_dir.join("mimeapps.list");
        assert!(written.exists());
        let contents = fs::read_to_string(&written).unwrap();
        assert!(contents.contains("[Default Applications]"));
        assert!(contents.contains("x-scheme-handler/http=rust-browser-handler.desktop"));
        assert!(contents.contains("x-scheme-handler/https=rust-browser-handler.desktop"));
    }

    #[test]
    fn write_mime_defaults_updates_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        let config_dir = dir.path().join(".config");
        fs::create_dir_all(&config_dir).unwrap();
        let mimeapps = config_dir.join("mimeapps.list");
        fs::write(
            &mimeapps,
            "[Default Applications]\nx-scheme-handler/http=firefox.desktop\nx-scheme-handler/https=firefox.desktop\n",
        )
        .unwrap();

        let saved_home = std::env::var("HOME").ok();
        unsafe { std::env::set_var("HOME", dir.path().to_str().unwrap()) };
        unsafe { std::env::remove_var("XDG_CONFIG_HOME") };

        let result = write_mime_defaults("rust-browser-handler.desktop");

        if let Some(prev) = saved_home {
            unsafe { std::env::set_var("HOME", prev) };
        } else {
            unsafe { std::env::remove_var("HOME") };
        }

        assert!(result.is_ok());
        let contents = fs::read_to_string(&mimeapps).unwrap();
        assert!(contents.contains("x-scheme-handler/http=rust-browser-handler.desktop"));
        assert!(contents.contains("x-scheme-handler/https=rust-browser-handler.desktop"));
        assert!(!contents.contains("firefox.desktop"));
    }
}
