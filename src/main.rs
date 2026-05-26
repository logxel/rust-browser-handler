mod browser_discovery;
mod gui;
mod platform;
mod rules;

use browser_discovery::*;
use clap::{Parser, Subcommand};
use gui::{GuiChooserOutcome, prompt_browser_selection_slint};
use log::{error, info, warn};
use platform::{Handler, PlatformHandler};
use regex::Regex;
use rules::*;
use std::fs;
use std::io;
use std::io::IsTerminal;
use std::io::Write;
use std::path::PathBuf;
use std::process::Command;

#[cfg(target_os = "linux")]
use serde_json::Value;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    url: Option<String>,
}

#[derive(Subcommand, Debug, Clone)]
enum Commands {
    /// Add a new rule
    Add {
        pattern: String,
        browser: String,
        #[arg(long)]
        regex: bool,
    },
    /// List all rules
    List,
    /// Remove a rule by pattern
    Remove { pattern: String },
    /// Import rules from a file
    Import { path: String },
    /// Export rules to a file
    Export { path: String },
    /// Install the application to a best-practice path for this platform
    Install,
    /// Register the application as the default browser handler
    Register,
    /// Unregister the application as the default browser handler
    Unregister,
    /// Uninstall the application from the best-practice path for this platform
    Uninstall,
    /// Open Windows Default Apps settings
    OpenSettings,
    /// Diagnose desktop URL handler resolution
    Doctor,
    /// Test URL handler invocation chain
    TestHandler { url: Option<String> },
}

fn print_help() {
    println!("Available commands:");
    println!("  add <pattern> <browser> [--regex]: Add a new rule");
    println!("  list: List all rules");
    println!("  remove <pattern>: Remove a rule by pattern");
    println!("  import <path>: Import rules from a file");
    println!("  export <path>: Export rules to a file");
    println!("  install: Install to a best-practice path");
    println!("  register: Register as browser handler");
    println!("  unregister: Unregister as browser handler");
    println!("  uninstall: Remove the best-practice install copy");
    println!("  open-settings: Open Windows Default Apps settings");
    println!("  doctor: Diagnose desktop URL handler resolution");
    println!("  test-handler [url]: Test URL handler invocation");
    println!("  exit: Exit interactive mode");
}

fn main() {
    env_logger::init();

    let cli = Cli::parse();

    // If no command/url, enter interactive mode to get a command
    if cli.command.is_none() && cli.url.is_none() {
        println!("Entering interactive mode. Type 'help' for commands.");
        io::stdout().flush().expect("Failed to flush stdout");
        println!();
        std::thread::sleep(std::time::Duration::from_millis(100));
        let mut input = String::new();
        loop {
            print!("> ");
            io::stdout().flush().expect("Failed to flush stdout");
            input.clear();
            match io::stdin().read_line(&mut input) {
                Ok(_) => {
                    let input = input.trim();
                    if input.is_empty() {
                        continue;
                    }
                    if input == "exit" {
                        break;
                    }
                    if input == "help" {
                        print_help();
                        continue;
                    }
                    // Try to parse the input into a Commands variant
                    match parse_interactive_command(input) {
                        Ok(command) => {
                            handle_command(Some(command), None);
                        }
                        Err(e) => {
                            warn!("{}", e);
                        }
                    }
                }
                Err(e) => {
                    error!("Failed to read input: {}", e);
                    break;
                }
            }
        }
    } else {
        // If CLI provided a command or URL, handle it directly
        handle_command(cli.command, cli.url);
    }
}

