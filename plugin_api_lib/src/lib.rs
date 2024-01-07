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
        rt.shutdown_timeout(std::time::Duration::from_secs(2));
        info!("Done");
    } else {
        error!("Unable to launch tokio async runtime, aborting launch")
    }

}

async fn internal_main() -> Result<(), Box<dyn std::error::Error> > {
    info!("Launching DataRace...");
    let datastore: &'static datastore::DataStore  = Box::leak(Box::new(datastore::DataStore::new()));

    let mut plugin_set = pluginloader::load_all_plugins(datastore).await?;
    
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


