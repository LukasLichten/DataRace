use std::{path::PathBuf, sync::{atomic::AtomicBool, Arc}};

use axum::{http::StatusCode, response::{IntoResponse, Response}, routing::get};
use log::{debug, error, info};
use tokio::{fs, net::TcpListener};

use utils::DataStoreLocked;

mod utils;
mod socket;
mod pages;
mod dashboard;

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
        .route("/lib/socket.io.js", get(js_lib_socket_io))
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

async fn serve_css() -> Response {
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

/// Retrieves the folder containing the dashboards
async fn get_dashboard_folder(datastore: DataStoreLocked) -> Result<PathBuf, StatusCode> {
    let ds_r = datastore.read().await;
    let folder = ds_r.get_config().get_dashboards_folder();
    // We keep lock over datarace to prevent a race condition with folder creation

    if !folder.exists() {
        // Creating folder
        info!("Dashboards folder did not exist, creating...");
        if let Err(e) = std::fs::create_dir_all(folder.as_path()) {
            error!("Failed to create Dashboards Folder: {}", e.to_string());
        }
    }

    drop(ds_r);

    if folder.is_file() {
        // We are screwed
        error!("Unable to open dashboards folder because it is a file!");
        return Err(StatusCode::INTERNAL_SERVER_ERROR);
    }

    Ok(folder)
}

// Returns a certain dashboard by name
async fn get_dashboard(datastore: DataStoreLocked, path: String) -> Result<dashboard::Dashboard, StatusCode> {
    let mut folder = get_dashboard_folder(datastore).await?;

    folder.push(path.as_str());
    folder.set_extension("json");

    if !folder.exists() {
        return Err(StatusCode::NOT_FOUND);
    }

    let content = match fs::read(folder.as_path()).await {
        Ok(cont) => cont,
        Err(e) => {
            error!("Unable to open Dashboard {} file: {}", path, e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    serde_json::from_slice(content.as_slice()).map_err(|e| {
        error!("Unable to parse Dashboard {} json file: {}", path, e);
        StatusCode::IM_A_TEAPOT
    })
}

// File is placed in assets/js_lib/socket.io.min.js
// It is aquired via https://cdn.socket.io/4.7.5/socket.io.min.js
//
// We include this in the binary and serve it from our server for offline compat
// and knowing this version works with our socketioxide version
async fn js_lib_socket_io() -> Response {
    let b = axum::body::Body::try_from(include_str!("../../assets/js_lib/socket.io.min.js"))
                .expect("Failed to generate BODY responde containing the socket.io js lib. Please recompile");
    
    Response::builder()
        .status(200)
        .header("content-type", "application/javascript; charset=utf-8")
        .body(b)
        .expect("Failed to generate responde containing the socket.io js lib. Please recompile")
}