// Handles all commands, regardless of how they were obtained
fn handle_command(command: Option<Commands>, url: Option<String>) {
    let handler = Handler;
    match command {
        Some(Commands::Add {
            pattern,
            browser,
            regex,
        }) => {
            info!(
                "Adding rule: pattern='{}', browser='{}', regex={}",
                pattern, browser, regex
            );
            match add_rule(pattern, browser, regex) {
                Ok(_) => info!("Rule added successfully."),
                Err(e) => error!("Failed to add rule: {}", e),
            }
        }
        Some(Commands::List) => {
            info!("Listing rules:");
            match list_rules() {
                Ok(_) => {}
                Err(e) => error!("Failed to list rules: {}", e),
            }
        }
        Some(Commands::Remove { pattern }) => {
            info!("Removing rule with pattern: '{}'", pattern);
            match remove_rule(&pattern) {
                Ok(_) => {}
                Err(e) => error!("Failed to remove rule: {}", e),
            }
        }
        Some(Commands::Import { path }) => {
            info!("Importing rules from: {}", path);
            match import_rules_from_file(&path) {
                Ok(_) => info!("Import successful."),
                Err(e) => error!("Failed to import rules: {}", e),
            }
        }
        Some(Commands::Export { path }) => {
            info!("Exporting rules to: {}", path);
            match export_rules_to_file(&path) {
                Ok(_) => info!("Export successful."),
                Err(e) => error!("Failed to export rules: {}", e),
            }
        }
        Some(Commands::Install) => match install_to_best_practice_path() {
            Ok(installed_path) => {
                info!(
                    "Successfully installed binary to {}",
                    installed_path.to_string_lossy()
                );
                println!("Installed binary to {}", installed_path.to_string_lossy());
                #[cfg(target_os = "linux")]
                println!("Tip: run '{} register'", installed_path.to_string_lossy());
                #[cfg(target_os = "windows")]
                println!("Tip: run \"{}\" register", installed_path.to_string_lossy());
            }
            Err(e) => {
                error!("Failed to install binary: {}", e);
                println!("Failed to install binary: {}", e);
                std::process::exit(1);
            }
        },
        Some(Commands::Register) => {
            info!("Registering as default browser handler...");
            println!("Registering as default browser handler...");
            match choose_register_location() {
                Ok(RegisterLocationChoice::Current) => match handler.set_as_default_handler() {
                    Ok(_) => {
                        info!("Successfully registered as default handler.");
                        println!("Successfully registered as default handler.");
                    }
                    Err(e) => {
                        error!("Failed to register as default handler: {}", e);
                        println!("Failed to register as default handler: {}", e);
                    }
                },
                Ok(RegisterLocationChoice::UseInstalled(installed_path)) => {
                    match register_from_binary(&installed_path) {
                        Ok(_) => {
                            info!(
                                "Successfully registered using installed binary at {}.",
                                installed_path.to_string_lossy()
                            );
                            println!("Successfully registered as default handler.");
                        }
                        Err(e) => {
                            error!("Failed to register installed binary: {}", e);
                            println!("Failed to register as default handler: {}", e);
                        }
                    }
                }
                Ok(RegisterLocationChoice::InstallAndRegister) => {
                    match install_to_best_practice_path() {
                        Ok(installed_path) => {
                            println!("Installed binary to {}", installed_path.to_string_lossy());
                            match register_from_binary(&installed_path) {
                                Ok(_) => {
                                    info!(
                                        "Successfully installed and registered from {}.",
                                        installed_path.to_string_lossy()
                                    );
                                    println!(
                                        "Successfully installed and registered as default handler."
                                    );
                                }
                                Err(e) => {
                                    error!("Failed to register installed binary: {}", e);
                                    println!(
                                        "Installation succeeded, but registration failed: {}",
                                        e
                                    );
                                }
                            }
                        }
                        Err(e) => {
                            error!("Failed to install binary: {}", e);
                            println!("Failed to install binary: {}", e);
                            std::process::exit(1);
                        }
                    }
                }
                Err(e) => {
                    error!("Failed to choose register location: {}", e);
                    println!("Failed to choose register location: {}", e);
                    std::process::exit(1);
                }
            }
        }
        Some(Commands::Unregister) => {
            info!("Unregistering as default browser handler...");
            match handler.unregister_handler() {
                Ok(_) => {
                    info!("Successfully unregistered as default handler.");
                    println!("Successfully unregistered as default handler.");
                }
                Err(e) => {
                    error!("Failed to unregister as default handler: {}", e);
                    println!("Failed to unregister as default handler: {}", e);
                }
            }
        }
        Some(Commands::Uninstall) => {
            info!("Uninstalling application from best-practice path...");
            match uninstall_best_practice_install() {
                Ok(uninstalled_path) => {
                    info!(
                        "Successfully uninstalled best-practice binary from {}.",
                        uninstalled_path.to_string_lossy()
                    );
                    println!(
                        "Successfully uninstalled best-practice binary from {}.",
                        uninstalled_path.to_string_lossy()
                    );
                }
                Err(e) => {
                    error!("Failed to uninstall best-practice binary: {}", e);
                    println!("Failed to uninstall best-practice binary: {}", e);
                    std::process::exit(1);
                }
            }
        }
        Some(Commands::OpenSettings) => {
            info!("Opening Windows Default Apps settings...");
            println!("Please add this app as a default browser handler for HTTP and HTTPS.");
            std::thread::sleep(std::time::Duration::from_secs(2));
            match Command::new("cmd")
                .args(["/C", "start ms-settings:defaultapps"])
                .spawn()
            {
                Ok(_) => info!("Windows Default Apps settings opened successfully."),
                Err(e) => error!("Failed to open Windows Default Apps settings: {}", e),
            }
        }
        Some(Commands::Doctor) => {
            doctor_cmd();
        }
        Some(Commands::TestHandler { url }) => {
            let test_url = url.unwrap_or_else(|| "http://example.com".to_string());
            test_handler_cmd(&test_url);
        }
        None => {
            // No subcommand provided, but maybe a URL
            if let Some(url) = url {
                handle_url_open(url);
            }
        }
    }
}

