use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

use crossbeam_channel::{Receiver, Sender};
use indexmap::IndexMap;
use lapce_xi_rope::RopeDelta;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};

use crate::{
    RequestId, RpcError,
    buffer::BufferId,
    file::{FileNodeItem, PathObject},
    source_control::FileDiff,
    terminal::{TermId, TerminalProfile},
};

#[allow(clippy::large_enum_variant)]
pub enum ProxyRpc {
    Request(RequestId, ProxyRequest),
    Notification(ProxyNotification),
    Shutdown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchMatch {
    pub line: usize,
    pub start: usize,
    pub end: usize,
    pub line_content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[serde(tag = "method", content = "params")]
pub enum ProxyRequest {
    NewBuffer {
        buffer_id: BufferId,
        path: PathBuf,
    },
    BufferHead {
        path: PathBuf,
    },
    GlobalSearch {
        pattern: String,
        case_sensitive: bool,
        whole_word: bool,
        is_regex: bool,
    },
    GitGetRemoteFileUrl {
        file: PathBuf,
    },
    GetFiles {
        path: String,
    },
    ReadDir {
        path: PathBuf,
    },
    Save {
        rev: u64,
        path: PathBuf,
        /// Whether to create the parent directories if they do not exist.
        create_parents: bool,
    },
    SaveBufferAs {
        buffer_id: BufferId,
        path: PathBuf,
        rev: u64,
        content: String,
        /// Whether to create the parent directories if they do not exist.
        create_parents: bool,
    },
    CreateFile {
        path: PathBuf,
    },
    CreateDirectory {
        path: PathBuf,
    },
    TrashPath {
        path: PathBuf,
    },
    DuplicatePath {
        existing_path: PathBuf,
        new_path: PathBuf,
    },
    RenamePath {
        from: PathBuf,
        to: PathBuf,
    },
    TestCreateAtPath {
        path: PathBuf,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[serde(tag = "method", content = "params")]
pub enum ProxyNotification {
    Initialize {
        workspace: Option<PathBuf>,
    },
    OpenFileChanged {
        path: PathBuf,
    },
    OpenPaths {
        paths: Vec<PathObject>,
    },
    Shutdown {},
    Update {
        path: PathBuf,
        delta: RopeDelta,
        rev: u64,
    },
    NewTerminal {
        term_id: TermId,
        profile: TerminalProfile,
    },
    GitCommit {
        message: String,
        diffs: Vec<FileDiff>,
    },
    GitCheckout {
        reference: String,
    },
    GitDiscardFilesChanges {
        files: Vec<PathBuf>,
    },
    GitDiscardWorkspaceChanges {},
    GitInit {},
    TerminalWrite {
        term_id: TermId,
        content: String,
    },
    TerminalResize {
        term_id: TermId,
        width: usize,
        height: usize,
    },
    TerminalClose {
        term_id: TermId,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[serde(tag = "method", content = "params")]
pub enum ProxyResponse {
    GitGetRemoteFileUrl {
        file_url: String,
    },
    NewBufferResponse {
        content: String,
        read_only: bool,
    },
    BufferHeadResponse {
        version: String,
        content: String,
    },
    ReadDirResponse {
        items: Vec<FileNodeItem>,
    },
    GetFilesResponse {
        items: Vec<PathBuf>,
    },
    GlobalSearchResponse {
        matches: IndexMap<PathBuf, Vec<SearchMatch>>,
    },
    CreatePathResponse {
        path: PathBuf,
    },
    Success {},
    SaveResponse {},
}

pub trait ProxyCallback: Send + FnOnce(Result<ProxyResponse, RpcError>) {}

impl<F: Send + FnOnce(Result<ProxyResponse, RpcError>)> ProxyCallback for F {}

enum ResponseHandler {
    Callback(Box<dyn ProxyCallback>),
}

impl ResponseHandler {
    fn invoke(self, result: Result<ProxyResponse, RpcError>) {
        match self {
            ResponseHandler::Callback(f) => f(result),
        }
    }
}

pub trait ProxyHandler {
    fn handle_notification(&mut self, rpc: ProxyNotification);
    fn handle_request(&mut self, id: RequestId, rpc: ProxyRequest);
}

#[derive(Clone)]
pub struct ProxyRpcHandler {
    tx: Sender<ProxyRpc>,
    rx: Receiver<ProxyRpc>,
    id: Arc<AtomicU64>,
    pending: Arc<Mutex<HashMap<u64, ResponseHandler>>>,
}

impl ProxyRpcHandler {
    pub fn new() -> Self {
        let (tx, rx) = crossbeam_channel::unbounded();
        Self {
            tx,
            rx,
            id: Arc::new(AtomicU64::new(0)),
            pending: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn rx(&self) -> &Receiver<ProxyRpc> {
        &self.rx
    }

    pub fn mainloop<H>(&self, handler: &mut H)
    where
        H: ProxyHandler,
    {
        use ProxyRpc::*;
        for msg in &self.rx {
            match msg {
                Request(id, request) => {
                    handler.handle_request(id, request);
                }
                Notification(notification) => {
                    handler.handle_notification(notification);
                }
                Shutdown => {
                    return;
                }
            }
        }
    }

    fn request_common(&self, request: ProxyRequest, rh: ResponseHandler) {
        let id = self.id.fetch_add(1, Ordering::Relaxed);

        self.pending.lock().insert(id, rh);

        if let Err(err) = self.tx.send(ProxyRpc::Request(id, request)) {
            tracing::error!("{:?}", err);
        }
    }

    pub fn request_async(
        &self,
        request: ProxyRequest,
        f: impl ProxyCallback + 'static,
    ) {
        self.request_common(request, ResponseHandler::Callback(Box::new(f)))
    }

    pub fn handle_response(
        &self,
        id: RequestId,
        result: Result<ProxyResponse, RpcError>,
    ) {
        let handler = { self.pending.lock().remove(&id) };
        if let Some(handler) = handler {
            handler.invoke(result);
        }
    }

    pub fn notification(&self, notification: ProxyNotification) {
        if let Err(err) = self.tx.send(ProxyRpc::Notification(notification)) {
            tracing::error!("{:?}", err);
        }
    }

    pub fn git_init(&self) {
        self.notification(ProxyNotification::GitInit {});
    }

    pub fn git_commit(&self, message: String, diffs: Vec<FileDiff>) {
        self.notification(ProxyNotification::GitCommit { message, diffs });
    }

    pub fn git_checkout(&self, reference: String) {
        self.notification(ProxyNotification::GitCheckout { reference });
    }

    pub fn shutdown(&self) {
        self.notification(ProxyNotification::Shutdown {});
        if let Err(err) = self.tx.send(ProxyRpc::Shutdown) {
            tracing::error!("{:?}", err);
        }
    }

    pub fn initialize(&self, workspace: Option<PathBuf>) {
        self.notification(ProxyNotification::Initialize { workspace });
    }

    pub fn new_terminal(&self, term_id: TermId, profile: TerminalProfile) {
        self.notification(ProxyNotification::NewTerminal { term_id, profile })
    }

    pub fn terminal_close(&self, term_id: TermId) {
        self.notification(ProxyNotification::TerminalClose { term_id });
    }

    pub fn terminal_resize(&self, term_id: TermId, width: usize, height: usize) {
        self.notification(ProxyNotification::TerminalResize {
            term_id,
            width,
            height,
        });
    }

    pub fn terminal_write(&self, term_id: TermId, content: String) {
        self.notification(ProxyNotification::TerminalWrite { term_id, content });
    }

    pub fn new_buffer(
        &self,
        buffer_id: BufferId,
        path: PathBuf,
        f: impl ProxyCallback + 'static,
    ) {
        self.request_async(ProxyRequest::NewBuffer { buffer_id, path }, f);
    }

    pub fn get_buffer_head(&self, path: PathBuf, f: impl ProxyCallback + 'static) {
        self.request_async(ProxyRequest::BufferHead { path }, f);
    }

    pub fn create_file(&self, path: PathBuf, f: impl ProxyCallback + 'static) {
        self.request_async(ProxyRequest::CreateFile { path }, f);
    }

    pub fn create_directory(&self, path: PathBuf, f: impl ProxyCallback + 'static) {
        self.request_async(ProxyRequest::CreateDirectory { path }, f);
    }

    pub fn trash_path(&self, path: PathBuf, f: impl ProxyCallback + 'static) {
        self.request_async(ProxyRequest::TrashPath { path }, f);
    }

    pub fn duplicate_path(
        &self,
        existing_path: PathBuf,
        new_path: PathBuf,
        f: impl ProxyCallback + 'static,
    ) {
        self.request_async(
            ProxyRequest::DuplicatePath {
                existing_path,
                new_path,
            },
            f,
        );
    }

    pub fn rename_path(
        &self,
        from: PathBuf,
        to: PathBuf,
        f: impl ProxyCallback + 'static,
    ) {
        self.request_async(ProxyRequest::RenamePath { from, to }, f);
    }

    pub fn test_create_at_path(
        &self,
        path: PathBuf,
        f: impl ProxyCallback + 'static,
    ) {
        self.request_async(ProxyRequest::TestCreateAtPath { path }, f);
    }

    pub fn save_buffer_as(
        &self,
        buffer_id: BufferId,
        path: PathBuf,
        rev: u64,
        content: String,
        create_parents: bool,
        f: impl ProxyCallback + 'static,
    ) {
        self.request_async(
            ProxyRequest::SaveBufferAs {
                buffer_id,
                path,
                rev,
                content,
                create_parents,
            },
            f,
        );
    }

    pub fn global_search(
        &self,
        pattern: String,
        case_sensitive: bool,
        whole_word: bool,
        is_regex: bool,
        f: impl ProxyCallback + 'static,
    ) {
        self.request_async(
            ProxyRequest::GlobalSearch {
                pattern,
                case_sensitive,
                whole_word,
                is_regex,
            },
            f,
        );
    }

    pub fn save(
        &self,
        rev: u64,
        path: PathBuf,
        create_parents: bool,
        f: impl ProxyCallback + 'static,
    ) {
        self.request_async(
            ProxyRequest::Save {
                rev,
                path,
                create_parents,
            },
            f,
        );
    }

    pub fn get_files(&self, f: impl ProxyCallback + 'static) {
        self.request_async(
            ProxyRequest::GetFiles {
                path: "path".into(),
            },
            f,
        );
    }

    pub fn read_dir(&self, path: PathBuf, f: impl ProxyCallback + 'static) {
        self.request_async(ProxyRequest::ReadDir { path }, f);
    }

    pub fn update(&self, path: PathBuf, delta: RopeDelta, rev: u64) {
        self.notification(ProxyNotification::Update { path, delta, rev });
    }

    pub fn git_discard_files_changes(&self, files: Vec<PathBuf>) {
        self.notification(ProxyNotification::GitDiscardFilesChanges { files });
    }

    pub fn git_discard_workspace_changes(&self) {
        self.notification(ProxyNotification::GitDiscardWorkspaceChanges {});
    }

    pub fn git_get_remote_file_url(
        &self,
        file: PathBuf,
        f: impl ProxyCallback + 'static,
    ) {
        self.request_async(ProxyRequest::GitGetRemoteFileUrl { file }, f);
    }
}

impl Default for ProxyRpcHandler {
    fn default() -> Self {
        Self::new()
    }
}
