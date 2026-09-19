#![allow(clippy::manual_clamp)]

pub mod buffer;
pub mod core;
pub mod counter;
pub mod file;
mod parse;
pub mod proxy;
pub mod source_control;
pub mod stdio;
pub mod terminal;

pub use parse::{RequestId, RpcObject};
use serde::{Deserialize, Serialize};

#[derive(Debug)]
pub enum RpcMessage<Req, Notif, Resp> {
    Request(RequestId, Req),
    Response(RequestId, Resp),
    Notification(Notif),
    Error(RequestId, RpcError),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcError {
    pub code: i64,
    pub message: String,
}
