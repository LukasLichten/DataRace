use std::{collections::HashMap, sync::Arc};

use kanal::{AsyncReceiver, AsyncSender};
use socketioxide::socket::Sid;
use tokio::sync::RwLock;
use crate::datastore::DataStore;

pub(super) type DataStoreLocked = &'static RwLock<DataStore>;
pub(super) type SocketDataRef = &'static SocketData;

#[derive(Debug, Clone)]
pub(super) enum Auth {
    Dashboard(String),
    Plugin(u64, Arc<String>)
}

pub(super) struct SocketData {
    pub datastore: DataStoreLocked,
    access_table: RwLock<HashMap<Sid, Auth>>,
    pub sender: AsyncSender<SocketChMsg>
}

impl SocketData {
    pub(super) fn new(datastore: DataStoreLocked) -> (SocketDataRef, AsyncReceiver<SocketChMsg>) {
        let (sx, rx) = kanal::unbounded_async();
        (Box::leak(
            Box::new(
                SocketData {
                    datastore,
                    access_table: RwLock::new(HashMap::new()),
                    sender: sx
                }
            )
        ), rx)
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
}

/// Serves as the Messaging Protocol of the Socket.io Server Channel
pub(super) enum SocketChMsg {
    AddDashboard(String),
    RmDashboard(String)
}
