use std::sync::{atomic::AtomicBool, Arc};

use log::{info, error, debug};
use tokio::runtime::Builder;

pub const API_VERSION: u64 = 0;

mod built_info {
    include!(concat!(env!("OUT_DIR"), "/built.rs"));
}

mod datastore;

#[allow(dead_code, unused_variables, unused_imports)]
mod web;

mod pluginloader;
pub(crate) mod utils;

static mut IS_RUNTIME: bool = false;

/// Used by the main executable to start the programm
/// Do NOT call this as a plugin
#[no_mangle]
pub extern "C" fn run() {
    unsafe {
        IS_RUNTIME = true;
    }


    let log_level = log::LevelFilter::Debug;
    env_logger::builder().filter_level(log_level).init();

    if let Ok(rt) = Builder::new_multi_thread().enable_all().build() {
        let res = rt.block_on(internal_main());

        if let Err(e) = res {
            error!("DataRace crashed: {}", e);
        } else {
            info!("Shutting down...");
        }
        rt.shutdown_timeout(std::time::Duration::from_secs(2));
        info!("Done");
    } else {
        error!("Unable to launch tokio async runtime, aborting launch")
    }

}

async fn internal_main() -> Result<(), Box<dyn std::error::Error> > {
    info!("Launching DataRace...");
    let datastore: &'static tokio::sync::RwLock<datastore::DataStore>  = Box::leak(Box::new(datastore::DataStore::new()));

    let shutdown = Arc::new(AtomicBool::new(false));
    // let sh_clone = shutdown.clone();
    ctrlc::set_handler(move || {
        futures::executor::block_on(async {
            if shutdown.load(std::sync::atomic::Ordering::Acquire) {
                // we are already in a shutdown
                error!("Stop requested a second time, so we are now hard exiting");
                std::process::exit(1);
            }

            // We shut down everything
            let mut ds = datastore.write().await;
            ds.start_shutdown().await;
            drop(ds);

            shutdown.store(true, std::sync::atomic::Ordering::Release);
        });
    })?;

    let mut plugin_set = pluginloader::load_all_plugins(datastore).await?;

    // web::run_webserver(datastore, sh_clone).await?;
    

    // Stops the Runtime from closing when plugins are still running
    while let Some(res) = plugin_set.join_next().await {
        match res {
            Ok(_) => debug!("Some plugin finished"),
            Err(e) => error!("Plugin Runner Task (and it's contained Plugin) Crashed: {}", e)
        }
    }

    Ok(())
}

mod api_func;
mod api_types;
pub use api_func::*;
pub use api_types::*;

/// Do not call this function during runtime, it will return u64::MAX!
/// It serves for compiletime macros to access the API Version
///
/// This function acts differently to prevent plugins from changing their API version after they
/// were compiled.
/// However it exists to allow retrieval of the API version against which you are compiling
#[no_mangle]
pub extern "C" fn compiletime_get_api_version() -> u64 {
    if unsafe {
        !IS_RUNTIME
    } {
        API_VERSION
    } else {
        u64::MAX
    }
}

#[repr(C)]
pub struct PluginNameHash {
    pub id: u64,
    pub valid: bool
}

/// Do not call this function during runtime, it will return (id: 0, valid: false)!
/// It serves for compiletime macros to generate the plugin id from the plugin_hash
///
/// This function acts differently to prevent plugins from changing their id during runtime (and
/// invalidating their compiletime propertyhandles).
/// Although you can aquire this id from a get_propertyhandle request... Just please don't
///
/// This function also checks if the name does not contain any invalid characters (currently only .)
///
/// The cstring pointer has to be deallocated by you.
#[no_mangle]
pub extern "C" fn compiletime_get_plugin_name_hash(ptr: *mut libc::c_char) -> PluginNameHash {
    if unsafe {
        IS_RUNTIME
    } {
        return PluginNameHash { id: 0, valid: false };
    }

    if let Some(str) = utils::get_string(ptr) {
        let str = str.to_lowercase();
        if let Some(val) = utils::generate_plugin_name_hash(str.as_str()) {
            PluginNameHash { id: val, valid: true }    
        } else {
            PluginNameHash { id: 0, valid: false }
        }

    } else {
        PluginNameHash { id: 0, valid: false }
    }
}
