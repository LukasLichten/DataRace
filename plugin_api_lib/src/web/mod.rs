use std::sync::{atomic::AtomicBool, Arc};

use axum::{response::IntoResponse, routing::get};
use log::{debug, info};
use tokio::net::TcpListener;

use utils::DataStoreLocked;

mod utils;
mod socket;
mod pages;

pub(crate) async fn run_webserver(datastore: DataStoreLocked, shutdown: Arc<AtomicBool>) -> Result<(), Box<dyn std::error::Error>> {
    debug!("Setting up webserver...");
    let layer = socket::create_socketio_layer(datastore).await;

    let app = axum::Router::new()
        .route("/", get(pages::index))
        .route("/dashboard", get(pages::dashboard_list))
        .route("/dashboard/render/:id", get(pages::load_dashboard))
        .route("/dashboard/edit/:id", get(pages::edit_dashboard))
        .route("/properties", get(pages::properties))
        .route("/setting", get(pages::settings))
        .route("/style.css", get(serve_css))
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

async fn serve_css() -> axum::response::Response {
    let mut res = serve_page("style.css").await.into_response();
    let header = res.headers_mut();
    header.insert(axum::http::header::CONTENT_TYPE, "text/css".parse().expect("string is string"));

    res
}

async fn serve_page(asset: &str) -> maud::Markup {
    maud::html! {
        (pages::serve_asset(asset).await)
    }
}
