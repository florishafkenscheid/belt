//! Utility functions for BELT.

use std::{path::Path, time::Duration};

/// Helper function to turn a Duration into a nicely formatted string
pub fn format_duration(duration: Duration) -> String {
    let total_secs = duration.as_secs();

    if total_secs < 60 {
        format!("{total_secs}s")
    } else if total_secs < 3600 {
        let mins = total_secs / 60;
        let secs = total_secs % 60;
        format!("{mins}m{secs}s")
    } else {
        let hours = total_secs / 3600;
        let mins = (total_secs % 3600) / 60;
        format!("{hours}h{mins}m")
    }
}

#[cfg(unix)]
use std::fs;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

/// Check if a file is an executable.
pub fn is_executable(path: &Path) -> bool {
    // On unix, check the 'execute' permission bit
    #[cfg(unix)]
    {
        fs::metadata(path).is_ok_and(|metadata| {
            metadata.is_file() && (metadata.permissions().mode() & 0o111 != 0)
        })
    }

    #[cfg(windows)]
    {
        path.is_file()
            && path
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("exe"))
    }

    // Fallback for other operating systems.
    #[cfg(not(any(unix, windows)))]
    {
        metadata.is_file()
    }
}
