use std::fs;
use std::io;
use std::path::Path;
use std::process::Command;

fn main() {
    println!("🦀 Setting up development environment...");

    let is_devcontainer = is_running_in_devcontainer();

    // Track statuses for summary report
    let mut statuses: Vec<(String, String)> = Vec::new();

    // 1. Set up Git commit template
    match setup_commit_template() {
        Ok(()) => statuses.push(("Git commit template".to_string(), "configured".to_string())),
        Err(e) => {
            eprintln!("❌ Failed to set up commit template: {}", e);
            println!();
            print_summary(&[("Git commit template".to_string(), "failed".to_string())]);
            std::process::exit(1);
        }
    }

    // 2. Install pre-commit hook from repository
    match install_precommit_hook() {
        Ok(()) => statuses.push(("Pre-commit hook".to_string(), "installed".to_string())),
        Err(e) => {
            eprintln!("⚠️  Failed to install pre-commit hook: {} (continuing setup)", e);
            statuses.push(("Pre-commit hook".to_string(), "failed".to_string()));
        }
    }

    // 3. Install required development tools
    statuses.extend(install_dev_tools());

    // 4. Windows cross-compilation guidance
    if is_devcontainer {
        statuses.push(("Cross-compilation".to_string(), "already handled by devcontainer".to_string()));
    } else {
        print_cross_compilation_guidance();
        statuses.push(("Cross-compilation".to_string(), "guidance shown (not in devcontainer)".to_string()));
    }

    // Print summary
    println!();
    print_summary(&statuses);

    println!();
    println!("🚀 You can now start developing with:");
    println!("   - cargo build --target x86_64-pc-windows-gnu        # Build the project");
    println!("   - cargo test         # Run tests");
    println!("   - cargo run          # Run the application");
    println!("   - cargo dev          # Auto-rebuild and run on file changes");
}

fn is_running_in_devcontainer() -> bool {
    // Docker container indicator file
    if Path::new("/.dockerenv").exists() {
        return true;
    }
    // Explicit dev container env var
    if std::env::var("DEV_CONTAINER").is_ok() {
        return true;
    }
    // Common devcontainer mount point
    if Path::new("/workspaces").exists() {
        return true;
    }
    false
}

fn is_tool_available(tool: &str) -> bool {
    Command::new(tool)
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn setup_commit_template() -> io::Result<()> {
    println!("📝 Setting up Git commit template...");

    let status = Command::new("git")
        .args(["config", "--local", "commit.template", ".github/commit-template.txt"])
        .status()?;

    if !status.success() {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            "Git command failed to set commit template"
        ));
    }

    println!("✅ Git commit template configured successfully!");
    Ok(())
}

fn install_precommit_hook() -> io::Result<()> {
    println!("🔍 Installing pre-commit hook...");

    // Ensure .git/hooks directory exists
    let hooks_dir = Path::new(".git/hooks");
    if !hooks_dir.exists() {
        fs::create_dir_all(hooks_dir)?;
    }

    // Copy the pre-commit hook from the repository to .git/hooks
    let source_path = Path::new(".github/hooks/pre-commit");
    let target_path = hooks_dir.join("pre-commit");

    fs::copy(source_path, &target_path)?;

    // Make the hook executable on Unix-like systems
    #[cfg(not(windows))]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&target_path)?.permissions();
        perms.set_mode(0o755); // rwxr-xr-x
        if let Err(e) = fs::set_permissions(&target_path, perms) {
            eprintln!("⚠️  Could not set executable permissions on pre-commit hook: {} (hook may still work)", e);
        }
    }

    println!("✅ Pre-commit hook installed successfully!");
    Ok(())
}

fn install_dev_tools() -> Vec<(String, String)> {
    println!("🔧 Installing development tools...");

    let mut results: Vec<(String, String)> = Vec::new();

    // --- rustfmt and clippy via rustup ---
    if !is_tool_available("rustup") {
        eprintln!("⚠️  rustup not found — skipping Rust component installation. Install from https://rustup.rs");
        results.push(("rustfmt/clippy".to_string(), "skipped (rustup not found)".to_string()));
    } else {
        // Check if tools are already installed
        let rustfmt_installed = is_tool_available("rustfmt");
        let clippy_installed = is_tool_available("clippy-driver");

        if rustfmt_installed && clippy_installed {
            println!("✅ rustfmt and clippy are already installed");
            results.push(("rustfmt/clippy".to_string(), "already installed".to_string()));
        } else {
            let status = Command::new("rustup")
                .args(["component", "add", "rustfmt", "clippy"])
                .status();

            match status {
                Ok(s) if s.success() => {
                    println!("✅ rustfmt and clippy installed");
                    results.push(("rustfmt/clippy".to_string(), "installed".to_string()));
                }
                Ok(_) => {
                    eprintln!("⚠️  rustfmt/clippy installation returned non-zero exit code");
                    results.push(("rustfmt/clippy".to_string(), "installation failed".to_string()));
                }
                Err(e) => {
                    eprintln!("⚠️  Failed to run rustup: {}", e);
                    results.push(("rustfmt/clippy".to_string(), format!("error: {}", e)));
                }
            }
        }
    }

    // --- cargo-watch via cargo ---
    if !is_tool_available("cargo") {
        eprintln!("⚠️  cargo not found — skipping cargo-watch installation.");
        results.push(("cargo-watch".to_string(), "skipped (cargo not found)".to_string()));
    } else {
        let watch_check = Command::new("cargo")
            .args(["watch", "--version"])
            .output();

        let already_installed = match &watch_check {
            Ok(output) => output.status.success(),
            Err(_) => false,
        };

        if already_installed {
            println!("✅ cargo-watch is already installed");
            results.push(("cargo-watch".to_string(), "already installed".to_string()));
        } else {
            println!("📦 Installing cargo-watch...");
            let status = Command::new("cargo")
                .args(["install", "cargo-watch"])
                .status();

            match status {
                Ok(s) if s.success() => {
                    println!("✅ cargo-watch installed");
                    results.push(("cargo-watch".to_string(), "installed".to_string()));
                }
                Ok(_) => {
                    eprintln!("⚠️  cargo-watch installation returned non-zero exit code");
                    results.push(("cargo-watch".to_string(), "installation failed".to_string()));
                }
                Err(e) => {
                    eprintln!("⚠️  Failed to install cargo-watch: {}", e);
                    results.push(("cargo-watch".to_string(), format!("error: {}", e)));
                }
            }
        }
    }

    results
}

fn print_cross_compilation_guidance() {
    println!();
    println!("ℹ️  Running outside devcontainer — for Windows cross-compilation you may want:");
    println!("   rustup target add x86_64-pc-windows-gnu");
    println!("   # On Debian/Ubuntu: sudo apt install gcc-mingw-w64");
    println!("   # On Fedora: sudo dnf install mingw64-gcc");
    println!("   # On Arch: sudo pacman -S mingw-w64-gcc");
}

fn print_summary(statuses: &[(String, String)]) {
    for (item, status) in statuses {
        let icon = if status.starts_with("failed") || status.starts_with("error") || status.starts_with("installation failed") {
            "❌"
        } else if status.starts_with("skipped") {
            "⚠️ "
        } else if status.starts_with("guidance") || status.starts_with("already handled") {
            "ℹ️ "
        } else {
            "✅"
        };
        println!("{} {}: {}", icon, item, status);
    }
}
