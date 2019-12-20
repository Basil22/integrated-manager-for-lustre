// Copyright (c) 2019 DDN. All rights reserved.
// Use of this source code is governed by a MIT-style
// license that can be found in the LICENSE file.

use crate::{
    agent_error::{ImlAgentError, RequiredError, Result},
    daemon_plugins::{DaemonPlugin, Output},
    http_comms::mailbox_client::send,
};
use futures::{
    channel::oneshot,
    future::{self, Either},
    Future, FutureExt,
};
use futures_util::stream::StreamExt as ForEachStreamExt;
use iml_wire_types::{Action, ActionId, ActionName, ActionResult, AgentResult, ToJsonValue};
use parking_lot::Mutex;
use std::{collections::HashMap, path::Path, pin::Pin, sync::Arc};
use stream_cancel::{StreamExt, Trigger, Tripwire};
use tokio::{fs, net::UnixListener};
use tokio_util::codec::{BytesCodec, FramedRead};

const CONF_FILE: &str = "/etc/iml/postman.conf";
const SOCK_DIR: &str = "/run/iml/";

pub struct PostOffice {
    // ids of actions (register/deregister)
    ids: Arc<Mutex<HashMap<ActionId, oneshot::Sender<()>>>>,
    // individual mailbox socket listeners
    routes: Arc<Mutex<HashMap<String, Trigger>>>,
}

/// Return socket address for a given mailbox
pub fn socket_name<P: AsRef<Path>>(mailbox: P) -> P {
    format!("{}/postman-{}.sock", SOCK_DIR, mailbox)
        .parse()
        .unwrap()
}

impl std::fmt::Debug for PostOffice {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(
            f,
            "PostOffice {{ ids: {:?}, registry: RegistryFn }}",
            self.ids
        )
    }
}

fn start_route(mailbox: String) -> Trigger {
    let (trigger, tripwire) = Tripwire::new();
    let addr = socket_name(mailbox);

    let rc = async move {
        let mut listener = UnixListener::bind(addr).unwrap();

        let incoming = listener.incoming().take_until(tripwire);
        while let Some(inbound) = incoming.next().await {
            if let Ok(inbound) = inbound {
                let stream = FramedRead::new(inbound, BytesCodec::new());
                let transfer = send(mailbox, stream).map(|r| {
                    if let Err(e) = r {
                        println!("Failed to transfer; error={}", e);
                    }
                });
                tokio::spawn(transfer);
            }
        }
    };
    tokio::spawn(rc);
    trigger
}

fn stop_route(trigger: Trigger) -> Result<(), ImlAgentError> {
    drop(trigger);

    Ok(())
}

pub fn create() -> impl DaemonPlugin {
    PostOffice {
        ids: Arc::new(Mutex::new(HashMap::new())),
        routes: Arc::new(Mutex::new(HashMap::new())),
    }
}

impl DaemonPlugin for PostOffice {
    fn start_session(&mut self) -> Pin<Box<dyn Future<Output = Result<Output>> + Send>> {
        let routes = Arc::clone(&self.routes);
        let fut = async move {
            let itr = fs::read_to_string(CONF_FILE).await.unwrap_or("".to_string()).lines().map(|mb| {
                let trigger = start_route(mb.to_string());
                (mb.to_string(), trigger)
            });
            routes.lock().extend(itr);
            Ok(None)
        };
        fut.boxed()
    }
    fn on_message(
        &self,
        v: serde_json::Value,
    ) -> Pin<Box<dyn Future<Output = Result<AgentResult>> + Send>> {
        let action: Action = match serde_json::from_value(v) {
            Ok(x) => x,
            Err(e) => return Box::pin(future::err(ImlAgentError::Serde(e))),
        };

        match action {
            Action::ActionStart { action, args, id } => {
                let mailbox: String = match serde_json::from_value(args) {
                    Ok(x) => x,
                    Err(e) => return Box::pin(future::err(ImlAgentError::Serde(e))),
                };
                let routes = Arc::clone(&self.routes);
                let fut = match action {
                    ActionName("register".to_string()) => {
                        async move {
                            // @@ add route to config
                            routes.lock().entry(mailbox).or_insert_with(|| {
                                start_route(mailbox)
                            });
                            Ok(())
                        }
                    }
                    ActionName("deregister".to_string()) => {
                        async move {
                            if let Some(tx) = routes.lock().remove(&mailbox) {
                                stop_route(tx)
                            }
                            Ok(())
                        }
                    }
                    _ => {
                        let err =
                            RequiredError(format!("Could not find action {} in registry", action));

                        let result = ActionResult {
                            id,
                            result: Err(format!("{:?}", err)),
                        };

                        return Box::pin(future::ok(result.to_json_value()));
                    }
                };

                // @@ - RUN above future
                let (tx, rx) = oneshot::channel();

                self.ids.lock().insert(id.clone(), tx);

                let ids = self.ids.clone();

                Box::pin(
                    future::select(fut, rx)
                        .map(move |r| match r {
                            Either::Left((result, _)) => {
                                ids.lock().remove(&id);
                                ActionResult { id, result }
                            }
                            Either::Right((_, z)) => {
                                drop(z);
                                ActionResult {
                                    id,
                                    result: ().to_json_value(),
                                }
                            }
                        })
                        .map(|r| Ok(r.to_json_value())),
                )
            }
            Action::ActionCancel { id } => {
                let tx = self.ids.lock().remove(&id);

                if let Some(tx) = tx {
                    // We don't care what the result is here.
                    let _ = tx.send(()).is_ok();
                }

                Box::pin(future::ok(
                    ActionResult {
                        id,
                        result: ().to_json_value(),
                    }
                    .to_json_value(),
                ))
            }
        }
    }
    fn teardown(&mut self) -> Result<()> {
        for (_, tx) in self.ids.lock().drain() {
            // We don't care what the result is here.
            let _ = tx.send(()).is_ok();
        }
        for (_, tx) in self.routes.lock().drain() {
            // We don't care what the result is here.
            let _ = tx.send(()).is_ok();
        }
        // @@ - stop threads

        Ok(())
    }
}