// Handles opening a URL, including rule matching and browser selection
fn handle_url_open(url: String) {
    let handler = Handler;
    let rules = match read_rules() {
        Ok(rules) => rules,
        Err(e) => {
            error!("Failed to read rules: {}", e);
            Vec::new()
        }
    };
    info!("Loaded rules: {:?}", rules);

    let browsers_set: std::collections::HashSet<String> =
        handler.find_browsers().into_iter().collect();
    let mut browsers: Vec<String> = browsers_set.into_iter().collect();
    browsers.sort();
    info!("Detected browsers: {:?}", browsers);

    let mut matched_browser_path: Option<String> = None;

    for rule in &rules {
        let is_regex = rule.is_regex.unwrap_or(false);

        if is_regex {
            match Regex::new(&rule.pattern) {
                Ok(re) => {
                    if re.is_match(&url) {
                        matched_browser_path = browsers
                            .iter()
                            .find(|browser_path: &&String| {
                                browser_path
                                    .to_lowercase()
                                    .contains(&rule.browser.to_lowercase())
                            })
                            .cloned();
                        if matched_browser_path.is_some() {
                            break;
                        }
                    }
                }
                Err(e) => {
                    error!("Invalid regex pattern '{}': {}", rule.pattern, e);
                }
            }
        } else if url.contains(&rule.pattern) {
            matched_browser_path = browsers
                .iter()
                .find(|browser_path: &&String| {
                    browser_path
                        .to_lowercase()
                        .contains(&rule.browser.to_lowercase())
                })
                .cloned();
            if matched_browser_path.is_some() {
                break;
            }
        }
    }

    if let Some(browser_path) = matched_browser_path {
        info!("Launching browser: {} with URL: {}", browser_path, url);
        match spawn_browser_process(&browser_path, &url) {
            Ok(_) => info!("Browser launched successfully."),
            Err(e) => error!("Failed to launch browser: {}", e),
        }
    } else {
        // No rule matched, present browser selection
        warn!("No rule matched for URL: {}", url);
        #[cfg(windows)]
        ensure_console_window();
        if browsers.is_empty() {
            error!("No browsers detected to open the URL.");
        } else if !io::stdin().is_terminal() {
            match prompt_browser_selection_slint(&url, &browsers) {
                GuiChooserOutcome::Selected {
                    browser_path,
                    save_rule,
                } => {
                    launch_browser_with_optional_rule(&url, &browser_path, save_rule);
                }
                GuiChooserOutcome::Cancelled => {
                    info!("Browser selection cancelled.");
                }
                GuiChooserOutcome::Unavailable => {
                    if let Some(browser_path) = browsers.first().cloned() {
                        info!(
                            "Slint chooser unavailable; launching fallback browser: {} with URL: {}",
                            browser_path, url
                        );
                        println!(
                            "No interactive terminal is available. Opening the URL in {}.",
                            get_browser_name_from_path(&browser_path)
                        );
                        match spawn_browser_process(&browser_path, &url) {
                            Ok(_) => info!("Fallback browser launched successfully."),
                            Err(e) => error!("Failed to launch fallback browser: {}", e),
                        }
                    }
                }
            }
        } else {
            println!("URL: {}", url);
            info!("Detected browsers:");
            for (i, browser_path) in browsers.iter().enumerate() {
                let browser_name = get_browser_name_from_path(browser_path);
                println!("{}: {}", i + 1, browser_name);
            }

            println!(
                "Enter the number of the browser to use (e.g., '1'), or '1s' to save as a rule, or 'cancel':"
            );
            let mut selection = String::new();
            if let Err(e) = io::stdout().flush() {
                error!("Failed to flush stdout before prompt: {}", e);
                return;
            }
            match io::stdin().read_line(&mut selection) {
                Ok(0) => {
                    let fallback_browser = browsers.first().cloned();
                    if let Some(browser_path) = fallback_browser {
                        info!(
                            "EOF received while waiting for browser selection; launching fallback browser: {} with URL: {}",
                            browser_path, url
                        );
                        println!(
                            "No input was received. Opening the URL in {}.",
                            get_browser_name_from_path(&browser_path)
                        );
                        match spawn_browser_process(&browser_path, &url) {
                            Ok(_) => info!("Fallback browser launched successfully."),
                            Err(e) => error!("Failed to launch fallback browser: {}", e),
                        }
                    }
                    return;
                }
                Err(e) => {
                    error!("Failed to read browser selection: {}", e);
                    return;
                }
                Ok(_) => {}
            }
            let selection = selection.trim().to_lowercase();

            if selection == "cancel" {
                info!("Browser selection cancelled.");
            } else {
                let save_rule = selection.ends_with('s');
                let selection_str = if save_rule {
                    &selection[..selection.len() - 1]
                } else {
                    &selection
                };

                if let Ok(index) = selection_str.parse::<usize>() {
                    if index > 0 && index <= browsers.len() {
                        let selected_browser_path = &browsers[index - 1];
                        info!(
                            "Launching selected browser: {} with URL: {}",
                            selected_browser_path, url
                        );
                        match spawn_browser_process(selected_browser_path, &url) {
                            Ok(_) => {
                                info!("Browser launched successfully.");

                                if save_rule {
                                    if let Some(domain) = url::Url::parse(&url)
                                        .ok()
                                        .and_then(|u| u.domain().map(|d| d.to_string()))
                                    {
                                        info!(
                                            "Adding rule for domain: {} with browser: {}",
                                            domain, selected_browser_path
                                        );
                                        match add_rule(domain, selected_browser_path.clone(), false)
                                        {
                                            Ok(_) => info!("Rule added successfully."),
                                            Err(e) => {
                                                error!("Failed to add rule: {}", e)
                                            }
                                        }
                                    } else {
                                        error!(
                                            "Could not extract domain from URL to save rule: {}",
                                            url
                                        );
                                    }
                                }
                            }
                            Err(e) => error!("Failed to launch browser: {}", e),
                        }
                    } else {
                        warn!("Invalid selection number: {}", selection_str);
                    }
                } else {
                    warn!("Invalid input format: {}", selection);
                }
            }
        }
    }
}

