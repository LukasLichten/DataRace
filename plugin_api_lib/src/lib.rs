use libc::c_char;
use log::{info, error};
use tokio::runtime::Builder;

mod built_info {
    include!(concat!(env!("OUT_DIR"), "/built.rs"));
}

mod datastore;
mod pluginloader;
pub(crate) mod utils;

/// Used by the main executable to start the programm
/// Do NOT call this as a plugin
#[no_mangle]
pub extern "C" fn run() {
    let log_level = log::LevelFilter::Debug;
    env_logger::builder().filter_level(log_level).init();

    if let Ok(rt) = Builder::new_multi_thread().enable_all().build() {
        let res = rt.block_on(internal_main());

        if let Err(e) = res {
            error!("DataRace crashed: {}", e);
        } else {
            info!("Shutting down...");
        }
    } else {
        error!("Unable to launch tokio async runtime, aborting launch")
    }
}

async fn internal_main() -> Result<(), Box<dyn std::error::Error> > {
    info!("Launing DataRace...");
    pluginloader::load_all_plugins().await?;

    Ok(())
}

pub struct PluginHandle {
    name: String,
    // rec: kanal::Receiver<u8>
}

macro_rules! get_handle {
    ($ptr:ident) => {
        unsafe {
            $ptr.as_ref()
        }
    };
}

/// Logs a null terminated String
/// String is not deallocated, that is your job
#[no_mangle]
pub extern "C" fn log_info(handle: *mut PluginHandle, message: *mut c_char) {
    let han = if let Some(handle) = get_handle!(handle) {
        handle
    } else {
        error!("Plugin Handle corrupted");
        return;
    };

    let msg = if let Some(message) = utils::get_string(message) {
        message
    } else {
        error!("Message was corrupted");
        return;
    };

    info!("{}: {}", han.name, msg);
}
