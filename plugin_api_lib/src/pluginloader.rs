use std::{path::PathBuf, fs};

use log::{error, info};

pub fn load_all_plugins() -> Result<(),()> {
    let plugin_folder = PathBuf::from("./plugins/");

    if !plugin_folder.is_dir() {
        info!("Plugins folder did not exist, creating...");
        if !fs::create_dir(plugin_folder.as_path()).is_ok() {
            error!("Unable to create plugins folder! Exiting...");
            return Err(());
        }
    }



    Ok(())
}
