# Rust Browser Handler (Command-Line)

This project is a Rust-based command-line application that acts as a cross-platform browser handler. It allows users to intercept link clicks and apply rules to automatically open links in a specific browser or interactively select a browser. Supports both Windows (full default handler integration) and Linux (XDG desktop integration).

## Features

- **Cross-Platform Browser Handling:** Intercepts http and https links when set as the default handler (Windows) or integrates with desktop settings (Linux/XDG).
- **Rule-Based Automation:** Allows users to define rules based on URL patterns (substring or regular expressions) to automatically open links in a specific browser without manual intervention. Rules are stored in a `rules.json` file in the user's configuration directory.
- **Interactive Browser Selection:** If no rule matches an intercepted URL, the application presents a list of detected browsers and prompts the user to select one. The user can also choose to save this selection as a new rule for the URL's domain.
- **Command-Line Interface (CLI):** Provides a non-graphical interface for managing browser selection rules and preferences. Can be used via subcommands or an interactive mode.
- **Platform-Specific Integration:**
  - **Windows:** Full registry-based default handler registration
  - **Linux:** XDG desktop integration via xdg-mime and .desktop files

## Installation

### Download
Download the appropriate release for your platform from the [GitHub Releases](https://github.com/axellpadilla/rust-browser-handler/releases) page.

### Windows
1. Download `rust_browser_handler_windows.zip`
2. **Extract to a permanent location** (recommended: `C:\Program Files\RustBrowserHandler\`)
3. Execute `rust_browser_handler.exe register` to register as the default browser handler
4. Use the `open-settings` command to verify registration or access Windows Default Apps settings

### Linux
1. Download `rust_browser_handler_linux.tar.gz`
2. **Extract to a permanent location** (recommended: `~/.local/bin/` or `/usr/local/bin/`)
3. Execute `./rust_browser_handler register` to register as the default browser handler
   - If run from a non-installed location, the app automatically installs (or updates) itself to `~/.local/bin/` before registering
   - Set `RUST_BROWSER_HANDLER_NO_AUTO_INSTALL` to any value to opt out of auto-install
4. The application will integrate with your desktop environment using XDG standards

**Important:** Always place the executable in a permanent location before registering, as the system integration (registry entries on Windows, .desktop files on Linux) will reference that specific path.

## Rules File Format

Rules are stored in a JSON file named `rules.json` in your user's configuration directory (e.g., `%APPDATA%\Roaming\RustBrowserHandler\rules.json` on Windows, `~/.config/rust_browser_handler/rules.json` on Linux).

Each rule is a JSON object with the following fields:

- `pattern` (string): The substring or regular expression to match against URLs.
- `browser` (string): The path or identifier of the browser to use.
- `is_regex` (boolean, optional): If `true`, the `pattern` is treated as a regular expression. If omitted or `false`, the pattern is matched as a substring.

**Example `rules.json`:**
```json
[
  {
    "pattern": "work.com",
    "browser": "/usr/bin/google-chrome"
  },
  {
    "pattern": ".*\\.internal\\.net",
    "browser": "/usr/bin/firefox",
    "is_regex": true
  }
]
```

If `is_regex` is not specified, the rule defaults to substring matching.

## Rules File Format

Rules are stored in a JSON file named `rules.json` in your user's configuration directory (e.g., `%APPDATA%\Roaming\RustBrowserHandler\rules.json` on Windows).

Each rule is a JSON object with the following fields:

- `pattern` (string): The substring or regular expression to match against URLs.
- `browser` (string): The path or identifier of the browser to use.
- `is_regex` (boolean, optional): If `true`, the `pattern` is treated as a regular expression. If omitted or `false`, the pattern is matched as a substring.

**Example `rules.json`:**
```json
[
  {
    "pattern": "work.com",
    "browser": "C:/Program Files/Google/Chrome/Application/chrome.exe"
  },
  {
    "pattern": ".*\\.internal\\.net",
    "browser": "C:/Program Files/Mozilla Firefox/firefox.exe",
    "is_regex": true
  }
]
]
```

If `is_regex` is not specified, the rule defaults to substring matching.

## Usage

### Portable Version

1. Download the ZIP file
2. Extract to a permanent location
3. Execute `rust_browser_handler.exe` to add rules.
4. Use the `open-settings` command to set this .exe as the default handler.

### Building

#### Linux Build (Native)

To build for Linux, navigate to the project directory in your terminal and run:

```bash
cargo build --release
```

This will generate an executable file in the `target/release/` directory.

#### Windows Build (Cross-Compilation)

To build the release version of the application for Windows, navigate to the project directory in your terminal and run:

```bash
cargo build --release --target x86_64-pc-windows-gnu
```

This will generate an executable file (likely `rust_browser_handler.exe` in the `target/x86_64-pc-windows-gnu/release/` directory).

**Note:** This project supports cross-compilation from Linux to Windows due to Windows-specific dependencies. The devcontainer is configured for this automatically.

### Registration

#### Windows
To register the application as the default handler for http and https protocols, run the executable with the `register` subcommand:

```bash
# First, place the executable in a permanent location (recommended: C:\Program Files\RustBrowserHandler\)
# Then register it as the default handler
target\x86_64-pc-windows-gnu\release\rust_browser_handler.exe register
```

To install to a best-practice Windows user path first:

```powershell
# Run from your current extracted/build location
rust_browser_handler.exe install

# Then register from the installed location
& "$env:LOCALAPPDATA\\Programs\\RustBrowserHandler\\rust_browser_handler.exe" register

# Remove the installed copy and unregister the handler
rust_browser_handler.exe uninstall
```

**Important:** Place the executable in a permanent location before registering, as the registry entries will point to that specific path.

This performs full registry integration to become the system default browser handler.

#### Linux (XDG)
On Linux systems, registration uses XDG standards for cross-desktop compatibility:

```bash
# register auto-installs to ~/.local/bin/ if run from elsewhere (unless RUST_BROWSER_HANDLER_NO_AUTO_INSTALL is set)
./rust_browser_handler register
```

To install to a best-practice Linux user path manually:

```bash
# Run from your current extracted/build location
./rust_browser_handler install

# Then this command is available from ~/.local/bin if it is in your PATH
rust_browser_handler register

# Remove the installed copy only
rust_browser_handler uninstall
```

**Note:** The `register` command auto-installs (or updates) itself to `~/.local/bin/` before registering when run from a non-best-practice location. Set `RUST_BROWSER_HANDLER_NO_AUTO_INSTALL` to any value to disable this behavior and fall back to interactive prompting.

This creates a `.desktop` file in `~/.local/share/applications/` and uses `xdg-mime` to associate the application with HTTP and HTTPS URL schemes. This works across desktop environments (GNOME, KDE, XFCE, COSMIC, etc.) that follow freedesktop.org standards.

For Ubuntu GNOME and Pop!_OS (GNOME or COSMIC), the freedesktop/XDG association is the canonical integration layer:

```bash
xdg-mime query default x-scheme-handler/http
xdg-mime query default x-scheme-handler/https
xdg-settings get default-web-browser
```

Expected result for the first two commands:

```text
rust-browser-handler.desktop
```

If COSMIC or GNOME Settings UI does not immediately show the app in Default Applications, log out and log back in after registration.

Alternatively, you can run the executable without arguments to enter interactive mode and use the `register` command there.

If you run `register` from a non-installed binary, the app automatically installs (or updates) itself to `~/.local/bin/` before registering, unless `RUST_BROWSER_HANDLER_NO_AUTO_INSTALL` is set. Use `unregister` separately if you want to clear the browser association.

### Rule Management

Rules are stored in a `rules.json` file in your user's configuration directory:
- **Windows:** `%APPDATA%\Roaming\RustBrowserHandler\rules.json`
- **Linux:** `~/.config/rust_browser_handler/rules.json`

You can manage rules using the command-line interface in two ways:

1.  **Direct Commands:** Run the executable with a subcommand and arguments:

    ```bash
    # Linux examples
    ./target/release/rust_browser_handler add "work.com" "/usr/bin/google-chrome"
    ./target/release/rust_browser_handler add ".*\.internal\.net" "/usr/bin/firefox" --regex
    ./target/release/rust_browser_handler list

    # Windows examples
    target\x86_64-pc-windows-gnu\release\rust_browser_handler.exe add "work.com" "C:/Program Files/Google/Chrome/Application/chrome.exe"
    target\x86_64-pc-windows-gnu\release\rust_browser_handler.exe add ".*\.internal\.net" "C:/Program Files/Mozilla Firefox/firefox.exe" --regex
    target\x86_64-pc-windows-gnu\release\rust_browser_handler.exe list

    # Remove a rule by pattern (works on both platforms)
    rust_browser_handler remove "work.com"

    # Import rules from a file
    rust_browser_handler import "path/to/your/rules.json"

    # Export rules to a file
    rust_browser_handler export "path/to/save/rules.json"

    # Windows: Open Windows Settings to manage default handlers
    rust_browser_handler open-settings
    ```

2.  **Interactive Mode:** Run the executable without any arguments to enter an interactive mode:

    ```bash
    # Linux
    ./target/release/rust_browser_handler

    # Windows
    target\x86_64-pc-windows-gnu\release\rust_browser_handler.exe
    ```

    In interactive mode, type commands like `add`, `list`, `remove`, `import`, `export`, `register`, and `exit`. Type `help` for a list of commands and their usage within the interactive mode.

### Intercepting URLs

#### Windows
Once registered, simply click on an http or https link from any application.

- If a rule matches the URL, the corresponding browser will be launched automatically.
- If no rule matches, the application will list detected browsers and prompt you to select one. You can type the browser number to open the link in that browser, or type the number followed by 's' (e.g., '1s') to open in that browser and save a rule for the URL's domain.

#### Linux
On Linux, URL interception uses XDG standards for cross-desktop compatibility. The application can be set as the default browser using `xdg-mime`, and links will be handled according to your rules.

- If a rule matches the URL, the corresponding browser will be launched automatically.
- If no rule matches, the application will list detected browsers and prompt you to select one.

## Future Development

- **macOS Support:** Implement platform handler for macOS with native integration.
- **Improved Browser Detection:** Further refine the process of automatically detecting installed browsers across all supported platforms.
- **Enhanced System Integration:** Deeper integration with system settings for a more seamless user experience on all platforms.
- **More Advanced Rule Options:** Explore additional rule criteria beyond URL patterns.

## Development Setup

This project uses a standardized development environment with linting, formatting, and commit message conventions.

### Dev Container Setup (Recommended)

This project includes a dev container configuration for a consistent development environment. To use it:

1. Ensure you have VS Code with the "Dev Containers" extension installed.
2. Open the project in VS Code.
3. When prompted, click "Reopen in Container" or run `Dev Containers: Reopen in Container` from the command palette.
4. The dev container will set up the Rust environment with cross-compilation support for Windows targets.

The dev container includes:
- Rust toolchain with Windows cross-compilation targets
- MinGW cross-compiler for linking Windows binaries
- VS Code extensions for Rust development (rust-analyzer, TOML support, LLDB debugger)

### Quick Setup

After cloning this repository, the dev container will automatically run the setup. If developing locally (not recommended), run:

```bash
make setup-dev
```

This will:
1. Configure Git to use our commit template
2. Install pre-commit hooks for linting and commit message validation
3. Install necessary development tools (rustfmt, clippy, cargo-watch)

### Development Commands

Check the `Makefile` for available commands.

### Commit Message Format

This project uses Gitmoji Conventional Commits format. The commit template will guide you, but here's a quick example:

```
:sparkles: feat(auth): add login functionality
```

See `.github/commit-template.txt` for more details.

```