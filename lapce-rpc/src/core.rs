use std::path::PathBuf;

use crossbeam_channel::{Receiver, Sender};
use lsp_types::ShowMessageParams;
use serde::{Deserialize, Serialize};

use crate::{
    RpcMessage, file::PathObject, source_control::DiffInfo, terminal::TermId,
};

pub enum CoreRpc {
    Notification(Box<CoreNotification>), // Box it since clippy complains
    Shutdown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FileChanged {
    Change(String),
    Delete,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[serde(tag = "method", content = "params")]
pub enum CoreNotification {
    OpenFileChanged {
        path: PathBuf,
        content: FileChanged,
    },
    OpenPaths {
        paths: Vec<PathObject>,
    },
    WorkspaceFileChange,
    ShowMessage {
        title: String,
        message: ShowMessageParams,
    },
    DiffInfo {
        diff: DiffInfo,
    },
    UpdateTerminal {
        term_id: TermId,
        content: Vec<u8>,
    },
    TerminalLaunchFailed {
        term_id: TermId,
        error: String,
    },
    TerminalProcessStopped {
        term_id: TermId,
        exit_code: Option<i32>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CoreRequest {}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[serde(tag = "method", content = "params")]
pub enum CoreResponse {}

pub type CoreMessage = RpcMessage<CoreRequest, CoreNotification, CoreResponse>;

pub trait CoreHandler {
    fn handle_notification(&mut self, rpc: CoreNotification);
}

#[derive(Clone)]
pub struct CoreRpcHandler {
    tx: Sender<CoreRpc>,
    rx: Receiver<CoreRpc>,
}

impl CoreRpcHandler {
    pub fn new() -> Self {
        let (tx, rx) = crossbeam_channel::unbounded();
        Self { tx, rx }
    }

    pub fn mainloop<H>(&self, handler: &mut H)
    where
        H: CoreHandler,
    {
        for msg in &self.rx {
            match msg {
                CoreRpc::Notification(rpc) => {
                    handler.handle_notification(*rpc);
                }
                CoreRpc::Shutdown => {
                    return;
                }
            }
        }
    }

    pub fn shutdown(&self) {
        if let Err(err) = self.tx.send(CoreRpc::Shutdown) {
            tracing::error!("{:?}", err);
        }
    }

    pub fn notification(&self, notification: CoreNotification) {
        if let Err(err) = self.tx.send(CoreRpc::Notification(Box::new(notification)))
        {
            tracing::error!("{:?}", err);
        }
    }

    pub fn workspace_file_change(&self) {
        self.notification(CoreNotification::WorkspaceFileChange);
    }

    pub fn diff_info(&self, diff: DiffInfo) {
        self.notification(CoreNotification::DiffInfo { diff });
    }

    pub fn open_file_changed(&self, path: PathBuf, content: FileChanged) {
        self.notification(CoreNotification::OpenFileChanged { path, content });
    }

    pub fn show_message(&self, title: String, message: ShowMessageParams) {
        self.notification(CoreNotification::ShowMessage { title, message });
    }

    pub fn terminal_process_stopped(&self, term_id: TermId, exit_code: Option<i32>) {
        self.notification(CoreNotification::TerminalProcessStopped {
            term_id,
            exit_code,
        });
    }

    pub fn terminal_launch_failed(&self, term_id: TermId, error: String) {
        self.notification(CoreNotification::TerminalLaunchFailed { term_id, error });
    }

    pub fn update_terminal(&self, term_id: TermId, content: Vec<u8>) {
        self.notification(CoreNotification::UpdateTerminal { term_id, content });
    }
}

impl Default for CoreRpcHandler {
    fn default() -> Self {
        Self::new()
    }
}
