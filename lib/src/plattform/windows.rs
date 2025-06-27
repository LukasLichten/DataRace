use std::{fs::Permissions, path::PathBuf};

pub(crate) const DEFAULT_DASHBOARD_PATH: &'static str = "~\\AppData\\Roaming\\DataRace\\Dashboards";

pub(super) fn get_master_config_path() -> Option<PathBuf> {
    let mut folder = get_install_folder()?;
    folder.push(super::MASTER_CONFIG_FILE_NAME);

    Some(folder)
}

/// Yes, for Windows we are using the Documents folder,
/// directing people into AppData/Roaming to add Dashboards is a bit weird 
/// (to Winblows users at least).
///
/// Linux uses .config
pub(super) fn get_config_folder() -> Option<PathBuf> {
    let mut folder = dirs::document_dir()?;
    folder.push(super::FOLDER_NAME);

    Some(folder)
}

fn get_install_folder() -> Option<PathBuf> {
    let mut exe = std::env::current_exe().ok()?;
    exe.pop();
    Some(exe)
}

pub(super) const DEFAULT_MASTER_PLUGIN_LOCATIONS: [&'static str; 1] = [":\\Plugins"];
pub(super) const DEFAULT_USER_PLUGIN_LOCATIONS: [&'static str; 0] = [];

pub(super) fn validate_master_config_permissions(path: &PathBuf) -> bool {
    if let Ok(meta) = path.metadata() {
        // This is very limited and does not account for Admin permissions, which should be
        // required for writes
        meta.permissions().readonly()
    } else {
        false
    }
}

/// Used for setting up MasterConfig
pub(super) fn set_restricted_permissions(path: &PathBuf) -> Option<()> {
    let mut permissions = path.metadata().ok()?.permissions();
    permissions.set_readonly(true);

    std::fs::set_permissions(path, permissions).ok()?;
    
    // Again, no insurance that this requires Admin Permissions

    Some(())
}
