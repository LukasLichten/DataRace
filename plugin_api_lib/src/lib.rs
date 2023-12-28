use libc::c_char;
use log::{info, error, debug};
use tokio::runtime::Builder;

mod built_info {
    include!(concat!(env!("OUT_DIR"), "/built.rs"));
}

#[allow(unused_variables, dead_code)]
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
    
    // Stops the Runtime from closing when plugins are still running
    while let Some(res) = plugin_set.join_next().await {
        match res {
            Ok(_) => debug!("Some plugin finished"),
            Err(e) => error!("Plugin Runner Task (and it's contained Plugin) Crashed: {}", e)
        }
    }

    Ok(())
}

pub struct PluginHandle {
    name: String,
    // rec: kanal::Receiver<u8>
}

/// Return codes from operations like create_property, etc.
#[repr(C)]
pub enum DataStoreReturnCode {
    Ok = 0,
    NotAuthenticated = 1,
    AlreadyExists = 2,
    DoesNotExist = 3,
    OutdatedPropertyHandle = 4,
    TypeMissmatch = 5,

}

/// A Handle that serves for easy access to getting and updating properties
/// These handles can be from time to time invalidated if a property seizes to exist
///
pub struct PropertyHandle {
    index: usize,
    hash: u64
}

macro_rules! get_handle {
    ($ptr:ident) => {
        unsafe {
            $ptr.as_ref()
        }
    };
}

/// Logs a null terminated String as a Info
/// String is not deallocated, that is your job
#[no_mangle]
pub extern "C" fn log_info(handle: *mut PluginHandle, message: *mut c_char) {
    log_plugin_msg(handle, message, log::Level::Info);
}

/// Logs a null terminated String as a Error
/// String is not deallocated, that is your job
#[no_mangle]
pub extern "C" fn log_error(handle: *mut PluginHandle, message: *mut c_char) {
    log_plugin_msg(handle, message, log::Level::Error);
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

    // Even with file and or module set, it will continue not logging the name we want
    // So this is the best bandage fix over this mess
    log::logger().log(&log::Record::builder()
        .level(log_level)
        .args(format_args!("[{}] {msg}", han.name))
        .build());
}
