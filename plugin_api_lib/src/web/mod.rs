use std::{str::FromStr, sync::{atomic::AtomicBool, Arc}};

use axum::{response::Html, routing::get};
use log::{debug, info};
use tokio::net::TcpListener;

use utils::DataStoreLocked;

mod utils;
mod socket;

pub(crate) async fn run_webserver(datastore: DataStoreLocked, shutdown: Arc<AtomicBool>) -> Result<(), Box<dyn std::error::Error>> {
    debug!("Setting up webserver...");
    let layer = socket::create_socketio_layer(datastore).await;

    let app = axum::Router::new()
        .route("/", get(|| async { serve_page("index.html").await }))
        .route("/style.css", get(|| async { serve_page("style.css").await }))
        .with_state(datastore)
        .layer(layer);
    let listener = TcpListener::bind("0.0.0.0:3000").await?;

    info!("Webserver Launched");
    axum::serve(listener, app)
        .with_graceful_shutdown(async move { while !shutdown.load(std::sync::atomic::Ordering::Acquire) { std::thread::sleep(std::time::Duration::from_secs(1)) }  })
        .await?;
    info!("Webserver stopped!");
    Ok(())
}

async fn serve_page(asset: &str) -> Html<String> {
    let mut path = std::path::PathBuf::from_str("./plugin_api_lib/assets").unwrap();
    path.push(asset);

    let val = std::fs::read_to_string(path.as_path()).unwrap();

    Html(val)
}
