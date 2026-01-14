use rmpv::Value;
use serde::{Deserialize, Serialize};

/// MessagePack-RPC Message Type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RpcMessageType {
    Request = 0,
    Response = 1,
    Notification = 2,
}

/// Helper to parse RPC message type
pub fn parse_message_type(val: &Value) -> Option<RpcMessageType> {
    match val.as_i64() {
        Some(0) => Some(RpcMessageType::Request),
        Some(1) => Some(RpcMessageType::Response),
        Some(2) => Some(RpcMessageType::Notification),
        _ => None,
    }
}