fn launch_browser_with_optional_rule(url: &str, browser_path: &str, save_rule: bool) {
    info!(
        "Launching selected browser: {} with URL: {}",
        browser_path, url
    );
    match spawn_browser_process(browser_path, url) {
        Ok(_) => {
            info!("Browser launched successfully.");

            if save_rule {
                if let Some(domain) = url::Url::parse(url)
                    .ok()
                    .and_then(|u| u.domain().map(|d| d.to_string()))
                {
                    info!(
                        "Adding rule for domain: {} with browser: {}",
                        domain, browser_path
                    );
                    match add_rule(domain, browser_path.to_string(), false) {
                        Ok(_) => info!("Rule added successfully."),
                        Err(e) => error!("Failed to add rule: {}", e),
                    }
                } else {
                    error!("Could not extract domain from URL to save rule: {}", url);
                }
            }
        }
        Err(e) => error!("Failed to launch browser: {}", e),
    }
}

fn spawn_browser_process(browser_spec: &str, url: &str) -> Result<(), std::io::Error> {
    if let Some(app_id) = browser_spec.strip_prefix("flatpak:") {
        Command::new("flatpak")
            .arg("run")
            .arg(app_id)
            .arg(url)
            .spawn()
            .map(|_| ())
    } else {
        Command::new(browser_spec).arg(url).spawn().map(|_| ())
    }
}

