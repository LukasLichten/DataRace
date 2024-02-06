use std::sync::{atomic::AtomicBool, Arc};

use log::{debug, info};
use socketioxide::{extract::{Data, SocketRef}, SocketIo};
use tokio::{net::TcpListener, sync::RwLock};

use crate::datastore::DataStore;



pub(crate) async fn run_webserver(datastore: &'static RwLock<DataStore>, shutdown: Arc<AtomicBool>) -> Result<(), Box<dyn std::error::Error>> {
    let (layer, io) = SocketIo::new_layer();

    io.ns("/", on_connect);

    let app = axum::Router::new()
        .with_state(datastore)
        .route("/", axum::routing::get(|| async { "Hello, World!" }))
        .layer(layer);
    let listener = TcpListener::bind("0.0.0.0:3000").await?;

    info!("Webserver Launched");
    axum::serve(listener, app)
        .with_graceful_shutdown(async move { while !shutdown.load(std::sync::atomic::Ordering::Acquire) { std::thread::sleep(std::time::Duration::from_secs(1)) }  })
        .await?;
    info!("Webserver stopped!");
    Ok(())
}

async fn on_connect(socket: SocketRef, Data(data): Data<serde_json::Value>) {
    debug!("Someone is trying to connect: {}", data.to_string());

    socket.on("message", |socket: SocketRef| {
        socket.emit("message-back", "Hello, World!").ok();
    });
}
