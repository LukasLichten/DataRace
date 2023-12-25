use log::info;

mod built_info {
    include!(concat!(env!("OUT_DIR"), "/built.rs"));
}

mod datastore;
mod pluginloader;

/// Used by the main executable to start the programm
/// Do NOT call this as a plugin
#[no_mangle]
pub extern "C" fn run() {
    let _ = internal_main();
    info!("Shutting down...");
}

fn internal_main() -> Result<(),()> {
    let log_level = log::LevelFilter::Info;
    env_logger::builder().filter_level(log_level).init();

    info!("Launing DataRace...");
    pluginloader::load_all_plugins()?;

    Ok(())
}
