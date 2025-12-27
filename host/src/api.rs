//! REST API server for nvim-web
//!
//! Provides HTTP endpoints for session management and automation.

use std::sync::Arc;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::RwLock;

use crate::session::{AsyncSessionManager, SessionInfo};

/// Simple HTTP response builder
fn http_response(status: u16, status_text: &str, content_type: &str, body: &str) -> String {
    format!(
        "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nAccess-Control-Allow-Origin: *\r\nConnection: close\r\n\r\n{}",
        status, status_text, content_type, body.len(), body
    )
}

fn json_response(body: &str) -> String {
    http_response(200, "OK", "application/json", body)
}

fn not_found() -> String {
    http_response(404, "Not Found", "text/plain", "Not Found")
}

// Reserved for future use
#[allow(dead_code)]
fn method_not_allowed() -> String {
    http_response(
        405,
        "Method Not Allowed",
        "text/plain",
        "Method Not Allowed",
    )
}

/// Parse HTTP request and extract method and path
fn parse_request(request: &str) -> Option<(&str, &str)> {
    let first_line = request.lines().next()?;
    let parts: Vec<&str> = first_line.split_whitespace().collect();
    if parts.len() >= 2 {
        Some((parts[0], parts[1]))
    } else {
        None
    }
}

/// Handle REST API requests
async fn handle_request(
    request: &str,
    session_manager: Arc<RwLock<AsyncSessionManager>>,
) -> String {
    let (method, path) = match parse_request(request) {
        Some((m, p)) => (m, p),
        None => return http_response(400, "Bad Request", "text/plain", "Bad Request"),
    };

    match (method, path) {
        // Health check
        ("GET", "/api/health") => {
            json_response(r#"{"status":"ok","version":"0.1.0"}"#)
        }

        // List sessions
        ("GET", "/api/sessions") => {
            let mgr = session_manager.read().await;
            let sessions: Vec<SessionInfo> = mgr.list_sessions();
            let json = format!(
                r#"{{"sessions":[{}]}}"#,
                sessions.iter()
                    .map(|s| format!(
                        r#"{{"id":"{}","age_secs":{},"connected":{}}}"#,
                        s.id, s.age_secs, s.connected
                    ))
                    .collect::<Vec<_>>()
                    .join(",")
            );
            json_response(&json)
        }

        // Get session count
        ("GET", "/api/sessions/count") => {
            let mgr = session_manager.read().await;
            json_response(&format!(r#"{{"count":{}}}"#, mgr.session_count()))
        }

        // Create new session
        ("POST", "/api/sessions") => {
            let mut mgr = session_manager.write().await;
            match mgr.create_session().await {
                Ok(id) => json_response(&format!(r#"{{"id":"{}","created":true}}"#, id)),
                Err(e) => http_response(500, "Internal Server Error", "application/json",
                    &format!(r#"{{"error":"{}"}}"#, e)),
            }
        }

        // Delete session
        ("DELETE", path) if path.starts_with("/api/sessions/") => {
            let id = &path["/api/sessions/".len()..];
            let mut mgr = session_manager.write().await;
            if mgr.remove_session(id).is_some() {
                json_response(&format!(r#"{{"id":"{}","deleted":true}}"#, id))
            } else {
                http_response(
                    404,
                    "Not Found",
                    "application/json",
                    r#"{"error":"session not found"}"#,
                )
            }
        }

        // Open project (magic link) - generate token
        // POST /api/open with body: {"path":"/abs/path"}
        ("POST", "/api/open") => {
            // Parse JSON body
            let body_start = request.find("\r\n\r\n").map(|i| i + 4).unwrap_or(0);
            let body = &request[body_start..];
            
            // Simple JSON parsing for path
            let path = body
                .split('"')
                .enumerate()
                .find(|(i, s)| *i % 4 == 1 && *s == "path")
                .and_then(|_| body.split('"').nth(3))
                .unwrap_or("");
            
            if path.is_empty() {
                return http_response(400, "Bad Request", "application/json", 
                    r#"{"error":"path is required"}"#);
            }
            
            let abs_path = std::path::PathBuf::from(path);
            if !abs_path.exists() {
                return http_response(404, "Not Found", "application/json",
                    r#"{"error":"path does not exist"}"#);
            }
            
            let abs_path = abs_path.canonicalize().unwrap_or(abs_path);
            let config = crate::project::ProjectConfig::load(&abs_path);
            let name = config.display_name(&abs_path);
            let token = crate::project::store_token(abs_path.clone(), config);
            
            json_response(&format!(
                r#"{{"token":"{}","name":"{}","path":"{}","url":"http://localhost:8080/?open={}"}}"#,
                token, name, abs_path.display(), token
            ))
        }

        // Claim token - exchange for session info
        ("GET", path) if path.starts_with("/api/claim/") => {
            let token = &path["/api/claim/".len()..];
            
            match crate::project::claim_token(token) {
                Some((path, config)) => {
                    let name = config.display_name(&path);
                    let cwd = config.resolved_cwd(&path);
                    let init_file = config.editor.init_file.clone().unwrap_or_default();
                    
                    json_response(&format!(
                        r#"{{"path":"{}","name":"{}","cwd":"{}","init_file":"{}"}}"#,
                        path.display(), name, cwd.display(), init_file
                    ))
                }
                None => http_response(404, "Not Found", "application/json",
                    r#"{"error":"token invalid or expired"}"#),
            }
        }

        // Get token info (without claiming)
        ("GET", path) if path.starts_with("/api/token/") => {
            let token = &path["/api/token/".len()..];
            
            match crate::project::get_token_info(token) {
                Some((path, config, claimed)) => {
                    let name = config.display_name(&path);
                    json_response(&format!(
                        r#"{{"path":"{}","name":"{}","claimed":{}}}"#,
                        path.display(), name, claimed
                    ))
                }
                None => http_response(404, "Not Found", "application/json",
                    r#"{"error":"token not found"}"#),
            }
        }

        // CORS preflight
        ("OPTIONS", _) => {
            "HTTP/1.1 204 No Content\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Allow-Methods: GET, POST, DELETE, OPTIONS\r\nAccess-Control-Allow-Headers: Content-Type\r\nConnection: close\r\n\r\n".to_string()
        }

        _ => not_found(),
    }
}

/// Start REST API server
pub async fn serve_api(
    addr: &str,
    session_manager: Arc<RwLock<AsyncSessionManager>>,
) -> anyhow::Result<()> {
    let listener = TcpListener::bind(addr).await?;
    eprintln!("API: REST server listening on {}", addr);

    loop {
        let (mut socket, _) = listener.accept().await?;
        let mgr = session_manager.clone();

        tokio::spawn(async move {
            let mut buf = vec![0u8; 4096];
            match socket.read(&mut buf).await {
                Ok(n) if n > 0 => {
                    let request = String::from_utf8_lossy(&buf[..n]);
                    let response = handle_request(&request, mgr).await;
                    let _ = socket.write_all(response.as_bytes()).await;
                }
                _ => {}
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_request() {
        let req = "GET /api/health HTTP/1.1\r\nHost: localhost\r\n\r\n";
        let (method, path) = parse_request(req).unwrap();
        assert_eq!(method, "GET");
        assert_eq!(path, "/api/health");
    }

    #[test]
    fn test_json_response() {
        let resp = json_response(r#"{"ok":true}"#);
        assert!(resp.contains("HTTP/1.1 200 OK"));
        assert!(resp.contains("application/json"));
        assert!(resp.contains(r#"{"ok":true}"#));
    }
}
