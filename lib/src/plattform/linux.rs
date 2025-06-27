//! Linux plattform specific functions

use std::{os::unix::fs::{MetadataExt, PermissionsExt}, path::PathBuf, str::FromStr};


pub(crate) const DEFAULT_DASHBOARD_PATH: &'static str = "~/.config/DataRace/Dashboards";

pub(super) fn get_master_config_path() -> Option<PathBuf> {
    PathBuf::from_str(format!("/etc/DataRace/{}",super::MASTER_CONFIG_FILE_NAME).as_str()).ok()
}

/// Linux we are using the .config folder, as it is the only logical option
///
/// On Windows we however use Documents instead, as this makes more sense for them
pub(super) fn get_config_folder() -> Option<PathBuf> {
    let mut folder = dirs::config_dir()?;
    folder.push(super::FOLDER_NAME);

    Some(folder)
}

pub(super) const DEFAULT_MASTER_PLUGIN_LOCATIONS: [&'static str; 1] = ["/usr/lib/DataRace"];
pub(super) const DEFAULT_USER_PLUGIN_LOCATIONS: [&'static str; 1] = ["~/.local/share/DataRace/Plugins"];

pub(super) fn validate_master_config_permissions(path: &PathBuf) -> bool {
    if let Ok(meta) = path.metadata() {
        // User and group id 0 for root
        meta.gid() == 0 
            && meta.uid() == 0
            // Mode 4 is Read-Only, %8 filters down to the all other users
            && meta.mode() % 8 == 4
    } else {
        false
    }
}

/// Used for setting up MasterConfig
pub(super) fn set_restricted_permissions(path: &PathBuf) -> Option<()> {
    std::os::unix::fs::chown(path, Some(0), Some(0)).ok()?;

    if path.is_file() {
        std::fs::set_permissions(path, PermissionsExt::from_mode(0o644)).ok()?;
    } else if path.is_dir() {
        std::fs::set_permissions(path, PermissionsExt::from_mode(0o755)).ok()?;
    }


    Some(())
}
