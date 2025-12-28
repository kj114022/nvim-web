//! Input queue for FIFO input handling - decoupled from rendering
//! Includes backpressure handling with retry logic

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::rc::Rc;
use web_sys::WebSocket;

const MAX_RETRIES: u8 = 5;

/// Connection state for resilience tracking
#[derive(Clone, Copy, PartialEq, Debug)]
#[allow(dead_code)]
pub enum ConnectionState {
    Connected,
    Degraded,     // Experiencing failures but still trying
    Disconnected, // WebSocket closed
}

/// Input queue for FIFO keyboard/mouse input handling
/// Features: FIFO ordering, backpressure, retry logic, offline queueing
pub struct InputQueue {
    queue: RefCell<VecDeque<(Vec<u8>, u8)>>, // (bytes, retry_count)
    ws: RefCell<Option<WebSocket>>,
    send_failures: Cell<u32>,
    state: Cell<ConnectionState>,
}

impl InputQueue {
    pub fn new(ws: WebSocket) -> Rc<Self> {
        Rc::new(Self {
            queue: RefCell::new(VecDeque::new()),
            ws: RefCell::new(Some(ws)),
            send_failures: Cell::new(0),
            state: Cell::new(ConnectionState::Connected),
        })
    }

    /// Get current connection state
    #[allow(dead_code)]
    pub fn connection_state(&self) -> ConnectionState {
        self.state.get()
    }

    /// Update WebSocket on reconnection
    #[allow(dead_code)]
    pub fn set_websocket(&self, ws: WebSocket) {
        *self.ws.borrow_mut() = Some(ws);
        self.state.set(ConnectionState::Connected);
        self.send_failures.set(0);
        web_sys::console::log_1(&"InputQueue: WebSocket reconnected".into());
        self.flush();
    }

    /// Mark as disconnected (called from onclose handler)
    #[allow(dead_code)]
    pub fn mark_disconnected(&self) {
        self.state.set(ConnectionState::Disconnected);
        web_sys::console::warn_1(&"InputQueue: Disconnected, messages queued".into());
    }

    /// Enqueue an input event (already encoded as msgpack bytes)
    pub fn enqueue(&self, bytes: Vec<u8>) {
        self.queue.borrow_mut().push_back((bytes, 0));
        self.flush();
    }

    /// Calculate backoff delay for logging (100, 200, 400, 800, 1600ms)
    fn backoff_delay_ms(retry_count: u8) -> u32 {
        100 * (1 << retry_count.min(4))
    }

    /// Flush all queued input to WebSocket
    /// Implements backpressure with retry logic
    pub fn flush(&self) {
        let ws_opt = self.ws.borrow();
        let ws = match ws_opt.as_ref() {
            Some(ws) if ws.ready_state() == WebSocket::OPEN => ws,
            _ => {
                // Queue remains intact for when connection is restored
                return;
            }
        };

        let mut queue = self.queue.borrow_mut();
        let mut requeue: VecDeque<(Vec<u8>, u8)> = VecDeque::new();
        
        while let Some((bytes, retry_count)) = queue.pop_front() {
            match ws.send_with_u8_array(&bytes) {
                Ok(_) => {
                    if self.send_failures.get() > 0 {
                        self.send_failures.set(0);
                        self.state.set(ConnectionState::Connected);
                        web_sys::console::log_1(&"InputQueue: Send recovered".into());
                    }
                }
                Err(e) => {
                    self.send_failures.set(self.send_failures.get() + 1);
                    
                    if self.send_failures.get() >= 3 {
                        self.state.set(ConnectionState::Degraded);
                    }
                    
                    if retry_count < MAX_RETRIES {
                        requeue.push_back((bytes, retry_count + 1));
                        web_sys::console::warn_1(&format!(
                            "InputQueue: Send failed (retry {}/{}, backoff ~{}ms): {:?}", 
                            retry_count + 1, MAX_RETRIES, 
                            Self::backoff_delay_ms(retry_count), e
                        ).into());
                    } else {
                        web_sys::console::error_1(&format!(
                            "InputQueue: Dropping after {} retries: {:?}", 
                            MAX_RETRIES, e
                        ).into());
                    }
                }
            }
        }
        
        for item in requeue {
            queue.push_back(item);
        }
    }

    /// Send a key input (convenience method)
    pub fn send_key(&self, nvim_key: &str) {
        let msg = rmpv::Value::Array(vec![
            rmpv::Value::String("input".into()),
            rmpv::Value::String(nvim_key.into()),
        ]);
        
        let mut bytes = Vec::new();
        if rmpv::encode::write_value(&mut bytes, &msg).is_ok() {
            self.enqueue(bytes);
        }
    }

    /// Get queue length (for diagnostics)
    #[allow(dead_code)]
    pub fn pending_count(&self) -> usize {
        self.queue.borrow().len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_backoff_delay() {
        assert_eq!(InputQueue::backoff_delay_ms(0), 100);
        assert_eq!(InputQueue::backoff_delay_ms(1), 200);
        assert_eq!(InputQueue::backoff_delay_ms(2), 400);
        assert_eq!(InputQueue::backoff_delay_ms(3), 800);
        assert_eq!(InputQueue::backoff_delay_ms(4), 1600);
        assert_eq!(InputQueue::backoff_delay_ms(5), 1600); // Capped
    }
}
