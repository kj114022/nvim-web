//! Input queue available for FIFO input handling
//! Includes backpressure, retry logic.
//! This module is safe for Worker usage (no DOM listeners).

use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::rc::Rc;
use web_sys::WebSocket;

const MAX_RETRIES: u8 = 5;

/// Connection state for resilience tracking
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[allow(dead_code)]
pub enum ConnectionState {
    Connected,
    Degraded,     // Experiencing failures but still trying
    Disconnected, // WebSocket closed
}

/// Input queue for FIFO keyboard/mouse input handling
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

    /// Hot swap WebSocket for backend migration
    /// Gracefully closes old connection before swapping
    pub fn replace_websocket(&self, new_ws: WebSocket) {
        // Mark disconnected to stop flush attempts on old socket
        self.state.set(ConnectionState::Disconnected);

        // Gracefully close old socket if open
        if let Some(old_ws) = self.ws.borrow_mut().take() {
            if old_ws.ready_state() == WebSocket::OPEN
                || old_ws.ready_state() == WebSocket::CONNECTING
            {
                let _ = old_ws.close();
            }
        }

        // Swap in new socket
        *self.ws.borrow_mut() = Some(new_ws);

        // Reset state
        self.state.set(ConnectionState::Connected);
        self.send_failures.set(0);

        // Trigger immediate flush of any queued keys
        self.flush();

        web_sys::console::log_1(&"[InputQueue] Hot swapped WebSocket connection".into());
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

    /// Calculate backoff delay for logging
    const fn backoff_delay_ms(retry_count: u8) -> u32 {
        100 * (1 << (if retry_count < 4 { retry_count } else { 4 }))
    }

    /// Flush all queued input to WebSocket
    pub fn flush(&self) {
        let ws_opt = self.ws.borrow();
        let ws = match ws_opt.as_ref() {
            Some(ws) if ws.ready_state() == WebSocket::OPEN => ws,
            _ => return,
        };

        let mut queue = self.queue.borrow_mut();
        let mut requeue: VecDeque<(Vec<u8>, u8)> = VecDeque::new();

        while let Some((bytes, retry_count)) = queue.pop_front() {
            match ws.send_with_u8_array(&bytes) {
                Ok(()) => {
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
                        web_sys::console::warn_1(
                            &format!(
                                "InputQueue: Send failed (retry {}/{}, backoff ~{}ms): {:?}",
                                retry_count + 1,
                                MAX_RETRIES,
                                Self::backoff_delay_ms(retry_count),
                                e
                            )
                            .into(),
                        );
                    } else {
                        web_sys::console::error_1(
                            &format!("InputQueue: Dropping after {MAX_RETRIES} retries: {e:?}",)
                                .into(),
                        );
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

    /// Send mouse input
    pub fn send_mouse(&self, button: &str, action: &str, row: usize, col: usize, modifiers: &str) {
        let msg = rmpv::Value::Array(vec![
            rmpv::Value::String("mouse".into()),
            rmpv::Value::String(button.into()),
            rmpv::Value::String(action.into()),
            rmpv::Value::String(modifiers.into()),
            rmpv::Value::Integer((row as i64).into()),
            rmpv::Value::Integer((col as i64).into()),
        ]);

        let mut bytes = Vec::new();
        if rmpv::encode::write_value(&mut bytes, &msg).is_ok() {
            self.enqueue(bytes);
        }
    }

    /// Send scroll input
    pub fn send_scroll(&self, direction: &str, row: usize, col: usize, modifiers: &str) {
        let msg = rmpv::Value::Array(vec![
            rmpv::Value::String("scroll".into()),
            rmpv::Value::String(direction.into()),
            rmpv::Value::String(modifiers.into()),
            rmpv::Value::Integer((row as i64).into()),
            rmpv::Value::Integer((col as i64).into()),
        ]);

        let mut bytes = Vec::new();
        if rmpv::encode::write_value(&mut bytes, &msg).is_ok() {
            self.enqueue(bytes);
        }
    }

    /// Send resize notification
    pub fn send_resize(&self, cols: usize, rows: usize) {
        let msg = rmpv::Value::Array(vec![
            rmpv::Value::String("resize".into()),
            rmpv::Value::Integer((cols as i64).into()),
            rmpv::Value::Integer((rows as i64).into()),
        ]);

        let mut bytes = Vec::new();
        if rmpv::encode::write_value(&mut bytes, &msg).is_ok() {
            self.enqueue(bytes);
        }
    }

    /// Send paste text
    pub fn send_paste(&self, text: &str) {
        // Neovim handles paste via `nvim_paste` API
        let msg = rmpv::Value::Array(vec![
            rmpv::Value::String("paste".into()),
            rmpv::Value::String(text.into()),
        ]);

        let mut bytes = Vec::new();
        if rmpv::encode::write_value(&mut bytes, &msg).is_ok() {
            self.enqueue(bytes);
        }
    }

    /// Send file drop event
    pub fn send_file_drop(&self, name: &str, data: &[u8]) {
        let msg = rmpv::Value::Array(vec![
            rmpv::Value::String("file_drop".into()),
            rmpv::Value::String(name.into()),
            rmpv::Value::Binary(data.to_vec()),
        ]);

        let mut bytes = Vec::new();
        if rmpv::encode::write_value(&mut bytes, &msg).is_ok() {
            self.enqueue(bytes);
        }
    }

    /// Get queue length
    #[allow(dead_code)]
    pub fn pending_count(&self) -> usize {
        self.queue.borrow().len()
    }
}
