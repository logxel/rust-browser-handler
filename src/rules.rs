use regex::Regex;
use serde::{Deserialize, Serialize};
use std::io;
use std::path::PathBuf;

#[derive(Debug, Serialize, Deserialize)]
pub struct Rule {
    pub pattern: String,
    pub browser: String,
    pub is_regex: Option<bool>, // Optional field to indicate if the pattern is a regex
    #[serde(skip)]
    pub compiled_regex: Option<Regex>,
}

impl Rule {
    /// Pre-compile the regex pattern so it is not recompiled on every match.
    pub fn compile_regex(&mut self) {
        if self.is_regex.unwrap_or(false) {
            self.compiled_regex = Regex::new(&self.pattern).ok();
        }
    }
}

pub fn get_rules_file_path() -> io::Result<PathBuf> {
    let mut path = dirs::config_dir().ok_or_else(|| {
        io::Error::new(io::ErrorKind::NotFound, "Could not find config directory")
    })?;
    path.push("RustBrowserHandler");
    path.push("rules.json");
    Ok(path)
}

pub fn read_rules() -> io::Result<Vec<Rule>> {
    let path = get_rules_file_path()?;
    if !path.exists() {
        return Ok(Vec::new()); // Return empty vec if file doesn't exist
    }
    let file = std::fs::File::open(path)?;
    let reader = std::io::BufReader::new(file);
    let mut rules: Vec<Rule> = serde_json::from_reader(reader)?;
    for rule in &mut rules {
        rule.compile_regex();
    }
    Ok(rules)
}

pub fn write_rules(rules: &Vec<Rule>) -> io::Result<()> {
    let path = get_rules_file_path()?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?; // Create directories if they don't exist
    }
    let file = std::fs::File::create(path)?;
    let writer = std::io::BufWriter::new(file);
    serde_json::to_writer_pretty(writer, rules)?;
    Ok(())
}

pub fn add_rule(pattern: String, browser: String, is_regex: bool) -> io::Result<()> {
    let mut rules = read_rules().unwrap_or_else(|_| Vec::new()); // Read existing rules or start with empty
    rules.push(Rule {
        pattern,
        browser,
        is_regex: Some(is_regex),
        compiled_regex: None,
    });
    write_rules(&rules)?;
    Ok(())
}

pub fn list_rules() -> io::Result<()> {
    let rules = read_rules().unwrap_or_else(|_| Vec::new());
    if rules.is_empty() {
        println!("No rules defined.");
    } else {
        for (i, rule) in rules.iter().enumerate() {
            println!(
                "{}: Pattern: {}, Browser: {}, Is Regex: {}",
                i,
                rule.pattern,
                rule.browser,
                rule.is_regex.unwrap_or(false)
            );
        }
    }
    Ok(())
}

pub fn remove_rule(pattern: &str) -> io::Result<()> {
    let mut rules = read_rules().unwrap_or_else(|_| Vec::new());
    let initial_len = rules.len();
    rules.retain(|rule| rule.pattern != pattern);
    if rules.len() < initial_len {
        write_rules(&rules)?;
        println!("Rule with pattern '{}' removed.", pattern);
    } else {
        println!("No rule found with pattern '{}'.", pattern);
    }
    Ok(())
}

pub fn import_rules_from_file(path: &str) -> io::Result<()> {
    let file = std::fs::File::open(path)?;
    let reader = std::io::BufReader::new(file);
    let rules: Vec<Rule> = serde_json::from_reader(reader)?;
    write_rules(&rules)?; // Write to the application's rules file
    println!("Rules imported successfully from '{}'.", path);
    Ok(())
}

pub fn export_rules_to_file(path: &str) -> io::Result<()> {
    let rules = read_rules().unwrap_or_else(|_| Vec::new()); // Read from the application's rules file
    let file = std::fs::File::create(path)?;
    let writer = std::io::BufWriter::new(file);
    serde_json::to_writer_pretty(writer, &rules)?;
    println!("Rules exported successfully to '{}'.", path);
    Ok(())
}

