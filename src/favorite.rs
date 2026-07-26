use serde::{Deserialize, Serialize};
use std::io;
use std::path::PathBuf;

#[derive(Debug, Default, Serialize, Deserialize)]
struct FavoriteFile {
    browser: Option<String>,
}

fn get_favorite_file_path() -> io::Result<PathBuf> {
    let mut path = dirs::config_dir().ok_or_else(|| {
        io::Error::new(io::ErrorKind::NotFound, "Could not find config directory")
    })?;
    path.push("RustBrowserHandler");
    path.push("favorite.json");
    Ok(path)
}

/// Returns the saved favorite browser spec (a path, or a profile-encoded
/// spec from `browser_profiles::encode_browser_spec`), if one is set.
pub fn read_favorite_browser() -> Option<String> {
    let path = get_favorite_file_path().ok()?;
    let content = std::fs::read_to_string(path).ok()?;
    let parsed: FavoriteFile = serde_json::from_str(&content).ok()?;
    parsed.browser.filter(|b| !b.is_empty())
}

pub fn write_favorite_browser(browser: &str) -> io::Result<()> {
    let path = get_favorite_file_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let content = serde_json::to_string_pretty(&FavoriteFile {
        browser: Some(browser.to_string()),
    })
    .map_err(io::Error::other)?;
    std::fs::write(path, content)?;
    Ok(())
}

/// Removes the saved favorite, if any. Returns whether one was actually set.
pub fn clear_favorite_browser() -> io::Result<bool> {
    let path = get_favorite_file_path()?;
    if !path.exists() {
        return Ok(false);
    }
    std::fs::remove_file(path)?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn favorite_file_round_trips_through_json() {
        let json = serde_json::to_string(&FavoriteFile {
            browser: Some("/usr/bin/firefox".to_string()),
        })
        .unwrap();
        let parsed: FavoriteFile = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.browser.as_deref(), Some("/usr/bin/firefox"));
    }

    #[test]
    fn missing_browser_field_defaults_to_none() {
        let parsed: FavoriteFile = serde_json::from_str("{}").unwrap();
        assert_eq!(parsed.browser, None);
    }
}
