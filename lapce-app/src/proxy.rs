use std::sync::{Arc, mpsc::Sender};

use crate::{terminal::event::TermEvent, workspace::LapceWorkspace};
use floem::{ext_event::create_signal_from_channel, reactive::ReadSignal};
use lapce_proxy::dispatch::Dispatcher;
use lapce_rpc::{
    core::{CoreHandler, CoreNotification, CoreRpcHandler},
    proxy::ProxyRpcHandler,
    terminal::TermId,
};

pub struct Proxy {
    pub tx: Sender<CoreNotification>,
    pub term_tx: Sender<(TermId, TermEvent)>,
}

#[derive(Clone)]
pub struct ProxyData {
    pub proxy_rpc: ProxyRpcHandler,
    pub core_rpc: CoreRpcHandler,
    pub notification: ReadSignal<Option<CoreNotification>>,
}

impl ProxyData {
    pub fn shutdown(&self) {
        self.proxy_rpc.shutdown();
        self.core_rpc.shutdown();
    }
}

pub fn new_proxy(
    workspace: Arc<LapceWorkspace>,
    term_tx: Sender<(TermId, TermEvent)>,
) -> ProxyData {
    let proxy_rpc = ProxyRpcHandler::new();
    let core_rpc = CoreRpcHandler::new();

    {
        let core_rpc = core_rpc.clone();
        let proxy_rpc = proxy_rpc.clone();
        std::thread::Builder::new()
            .name("ProxyRpcHandler".to_owned())
            .spawn(move || {
                proxy_rpc.initialize(workspace.path.clone());

                let core_rpc = core_rpc.clone();
                let proxy_rpc = proxy_rpc.clone();
                let mut dispatcher = Dispatcher::new(core_rpc, proxy_rpc);
                let proxy_rpc = dispatcher.proxy_rpc.clone();
                proxy_rpc.mainloop(&mut dispatcher);
            })
            .unwrap();
    }

    let (tx, rx) = std::sync::mpsc::channel();
    {
        let core_rpc = core_rpc.clone();
        std::thread::Builder::new()
            .name("CoreRpcHandler".to_owned())
            .spawn(move || {
                let mut proxy = Proxy { tx, term_tx };
                core_rpc.mainloop(&mut proxy);
            })
            .unwrap()
    };

    let notification = create_signal_from_channel(rx);

    ProxyData {
        proxy_rpc,
        core_rpc,
        notification,
    }
}

impl CoreHandler for Proxy {
    fn handle_notification(&mut self, rpc: lapce_rpc::core::CoreNotification) {
        if let CoreNotification::UpdateTerminal { term_id, content } = &rpc {
            if let Err(err) = self
                .term_tx
                .send((*term_id, TermEvent::UpdateContent(content.to_vec())))
            {
                tracing::error!("{:?}", err);
            }
            return;
        }
        if let Err(err) = self.tx.send(rpc) {
            tracing::error!("{:?}", err);
        }
    }
}
