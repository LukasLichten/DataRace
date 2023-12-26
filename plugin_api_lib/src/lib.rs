use libc::c_char;
use log::{info, error, debug};
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
    let mut plugin_set = pluginloader::load_all_plugins().await?;
    
    // Temporary, insure runtime stays alive long enough to deliver message
    // std::thread::sleep(std::time::Duration::from_millis(500));

    while let Some(res) = plugin_set.join_next().await {
        match res {
            Ok(_) => debug!("Some plugin finished"),
            Err(e) => error!("One Plugin crashed {}", e)
        }
    }

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
    log_plugin_msg(handle, message, log::Level::Info);
}

fn log_plugin_msg(handle: *mut PluginHandle, message: *mut c_char, log_level: log::Level) {
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

    log::logger().log(&log::Record::builder()
        .level(log_level)
        .args(format_args!("[{}] {msg}", han.name))
        .build());
}
