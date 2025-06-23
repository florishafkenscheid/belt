use std::path::PathBuf;

pub fn get_default_factorio_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();

    if cfg!(target_os = "windows") {
        // Steam
        paths.push(PathBuf::from(
            r"C:\Program Files (x86)\Steam\steamapps\common\Factorio\bin\x64\factorio.exe",
        ));

        // Standalone
        paths.push(PathBuf::from(
            r"C:\Program Files\Factorio\bin\x64\factorio.exe",
        ));

        // User steam library (uncommon)
        if let Some(home) = dirs::home_dir() {
            paths.push(
                home.join(r"AppData\Local\Steam\steamapps\common\Factorio\bin\x64\factorio.exe"),
            );
        }
    } else if cfg!(target_os = "linux") {
        // User bin (symlinked, personally have this)
        if let Some(home) = dirs::home_dir() {
            paths.push(home.join(".local/bin/factorio"));
        }
        // System wide
        paths.push(PathBuf::from("/usr/local/bin/factorio"));
        paths.push(PathBuf::from("/usr/bin/factorio"));
        paths.push(PathBuf::from("/opt/factorio/bin/x64/factorio"));
        // Steam on Linux
        if let Some(home) = dirs::home_dir() {
            paths.push(home.join(".steam/steam/steamapps/common/Factorio/bin/x64/factorio"));
            paths.push(home.join(".local/share/Steam/steamapps/common/Factorio/bin/x64/factorio"));
        }
    } else if cfg!(target_os = "macos") {
        if let Some(home) = dirs::home_dir() {
            paths.push(home.join(
                "Library/Application Support/Steam/steamapps/common/Factorio/factorio.app/Contents/MacOS/factorio",
            ));
        }
        paths.push(PathBuf::from(
            "/Applications/factorio.app/Contents/MacOS/factorio",
        ));
    }

    paths
}

pub fn get_os_info() -> String {
    format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH)
}
