//! File-manager integration and recycle bin.

use std::path::Path;

use crate::{PlatformError, Result};

/// Open a folder in the system file manager.
pub fn open_folder(path: &Path) -> Result<()> {
    if !path.exists() {
        return Err(PlatformError::Other(format!("{} is not available (storage offline?)", path.display())));
    }
    open::that_detached(path).map_err(PlatformError::Io)
}

/// Open a file with its default associated application.
pub fn open_file(path: &Path) -> Result<()> {
    if !path.exists() {
        return Err(PlatformError::Other(format!("{} is not available (storage offline?)", path.display())));
    }
    open::that_detached(path).map_err(PlatformError::Io)
}


/// Reveal (select) a file or folder in the system file manager.
pub fn reveal(path: &Path) -> Result<()> {
    if !path.exists() {
        return Err(PlatformError::Other(format!("{} is not available (storage offline?)", path.display())));
    }
    #[cfg(windows)]
    {
        // explorer.exe is the shell itself; /select highlights the item.
        let mut arg = std::ffi::OsString::from("/select,");
        arg.push(path.as_os_str());
        std::process::Command::new("explorer.exe").arg(arg).spawn()?;
        Ok(())
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("/usr/bin/open").arg("-R").arg(path).spawn()?;
        Ok(())
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        let target = if path.is_dir() { path } else { path.parent().unwrap_or(path) };
        open::that_detached(target).map_err(PlatformError::Io)
    }
}

/// Move a file or directory to the OS trash / recycle bin.
pub fn move_to_trash(path: &Path) -> Result<()> {
    trash::delete(path).map_err(|e| PlatformError::Trash(e.to_string()))
}

/// Whether the platform trash is expected to work for this path. Network
/// shares and some removable filesystems have no recycle bin; callers must
/// then ask for explicit permanent-deletion confirmation.
pub fn trash_supported(path: &Path) -> bool {
    path.exists()
}