// Parses interactive input into a Commands variant
fn parse_interactive_command(input: &str) -> Result<Commands, &'static str> {
    let parts: Vec<&str> = input.split_whitespace().collect();
    if parts.is_empty() {
        return Err("No command entered");
    }
    match parts[0] {
        "add" => {
            // Usage: add <pattern> <browser> [--regex]
            if parts.len() < 3 {
                return Err("Usage: add <pattern> <browser> [--regex]");
            }
            let pattern = parts[1].to_string();
            let browser = parts[2].to_string();
            let regex = parts.contains(&"--regex");
            Ok(Commands::Add {
                pattern,
                browser,
                regex,
            })
        }
        "list" => Ok(Commands::List),
        "remove" => {
            if parts.len() < 2 {
                return Err("Usage: remove <pattern>");
            }
            Ok(Commands::Remove {
                pattern: parts[1].to_string(),
            })
        }
        "import" => {
            if parts.len() < 2 {
                return Err("Usage: import <path>");
            }
            Ok(Commands::Import {
                path: parts[1].to_string(),
            })
        }
        "export" => {
            if parts.len() < 2 {
                return Err("Usage: export <path>");
            }
            Ok(Commands::Export {
                path: parts[1].to_string(),
            })
        }
        "install" => Ok(Commands::Install),
        "register" => Ok(Commands::Register),
        "unregister" => Ok(Commands::Unregister),
        "uninstall" => Ok(Commands::Uninstall),
        "open-settings" => Ok(Commands::OpenSettings),
        "doctor" => Ok(Commands::Doctor),
        "test-handler" => {
            let url = parts.get(1).map(|s| s.to_string());
            Ok(Commands::TestHandler { url })
        }
        _ => Err("Unknown command. Type 'help' for commands."),
    }
}

#[cfg(target_os = "linux")]
fn doctor_cmd() {
    println!("\nDesktop URL handler diagnostics");
    println!("==============================\n");

    match std::env::current_exe() {
        Ok(exe) => println!("Current executable: {}", exe.to_string_lossy()),
        Err(e) => eprintln!("warning: Failed to determine current executable: {}", e),
    }

    // Check desktop file
    if let Some(data_dir) = dirs::data_dir() {
        let desktop_file = data_dir.join("applications/rust-browser-handler.desktop");
        if desktop_file.exists() {
            if let Ok(content) = fs::read_to_string(&desktop_file) {
                if let Some(exec_line) = content.lines().find(|l| l.starts_with("Exec=")) {
                    println!("Desktop entry: {}", exec_line);
                } else {
                    println!("Desktop entry: missing Exec line");
                }
            }
        } else {
            println!("Desktop entry: NOT FOUND");
        }
    }

    // Check xdg-mime
    let check_mime = |scheme: &str| -> String {
        let output = Command::new("xdg-mime")
            .args(["query", "default", &format!("x-scheme-handler/{}", scheme)])
            .output();
        match output {
            Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).trim().to_string(),
            _ => "<unavailable>".to_string(),
        }
    };
    println!("xdg-mime HTTP: {}", check_mime("http"));
    println!("xdg-mime HTTPS: {}", check_mime("https"));

    let check_gio = |scheme: &str| -> String {
        let output = Command::new("gio")
            .args(["mime", &format!("x-scheme-handler/{}", scheme)])
            .output();
        match output {
            Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).trim().to_string(),
            _ => "<unavailable>".to_string(),
        }
    };
    println!("gio HTTP: {}", check_gio("http"));
    println!("gio HTTPS: {}", check_gio("https"));

    // Check VS Code settings
    if let Some(config_dir) = dirs::config_dir() {
        let settings_file = config_dir.join("Code/User/settings.json");
        if settings_file.exists()
            && let Ok(content) = fs::read_to_string(&settings_file)
            && let Ok(json) = serde_json::from_str::<Value>(&content)
        {
            if let Some(browser) = json.get("workbench.externalBrowser") {
                println!("VS Code externalBrowser: {}", browser);
            } else {
                println!("VS Code externalBrowser: <unset>");
            }
        }
    }
}

