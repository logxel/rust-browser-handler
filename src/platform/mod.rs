use std::error::Error;

pub trait PlatformHandler {
    fn find_browsers(&self) -> Vec<String>;
    fn set_as_default_handler(&self) -> Result<(), Box<dyn Error>>;
    fn unregister_handler(&self) -> Result<(), Box<dyn Error>>;
    #[allow(dead_code)]
    fn is_default_handler(&self) -> bool;

    /// Returns the OS-configured default browser's path (in the same format
    /// `find_browsers` uses), so it can be floated to the top of the
    /// selection list. `None` when it can't be determined, or when the
    /// default is this handler itself.
    fn default_browser_path(&self) -> Option<String> {
        None
    }
}

#[cfg(windows)]
mod windows_impl;
#[cfg(windows)]
pub use windows_impl::WindowsHandler as Handler;

#[cfg(unix)]
mod linux_impl;
#[cfg(unix)]
pub use linux_impl::LinuxHandler as Handler;

// Fallback for unsupported platforms
#[cfg(not(any(windows, unix)))]
pub struct UnsupportedHandler;
#[cfg(not(any(windows, unix)))]
impl PlatformHandler for UnsupportedHandler {
    fn find_browsers(&self) -> Vec<String> {
        vec![]
    }
    fn set_as_default_handler(&self) -> Result<(), Box<dyn Error>> {
        Err("Unsupported platform".into())
    }
    fn unregister_handler(&self) -> Result<(), Box<dyn Error>> {
        Err("Unsupported platform".into())
    }
    fn is_default_handler(&self) -> bool {
        false
    }
}
#[cfg(not(any(windows, unix)))]
pub use UnsupportedHandler as Handler;
