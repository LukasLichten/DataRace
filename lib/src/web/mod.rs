use std::{net::SocketAddr, path::PathBuf, sync::{atomic::AtomicBool, Arc}};

use axum::{extract::{Request, State}, http::StatusCode, middleware::Next, response::{IntoResponse, Response}, routing::get};
use axum_client_ip::{ClientIp, ClientIpSource};
use datarace_socket_spec::dashboard::Dashboard;
use log::{debug, info, warn};
use tokio::{fs, net::TcpListener};
use tower::ServiceBuilder;

use utils::{DataStoreLocked, IpMatchPerformer};

pub(crate) use utils::{SocketChMsg, WebSocketChReceiver, create_websocket_channel, IpMatcher};

mod utils;
mod socket;
mod pages;
mod dashboard;

pub(crate) const DEFAULT_IP: &str = "0.0.0.0";
pub(crate) const DEFAULT_PORT: u16 = 3939;

pub(crate) async fn run_webserver(datastore: DataStoreLocked, websocket_ch_recv: WebSocketChReceiver, shutdown: Arc<AtomicBool>) -> Result<(), Box<dyn std::error::Error>> {
    let ds_r = datastore.read().await;
    let config = ds_r.get_config().clone();
    drop(ds_r);

    if config.disable_web_server {
        info!("Webserver Disabled");
        return Ok(());
    }
    let addr = format!("{}:{}", config.web_server_ip, config.web_server_port);

    // This may need to be configurable when put behind a reverse proxy, but for most usecases this is fine
    let ip_source = ClientIpSource::ConnectInfo;
    let ip_matcher: Option<IpMatcher> = config.web_ip_whitelist;

    debug!("Setting up webserver ({})...", addr.as_str());

    let socket_layer = socket::create_socketio_layer(datastore, websocket_ch_recv).await;

    let app = axum::Router::new()
        .route("/", get(pages::index))
        .route("/dashboard", get(pages::dashboard_list))
        .route("/dashboard/render/{id}", get(pages::load_dashboard))
        .route("/dashboard/edit/{id}", get(pages::edit_dashboard))
        .route("/properties", get(pages::properties))
        .route("/setting", get(pages::settings))
        .route("/style.css", get(css_main_style))
        .route("/lib/socket.io.js", get(js_lib_socket_io))
        .route("/lib/datarace.dash.js", get(js_lib_datarace_dashboard))
        .with_state(datastore)
        .layer(ServiceBuilder::new()
            .layer(ip_source.into_extension())
            .layer(axum::middleware::from_fn_with_state(ip_matcher, ip_filtering_middleware))
            .layer(socket_layer)
        );
    let listener = TcpListener::bind(addr.as_str()).await?;

    info!("Webserver Launched on {}", addr);
    axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>())
        .with_graceful_shutdown(async move { 
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(1));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            while !shutdown.load(std::sync::atomic::Ordering::Acquire) { 
                interval.tick().await;
            }  
        })
        .await?;
    info!("Webserver stopped!");
    Ok(())
}

#[allow(dead_code)]
async fn serve_page(asset: &str) -> maud::Markup {
    maud::html! {
        (pages::serve_asset(asset).await)
    }
}

async fn ip_filtering_middleware(State(ip_matcher): State<Option<IpMatcher>>, ClientIp(ip): ClientIp,  request: Request, next: Next) -> Response {
    if ip_matcher.perform(&ip) {
        next.run(request).await
    } else {
        warn!("Web server blocked request of client '{ip}': Not on the Whitelist");
        FsResourceError::AccessDenied.into_response(request.uri().to_string())
    }
}

/// Retrieves the folder containing the dashboards
async fn get_dashboard_folder(datastore: DataStoreLocked) -> Result<PathBuf, FsResourceError> {
    let ds_r = datastore.read().await;
    let folder = ds_r.get_config().dashboards_location.clone();
    drop(ds_r);

    // Due to the config read this folder should exist, if it doesn't the next operation on it will
    // fail telling on that

    Ok(folder)
}

// Returns a certain dashboard by name
async fn get_dashboard(datastore: DataStoreLocked, path: String) -> Result<Dashboard, FsResourceError> {
    let mut folder = get_dashboard_folder(datastore).await?;

    folder.push(path.as_str());
    folder.set_extension("json");

    read_dashboard_from_path(folder).await
}

