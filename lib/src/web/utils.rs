use std::{collections::HashMap, sync::Arc};

use kanal::{AsyncReceiver, AsyncSender};
use socketioxide::socket::Sid;
use tokio::sync::RwLock;
use crate::{datastore::DataStore, ActionHandle, PropertyHandle};
use datarace_socket_spec::socket::{Action, PropertyHandle as WebPropertyHandle, ActionHandle as WebActionHandle};

pub(super) type DataStoreLocked = &'static RwLock<DataStore>;
pub(super) type SocketDataRef = &'static SocketData;
pub(crate) type WebSocketChReceiver = AsyncReceiver<SocketChMsg>;

#[derive(Debug, Clone)]
pub(super) enum Auth {
    Dashboard(String),
    #[allow(dead_code)]
    Plugin(u64, Arc<String>)
} 

pub(crate) fn create_websocket_channel() -> (AsyncSender<SocketChMsg>, WebSocketChReceiver) {
    kanal::unbounded_async()
}

pub(super) struct SocketData {
    pub datastore: DataStoreLocked,
    access_table: RwLock<HashMap<Sid, Auth>>,
    pub sender: AsyncSender<SocketChMsg>
}

impl SocketData {
    pub(super) async fn new(datastore: DataStoreLocked) -> SocketDataRef {
        let ds_r = datastore.read().await;
        let sx = ds_r.get_websocket_channel().clone();
        drop(ds_r);

        Box::leak(
            Box::new(
                SocketData {
                    datastore,
                    access_table: RwLock::new(HashMap::new()),
                    sender: sx
                }
            )
        )
    }

    pub(super) async fn insert_auth(&self, id: Sid, auth: Auth) {
        let mut w_table = self.access_table.write().await;
        w_table.insert(id, auth);
        drop(w_table);
    }

    pub(super) async fn insert_dashboard(&self, id: Sid, name: String) {
        self.insert_auth(id, Auth::Dashboard(name.clone())).await;

        let _ = self.sender.send(SocketChMsg::AddDashboard(name)).await;
    }

    pub(super) async fn get_auth(&self, id: &Sid) -> Option<Auth> {
        let r_table = self.access_table.read().await;
        if let Some(value) = r_table.get(id) {
            let val = value.clone();
            drop(r_table);

            Some(val)
        } else {
            drop(r_table);
            None
        }
    }

    pub(super) async fn remove_auth(&self, id: &Sid) {
        let mut w_table = self.access_table.write().await;
        if let Some(res) = w_table.remove(id) {
            drop(w_table);
            match res {
                Auth::Dashboard(name) => { 
                    let _ = self.sender.send(SocketChMsg::RmDashboard(name)).await;
                },
                Auth::Plugin(_, _) => todo!("Plugin removal not yet implemented")
            }
        }
    }

    pub(super) async fn trigger_action(&self, id: &Sid, action: Action) -> Result<u64, String> {
        let origin = match self.get_auth(id).await.ok_or(format!("Client {id} has not done auth yet"))? {
            Auth::Dashboard(_) => 0,
            Auth::Plugin(id, _) => id
        };

        // TODO some preventitive measure to stop anyone authing as a Dashboard and triggering
        // random actions with random junk data

        let ds_r = self.datastore.read().await;
        let res = ds_r.trigger_web_action(origin, action).await;
        drop(ds_r);

        res.map_err(|e| e.to_string())
    }
}

/// Serves as the Messaging Protocol of the Socket.io Server Channel
pub(crate) enum SocketChMsg {
    AddDashboard(String),
    RmDashboard(String),
    ChangedProperty(crate::PropertyHandle, crate::utils::ValueContainer)
}

impl From<PropertyHandle> for WebPropertyHandle {
    fn from(value: PropertyHandle) -> Self {
        WebPropertyHandle::new(value.plugin, value.property)
    }
}

impl From<ActionHandle> for WebActionHandle {
    fn from(value: ActionHandle) -> Self {
        WebActionHandle::new(value.plugin, value.action)
    }
}

impl TryFrom<WebActionHandle> for ActionHandle {
    type Error = &'static str;

    fn try_from(value: WebActionHandle) -> Result<Self, Self::Error> {
        let (plugin, action) = value.get_hashes().ok_or("Malformed ActionHandle")?;
        Ok(ActionHandle { plugin, action })
    }
}