#[cfg(not(target_os = "linux"))]
fn doctor_cmd() {
    println!("Doctor diagnostics available on Linux only.");
}

#[cfg(target_os = "linux")]
fn test_handler_cmd(test_url: &str) {
    println!("\nTesting handler invocation chain");
    println!("================================\n");

    match std::env::current_exe() {
        Ok(exe) => {
            println!("[1] Direct binary execution (no URL open):");
            match Command::new(&exe).arg("doctor").output() {
                Ok(output) => {
                    if output.status.success() {
                        println!("  ✓ Binary executed successfully");
                    } else {
                        println!("  ✗ Binary failed with exit code {}", output.status);
                    }
                }
                Err(e) => println!("  ✗ Error: {}", e),
            }
        }
        Err(e) => {
            println!("[1] Direct binary execution (no URL open):");
            println!("  ✗ Skipped: failed to determine executable path ({})", e);
        }
    }

    println!("\n[2] xdg-open invocation:");
    match Command::new("xdg-open").arg(test_url).output() {
        Ok(output) => {
            if output.status.success() {
                println!("  ✓ xdg-open succeeded");
            } else {
                println!("  ✗ xdg-open failed (exit code {})", output.status);
                let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
                if !stderr.is_empty() {
                    println!("  stderr: {}", stderr);
                }
            }
        }
        Err(e) => println!("  ✗ Error: {}", e),
    }
}

#[cfg(not(target_os = "linux"))]
fn test_handler_cmd(_url: &str) {
    println!("Test handler available on Linux only.");
}

fn install_to_best_practice_path() -> Result<PathBuf, Box<dyn std::error::Error>> {
    let current_exe = std::env::current_exe()?;

    #[cfg(target_os = "linux")]
    let target_dir = {
        let home_dir = dirs::home_dir().ok_or("Could not determine home directory")?;
        home_dir.join(".local").join("bin")
    };

    #[cfg(target_os = "windows")]
    let target_dir = {
        let local_data =
            dirs::data_local_dir().ok_or("Could not determine local data directory")?;
        local_data.join("Programs").join("RustBrowserHandler")
    };

    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    let target_dir = {
        let home_dir = dirs::home_dir().ok_or("Could not determine home directory")?;
        home_dir.join(".local").join("bin")
    };

    fs::create_dir_all(&target_dir)?;

    #[cfg(target_os = "windows")]
    let target_path = target_dir.join("rust_browser_handler.exe");

    #[cfg(not(target_os = "windows"))]
    let target_path = target_dir.join("rust_browser_handler");

    if current_exe != target_path {
        fs::copy(&current_exe, &target_path)?;
    }

    #[cfg(unix)]
    {
        let mut permissions = fs::metadata(&target_path)?.permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&target_path, permissions)?;
    }

    Ok(target_path)
}

