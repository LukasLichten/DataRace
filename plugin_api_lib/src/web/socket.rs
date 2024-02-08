
use log::debug;
use socketioxide::{extract::{Data, SocketRef, State}, SocketIo};
use super::DataStoreLocked;

pub(super) async fn create_socketio_layer(datastore: DataStoreLocked) -> socketioxide::layer::SocketIoLayer {
    let (layer, io) = SocketIo::builder()
        .with_state(datastore)
        .build_layer();

    io.ns("/", on_connect);

    tokio::task::spawn(test(io, datastore));

    layer
}

async fn on_connect(socket: SocketRef) {
    debug!("Someone is trying to connect, {}", socket.id);
    

    socket.on("message", |socket: SocketRef, Data(data): Data<serde_json::Value>, State(_store): State<DataStoreLocked>| {
        socket.emit("message-back", format!("Hello, World! {}", data.to_string())).ok();
        
    });


    socket.on("get_property", |socket: SocketRef, Data(_data): Data<serde_json::Value>, State(store): State<DataStoreLocked>| async move {
        let r_store = store.read().await;
        let han = r_store.get_property_handle("sample_plugin.Test".to_string()).unwrap();
        let val = r_store.get_property(&han).await.unwrap();

        drop(r_store);

        socket.emit("get_property-return", if let crate::utils::Value::Int(i) = val { i.to_string() } else { "null".to_string() } ).ok();
        
    });

    socket.on("exit", |socket: SocketRef, Data(_data): Data<serde_json::Value>, State(_store): State<DataStoreLocked>| {
        socket.disconnect().ok();
    }); 


    socket.on_disconnect(|_socket: SocketRef, State(_store): State<DataStoreLocked>| {
        debug!("Left *big sad*");
    });
    
}

async fn test(io: SocketIo, _datastore: DataStoreLocked) {
    loop {
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        io.emit("test", format!("FreeBird!")).ok();
    }
}
