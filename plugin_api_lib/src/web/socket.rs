
use std::sync::Arc;

use kanal::AsyncReceiver;
use log::{debug, error};
use serde::{Deserialize, Serialize};
use socketioxide::{extract::{Data, SocketRef, State}, SocketIo};
use crate::pluginloader::Message;

use super::utils::{DataStoreLocked,SocketDataRef,Auth};

pub(super) async fn create_socketio_layer(datastore: DataStoreLocked) -> socketioxide::layer::SocketIoLayer {
    let (store,rx) = super::utils::SocketData::new(datastore);

    let (layer, io) = SocketIo::builder()
        .with_state(store)
        .build_layer();

    io.ns("/", on_connect);

    tokio::task::spawn(test(io, store, rx));

    layer
}

async fn on_connect(socket: SocketRef) {
    debug!("Someone is trying to connect, {}", socket.id);
    

    socket.on("message", |socket: SocketRef, Data(data): Data<serde_json::Value>, State(store): State<SocketDataRef>| async move {
        let name = match store.get_auth(&socket.id).await {
            Some(Auth::Consumer) => "Consumer".to_string(),
            Some(Auth::Plugin(_, name)) => format!("Plugin {}", name),
            None => "Un-Authericed".to_string()
        };
        socket.emit("message-back", format!("Hello, World! {}", name)).ok();
        
    });

    // For some reason I can't serialize the Plugin version through Serializer,
    // the function just isn't called
    socket.on("auth", |socket: SocketRef, Data(data): Data<Authentication>, State(store): State<SocketDataRef>| async move {
        debug!("{} socket trying to auth", socket.id);

        if store.get_auth(&socket.id).await.is_some() {
            // This is an error, you should not be able to auth twice
            error!("Already Authericed");
            return;
        }
        
        if let Authentication::Plugin { name } = data {
            // this is a plugin, we have to internally register it
            let mut w_ds = store.datastore.write().await;

            // if let Some(token) = w_ds.register_plugin(name.clone(), 0, store.sender.as_sync().clone()) {
            //     store.insert_auth(socket.id, Auth::Plugin(token, Arc::new(name))).await;
            // } else {
            //     // Failed, likely name collision
            //     error!("Failed to create Plugin");
            //     return;
            // }
        } else {
            // Consumer only
            store.insert_auth(socket.id, Auth::Consumer).await;
        }
    });


    socket.on("get_property", |socket: SocketRef, Data(_data): Data<serde_json::Value>, State(store): State<SocketDataRef>| async move {
        // let r_store = store.datastore.read().await;
        // let han = r_store.get_property_handle("sample_plugin.Test".to_string()).unwrap();
        // let val = r_store.get_property(&han).await.unwrap();
        //
        // drop(r_store);
        //
        // socket.emit("get_property-return", if let crate::utils::Value::Int(i) = val { i.to_string() } else { "null".to_string() } ).ok();
        
    });

    socket.on("exit", |socket: SocketRef, Data(_data): Data<serde_json::Value>, State(_store): State<SocketDataRef>| {
        socket.disconnect().ok();
    }); 


    socket.on_disconnect(|_socket: SocketRef, State(_store): State<SocketDataRef>| {
        debug!("Left *big sad*");
    });
    
}

async fn test(io: SocketIo, _datastore: SocketDataRef, _rx: AsyncReceiver<Message>) {
    loop {
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        // io.emit("test", format!("FreeBird!")).ok();
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
enum Authentication {
    Consumer,
    Plugin{name: String} // this should serialize to {"Plugin": {"name": "test"}}
}
