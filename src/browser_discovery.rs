use std::collections::HashMap;
use std::path::Path;

/// Mapping from executable file names to official browser names
pub static BROWSER_NAME_MAP: std::sync::OnceLock<HashMap<&'static str, &'static str>> =
    std::sync::OnceLock::new();

pub fn get_browser_map() -> &'static HashMap<&'static str, &'static str> {
    BROWSER_NAME_MAP.get_or_init(|| {
        let mut m = HashMap::new();
        m.insert("chrome", "Google Chrome");
        m.insert("chromium", "Chromium");
        m.insert("chromium-browser", "Chromium");
        m.insert("firefox", "Mozilla Firefox");
        m.insert("firefox-esr", "Mozilla Firefox ESR");
        m.insert("librewolf", "LibreWolf");
        m.insert("brave-browser", "Brave");
        m.insert("opera", "Opera");
        m.insert("vivaldi", "Vivaldi");
        m.insert("thorium-browser", "Thorium");
        m.insert("floorp", "Floorp");
        m.insert("microsoft-edge", "Microsoft Edge");
        m.insert("microsoft-edge-stable", "Microsoft Edge");
        m.insert("microsoft-edge-dev", "Microsoft Edge");
        m.insert("microsoft-edge-beta", "Microsoft Edge");
        m.insert("msedge", "Microsoft Edge");
        m.insert("brave", "Brave");
        m.insert("brave-stable", "Brave");
        m.insert("google-chrome", "Google Chrome");
        m.insert("google-chrome-stable", "Google Chrome");
        m
    })
}

fn get_flatpak_browser_name(app_id: &str) -> String {
    let lower = app_id.to_ascii_lowercase();
    if lower.contains("firefox") {
        if lower.contains("developeredition") {
            "Mozilla Firefox Developer Edition".to_string()
        } else if lower.contains("esr") {
            "Mozilla Firefox ESR".to_string()
        } else {
            "Mozilla Firefox".to_string()
        }
    } else if lower.contains("chromium") {
        "Chromium".to_string()
    } else if lower.contains("brave") {
        "Brave".to_string()
    } else if lower.contains("microsoft.edge") || lower.contains("edge") {
        "Microsoft Edge".to_string()
    } else if lower.contains("vivaldi") {
        "Vivaldi".to_string()
    } else if lower.contains("opera") {
        "Opera".to_string()
    } else if lower.contains("librewolf") {
        "LibreWolf".to_string()
    } else {
        app_id.to_string()
    }
}

/// Gets a displayable browser name from its path using the official name if possible
pub fn get_browser_name_from_path(path: &str) -> String {
    if let Some(app_id) = path.strip_prefix("flatpak:") {
        return get_flatpak_browser_name(app_id);
    }
    Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .and_then(|name| {
            let lower = name.to_ascii_lowercase();
            get_browser_map()
                .get(lower.as_str())
                .map(|s| s.to_string())
                .or(Some(name.to_string()))
        })
        .unwrap_or_else(|| path.to_string())
}

/**
 * Normalizes a path for cross-platform deduplication.
 *
 * Strategy:
 * - Prefer filesystem-based canonicalization when available.
 * - On error, fall back to a deterministic, purely syntactic normalization:
 *   - Convert '\' to '/' so separators are consistent across platforms.
 *   - Remove redundant trailing separators (except for root "/").
 */
pub fn normalize_path(path: &str) -> String {
    match Path::new(path).canonicalize() {
        Ok(canonical) => canonical.to_string_lossy().replace('\\', "/"),
        Err(_) => normalize_fallback(path),
    }
}

/// Syntactic, filesystem-independent normalization used when `canonicalize` fails.
fn normalize_fallback(path: &str) -> String {
    // Normalize separators to forward slashes
    let mut normalized = path.replace('\\', "/");

    // Collapse trailing separators, but keep "/" as-is
    while normalized.len() > 1 && normalized.ends_with('/') {
        normalized.pop();
    }

    normalized
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn test_get_browser_name_from_path() {
        assert_eq!(
            get_browser_name_from_path("C:/Program Files/Google/Chrome/Application/chrome"),
            "Google Chrome".to_string()
        );
        assert_eq!(
            get_browser_name_from_path("C:/Program Files/Mozilla Firefox/firefox"),
            "Mozilla Firefox".to_string()
        );
        assert_eq!(get_browser_name_from_path(""), "".to_string());
        assert_eq!(
            get_browser_name_from_path("C:/not_a_browser.txt"),
            "not_a_browser.txt".to_string()
        );
    }

    #[test]
    fn test_normalize_path() {
        assert_eq!(
            normalize_path("C:\\Program Files\\Google\\Chrome\\Application\\chrome"),
            "C:/Program Files/Google/Chrome/Application/chrome".to_string()
        );

        let unique = format!(
            "rbh-normalize-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system clock is before UNIX_EPOCH")
                .as_nanos()
        );
        let temp_root = std::env::temp_dir().join(unique);
        let nested_dir = temp_root.join("nested");
        fs::create_dir_all(&nested_dir).expect("failed to create temp test directory");
        let file_path = nested_dir.join("browser-bin");
        fs::write(&file_path, b"test").expect("failed to create temp test file");

        let synthetic_with_dot_segments = nested_dir.join("..").join("nested/browser-bin");
        let expected = file_path
            .canonicalize()
            .expect("failed to canonicalize expected path")
            .to_string_lossy()
            .replace('\\', "/");

        assert_eq!(
            normalize_path(&synthetic_with_dot_segments.to_string_lossy()),
            expected
        );

        let _ = fs::remove_dir_all(&temp_root);

        assert_eq!(
            normalize_path("C:/not_a_browser.txt/"),
            "C:/not_a_browser.txt".to_string()
        );
    }

    #[test]
    fn test_fallback_normalize_path() {
        // Simulate a path that cannot be canonicalized (e.g., non-existent path)
        let non_existent_path = "C:/this/path/does/not/exist";
        let normalized = normalize_path(non_existent_path);
        assert_eq!(normalized, "C:/this/path/does/not/exist".to_string());
    }
}