#[cfg(test)]
mod rule_tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn create_temp_rules_file(content: &str) -> NamedTempFile {
        let mut file = NamedTempFile::new().expect("Failed to create temp file");
        writeln!(file, "{}", content).expect("Failed to write to temp file");
        file
    }

    #[test]
    fn test_read_rules_empty() {
        let file = create_temp_rules_file("[]");
        let result = read_rules_from_path(file.path().to_str().unwrap());
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 0);
    }

    #[test]
    fn test_read_rules_with_data() {
        let content = r#"[
            { "pattern": "google.com", "browser": "chrome" },
            { "pattern": "firefox.com", "browser": "firefox", "is_regex": false }
        ]"#;
        let file = create_temp_rules_file(content);
        let result = read_rules_from_path(file.path().to_str().unwrap());
        assert!(result.is_ok());
        let rules = result.unwrap();
        assert_eq!(rules.len(), 2);
        assert_eq!(rules[0].pattern, "google.com");
        assert_eq!(rules[0].browser, "chrome");
        assert_eq!(rules[0].is_regex, None);
        assert_eq!(rules[1].pattern, "firefox.com");
        assert_eq!(rules[1].browser, "firefox");
        assert_eq!(rules[1].is_regex, Some(false));
    }

    #[test]
    fn test_write_rules() {
        let rules = vec![Rule {
            pattern: "test.com".to_string(),
            browser: "edge".to_string(),
            is_regex: Some(true),
            compiled_regex: None,
        }];
        let file = NamedTempFile::new().expect("Failed to create temp file");
        let path = file.path().to_str().unwrap().to_string();
        drop(file); // Close the file so write_rules can open it

        let result = write_rules_to_path(&rules, &path);
        assert!(result.is_ok());

        let read_back_rules = read_rules_from_path(&path).expect("Failed to read back rules");
        assert_eq!(read_back_rules.len(), 1);
        assert_eq!(read_back_rules[0].pattern, "test.com");
        assert_eq!(read_back_rules[0].browser, "edge");
        assert_eq!(read_back_rules[0].is_regex, Some(true));
    }

    #[test]
    fn test_add_rule() {
        let file = create_temp_rules_file("[]");
        let path = file.path().to_str().unwrap().to_string();
        drop(file); // Close the file so add_rule can open it

        // Add a rule
        add_rule_to_path(
            &path,
            "example.com".to_string(),
            "chrome".to_string(),
            false,
        )
        .expect("Failed to add rule");

        // Read back and verify
        let rules = read_rules_from_path(&path).expect("Failed to read back rules");
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].pattern, "example.com");
        assert_eq!(rules[0].browser, "chrome");
        assert_eq!(rules[0].is_regex, Some(false));

        // Add another rule
        add_rule_to_path(&path, "regex.*".to_string(), "firefox".to_string(), true)
            .expect("Failed to add rule");

        // Read back and verify
        let rules = read_rules_from_path(&path).expect("Failed to read back rules");
        assert_eq!(rules.len(), 2);
        assert_eq!(rules[1].pattern, "regex.*");
        assert_eq!(rules[1].browser, "firefox");
        assert_eq!(rules[1].is_regex, Some(true));
    }

    #[test]
    fn test_remove_rule() {
        let content = r#"[
            { "pattern": "google.com", "browser": "chrome" },
            { "pattern": "firefox.com", "browser": "firefox" }
        ]"#;
        let file = create_temp_rules_file(content);
        let path = file.path().to_str().unwrap().to_string();
        // drop(file); // Close the file so remove_rule can open it - Removed to keep file alive

        // Remove a rule
        remove_rule_from_path(&path, "google.com").expect("Failed to remove rule");

        // Read back and verify
        let rules = read_rules_from_path(&path).expect("Failed to read back rules");
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].pattern, "firefox.com");

        // Try to remove a non-existent rule
        remove_rule_from_path(&path, "nonexistent.com").expect("Failed to remove rule");

        // Read back and verify (should be unchanged)
        let rules = read_rules_from_path(&path).expect("Failed to read back rules");
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].pattern, "firefox.com");
    }

    // Helper functions to read/write/add/remove from a specific path for testing
    fn read_rules_from_path(path: &str) -> io::Result<Vec<Rule>> {
        let file = std::fs::File::open(path)?;
        let reader = std::io::BufReader::new(file);
        let rules = serde_json::from_reader(reader)?;
        Ok(rules)
    }

    fn write_rules_to_path(rules: &Vec<Rule>, path: &str) -> io::Result<()> {
        let file = std::fs::File::create(path)?;
        let writer = std::io::BufWriter::new(file);
        serde_json::to_writer_pretty(writer, rules)?;
        Ok(())
    }

    fn add_rule_to_path(
        path: &str,
        pattern: String,
        browser: String,
        is_regex: bool,
    ) -> io::Result<()> {
        let mut rules = read_rules_from_path(path).unwrap_or_else(|_| Vec::new());
        rules.push(Rule {
            pattern,
            browser,
            is_regex: Some(is_regex),
            compiled_regex: None,
        });
        write_rules_to_path(&rules, path)?;
        Ok(())
    }

    fn remove_rule_from_path(path: &str, pattern: &str) -> io::Result<()> {
        let mut rules = read_rules_from_path(path).unwrap_or_else(|_| Vec::new());
        let initial_len = rules.len();
        rules.retain(|rule| rule.pattern != pattern);
        if rules.len() < initial_len {
            write_rules_to_path(&rules, path)?;
        }
        Ok(())
    }
}
