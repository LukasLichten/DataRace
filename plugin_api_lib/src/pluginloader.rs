use std::{path::PathBuf, fs};

use dlopen2::wrapper::{WrapperApi, Container, WrapperMultiApi};
use log::{error, info, debug};

use tokio::task::JoinSet;

use crate::{PluginHandle, utils};

pub async fn load_all_plugins() -> Result<JoinSet<()>,Box<dyn std::error::Error>> {
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

    let mut plugin_task_handles = JoinSet::<()>::new();


    if let Ok(mut res) = fs::read_dir(plugin_folder) {
        while let Some(Ok(item)) = res.next() {
            debug!("Found {} in plugin folder", item.path().to_str().unwrap());
            if item.path().extension().unwrap().to_str().unwrap() == ending {
                plugin_task_handles.spawn(run_plugin(item.path()));
            }
        }

    }

 
    Ok(plugin_task_handles)
}

async fn run_plugin(path: PathBuf) {
    if let Ok(wrapper) = unsafe { Container::<PluginWrapper>::load(path.to_str().unwrap()) } {
        let name = if let Some(ref name_handle) = wrapper.name {
            let ptr = name_handle.get_plugin_name();
            let n = utils::get_string(ptr).clone();
            
            name_handle.free_plugin_name(ptr);

            n
        } else {
            None
        };

        let name = if let Some(name) = name {
            name
        } else {
            path.file_stem().unwrap().to_str().unwrap().to_string()
        };

        let handle = PluginHandle { name };
        let ptr_h = Box::into_raw(Box::new(handle));
        wrapper.func.init(ptr_h);
    } else {
        // panic!("Test!");
    }
}

#[derive(WrapperMultiApi)]
pub struct PluginWrapper {
    name: Option<PluginNameWrapper>,
    func: PluginFuncWrapper
}

#[derive(WrapperApi)]
struct PluginNameWrapper {
    get_plugin_name: extern "C" fn() -> *mut libc::c_char,
    free_plugin_name: extern "C" fn(ptr: *mut libc::c_char),
}

#[derive(WrapperApi)]
struct PluginFuncWrapper {
    init: extern "C" fn(handle: *mut PluginHandle) -> libc::c_int
}