async fn read_dashboard_from_path(folder: PathBuf) -> Result<Dashboard, FsResourceError> {
    if !folder.exists() {
        return Err(FsResourceError::DoesNotExist);
    }

    let content = match fs::read(folder.as_path()).await {
        Ok(cont) => cont,
        Err(e) => {
            return Err(FsResourceError::from(e));
        }
    };

    serde_json::from_slice(content.as_slice()).map_err(|e| {
        FsResourceError::from(e)
    })
}

pub(crate) enum FsResourceError {
    AccessDenied,
    DoesNotExist,
    #[allow(dead_code)]
    Custom(String),
    FSError(std::io::Error),
    SerdeParseError(serde_json::Error)
}

impl From<std::io::Error> for FsResourceError {
    fn from(value: std::io::Error) -> Self {
        Self::FSError(value)
    }
}

impl From<serde_json::Error> for FsResourceError {
    fn from(value: serde_json::Error) -> Self {
        Self::SerdeParseError(value)
    }
}

impl FsResourceError {
    fn into_response(self, resource_name: String) -> Response {
        let mut res = maud::html! {
            (maud::DOCTYPE)
            meta charset="utf-8";
            title { "Error - DataRace" }
            (self.format(Some(resource_name)))
        }.into_response();

        *res.status_mut() = match self {
            Self::DoesNotExist => StatusCode::NOT_FOUND,
            Self::Custom(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::FSError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::SerdeParseError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            Self::AccessDenied => StatusCode::FORBIDDEN,
        };

        res
    }

    fn format(&self, resource_name: Option<String>) -> String {
        format!("Unable to load Resource{}: {}",
            match resource_name {
                Some(text) => format!(" {}", text),
                None => String::new()
            },
            match self {
                Self::AccessDenied => "You are not permitted to access this resource".to_string(),
                Self::DoesNotExist => "Does Not Exist".to_string(),
                Self::Custom(text) => text.clone(),
                Self::FSError(e) => format!("Failed to open file: {}", e.to_string()),
                Self::SerdeParseError(e) => format!("Unable to parse: {}", e.to_string())
            }
        )
    }
}

/// File is placed in assets/js_lib/socket.io.min.js
/// It is aquired via https://cdn.socket.io/4.8.1/socket.io.min.js
///
/// We include this in the binary and serve it from our server for offline compat
/// and knowing this version works with our socketioxide version
async fn js_lib_socket_io() -> Response {
    let b = axum::body::Body::try_from(include_str!("../../assets/js_lib/socket.io.min.js"))
                .expect("Failed to generate BODY responds containing the socket.io js lib. Please recompile");
    
    Response::builder()
        .status(200)
        .header(axum::http::header::CONTENT_TYPE, "application/javascript; charset=utf-8")
        .body(b)
        .expect("Failed to generate responde containing the socket.io js lib. Please recompile")
}

/// Sends the DataRace dashboard library, which handles values parsing
async fn js_lib_datarace_dashboard() -> Response {
    // let b = axum::body::Body::try_from(include_str!("../../assets/js_lib/datarace.dash.js"))
    //             .expect("Failed to generate BODY responds containing the datarace.dash js lib. Please recompile");
    
    let b = {
        let res = serve_page("js_lib/datarace.dash.js").await.into_response();
        res.into_body()
    };

    Response::builder()
        .status(200)
        .header(axum::http::header::CONTENT_TYPE, "application/javascript; charset=utf-8")
        .body(b)
        .expect("Failed to generate responde containing the datarace.dash js lib. Please recompile")
}

// File is placed in assets/style.css
//
// For debugging this should be dynmaically loaded (code provided)
async fn css_main_style() -> Response {
    let b = axum::body::Body::try_from(include_str!("../../assets/style.css"))
                .expect("Failed to generate BODY responds containing the style css. Please recompile");

    // let b = {
    //     let res = serve_page("style.css").await.into_response();
    //     res.into_body()
    // };
    
    Response::builder()
        .status(200)
        .header(axum::http::header::CONTENT_TYPE, "text/css")
        .body(b)
        .expect("Failed to generate responde containing the style css. Please recompile")
}
