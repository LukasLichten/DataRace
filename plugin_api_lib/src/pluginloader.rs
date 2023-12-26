use std::{path::PathBuf, fs};

use dlopen2::wrapper::{WrapperApi, Container};
use log::{error, info, debug};

use tokio::task;

use crate::PluginHandle;

pub async fn load_all_plugins() -> Result<(),Box<dyn std::error::Error>> {
    let plugin_folder = PathBuf::from("./plugins/");

    if !plugin_folder.is_dir() {
        info!("Plugins folder did not exist, creating...");
        if let Err(e) = fs::create_dir(plugin_folder.as_path()) {
            error!("Unable to create plugins folder! Exiting...");
            // TODO turn this into message into the error returned
            return Err(Box::new(e));
        }
    }

    #[cfg(target_os = "linux")]
    let ending = "so";
    #[cfg(target_os = "windows")]
    let ending = "dll";


    if let Ok(mut res) = fs::read_dir(plugin_folder) {
        while let Some(Ok(item)) = res.next() {
            debug!("Found {} in plugin folder", item.path().to_str().unwrap());
            if item.path().extension().unwrap().to_str().unwrap() == ending {
                task::spawn(run_plugin(item.path().to_str().unwrap().to_string()));
            }
        }

    }

 
    Ok(())
}

async fn run_plugin(path: String) {
    if let Ok(wrapper) = unsafe { Container::<PluginWrapper>::load(path.as_str()) } {
        let handle = PluginHandle { name: path };
        let ptr_h = Box::into_raw(Box::new(handle));
        wrapper.init(ptr_h);
    }
}

#[derive(WrapperApi)]
pub struct PluginWrapper {
    init: fn(handle: *mut PluginHandle) -> libc::c_int
}
