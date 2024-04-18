use std::{collections::HashMap, sync::Arc};

use kanal::{AsyncReceiver, AsyncSender};
use socketioxide::socket::Sid;
use tokio::sync::RwLock;
use crate::{datastore::DataStore, pluginloader::LoaderMessage};

pub(super) type DataStoreLocked = &'static RwLock<DataStore>;
pub(super) type SocketDataRef = &'static SocketData;

#[derive(Debug, Clone)]
pub(super) enum Auth {
    Consumer,
    Plugin(u64, Arc<String>)
}

pub(super) struct SocketData {
    pub datastore: DataStoreLocked,
    access_table: RwLock<HashMap<Sid, Auth>>,
    pub sender: AsyncSender<LoaderMessage>
}

impl SocketData {
    pub(super) fn new(datastore: DataStoreLocked) -> (SocketDataRef, AsyncReceiver<LoaderMessage>) {
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

    pub(super) async fn remove_auth(&self, id: &Sid) -> bool {
        let mut w_table = self.access_table.write().await;
        let del = w_table.remove(id).is_some();
        drop(w_table);

        del
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Value {
    None,
    Int(i64),
    Float(u64),
    Bool(bool),
    Str(Arc<String>),
    Dur(i64)
}