fn best_practice_install_path() -> Result<PathBuf, Box<dyn std::error::Error>> {
    #[cfg(target_os = "linux")]
    {
        let home_dir = dirs::home_dir().ok_or("Could not determine home directory")?;
        Ok(home_dir
            .join(".local")
            .join("bin")
            .join("rust_browser_handler"))
    }

    #[cfg(target_os = "windows")]
    {
        let local_data =
            dirs::data_local_dir().ok_or("Could not determine local data directory")?;
        Ok(local_data
            .join("Programs")
            .join("RustBrowserHandler")
            .join("rust_browser_handler.exe"))
    }

    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        let home_dir = dirs::home_dir().ok_or("Could not determine home directory")?;
        Ok(home_dir
            .join(".local")
            .join("bin")
            .join("rust_browser_handler"))
    }
}

enum RegisterLocationChoice {
    Current,
    InstallAndRegister,
    UseInstalled(PathBuf),
}

fn choose_register_location() -> Result<RegisterLocationChoice, Box<dyn std::error::Error>> {
    let current_exe = std::env::current_exe()?;
    let expected_path = best_practice_install_path()?;

    if current_exe == expected_path {
        return Ok(RegisterLocationChoice::Current);
    }

    if !io::stdin().is_terminal() {
        println!();
        if expected_path.exists() {
            println!(
                "Non-interactive session detected. Using the installed best-practice copy at {}.",
                expected_path.to_string_lossy()
            );
            return Ok(RegisterLocationChoice::UseInstalled(expected_path));
        }

        println!(
            "Non-interactive session detected. Installing to the best-practice path at {} and registering that copy.",
            expected_path.to_string_lossy()
        );
        return Ok(RegisterLocationChoice::InstallAndRegister);
    }

    println!();
    if expected_path.exists() {
        println!(
            "The best-practice install already exists at {}.",
            expected_path.to_string_lossy()
        );
    } else {
        println!(
            "The best-practice install path is {}.",
            expected_path.to_string_lossy()
        );
    }
    println!(
        "You are currently running {}.",
        current_exe.to_string_lossy()
    );
    println!("1) Install to the best-practice path and register that copy");
    println!("2) Use the current location and register it as-is");
    print!("Select 1 or 2 [2]: ");
    io::stdout().flush()?;

    let mut selection = String::new();
    let bytes_read = io::stdin().read_line(&mut selection)?;
    if bytes_read == 0 {
        if expected_path.exists() {
            println!(
                "No input received. Using the installed best-practice copy at {}.",
                expected_path.to_string_lossy()
            );
            return Ok(RegisterLocationChoice::UseInstalled(expected_path));
        }

        println!(
            "No input received. Installing to the best-practice path at {} and registering that copy.",
            expected_path.to_string_lossy()
        );
        return Ok(RegisterLocationChoice::InstallAndRegister);
    }
    let selection = selection.trim();

    if selection == "1" {
        Ok(RegisterLocationChoice::InstallAndRegister)
    } else {
        Ok(RegisterLocationChoice::Current)
    }
}

fn register_from_binary(binary_path: &std::path::Path) -> Result<(), Box<dyn std::error::Error>> {
    let status = Command::new(binary_path).arg("register").status()?;

    if status.success() {
        Ok(())
    } else {
        Err(format!(
            "Installed binary returned non-zero exit status during register: {}",
            status
        )
        .into())
    }
}

fn uninstall_best_practice_install() -> Result<PathBuf, Box<dyn std::error::Error>> {
    let target_path = best_practice_install_path()?;

    if target_path.exists() {
        fs::remove_file(&target_path)?;
    }

    Ok(target_path)
}

#[cfg(windows)]
fn ensure_console_window() {
    use winapi::um::wincon::GetConsoleWindow;
    use winapi::um::winuser::{SW_SHOW, SetForegroundWindow, ShowWindow};

    unsafe {
        // Show and bring the console window to the foreground if it exists
        let hwnd = GetConsoleWindow();
        if !hwnd.is_null() {
            ShowWindow(hwnd, SW_SHOW);
            SetForegroundWindow(hwnd);
        }
    }
}
