use std::sync::Arc;

use nvim_web_host::session::AsyncSessionManager;
use nvim_web_host::ws::serve_multi_async;
use tokio::net::TcpStream;
use tokio::sync::RwLock;
use tokio_tungstenite::{connect_async, tungstenite::client::IntoClientRequest};

#[tokio::test]
async fn test_origin_validation() {
    // Start server in background
    let manager = Arc::new(RwLock::new(AsyncSessionManager::new()));

    // Check if port 9003 is available, if not, skip test or fail gracefully
    if TcpStream::connect("127.0.0.1:9003").await.is_ok() {
        eprintln!("Port 9003 in use, skimming origin test");
        return;
    }

    tokio::spawn(async move {
        if let Err(e) = serve_multi_async(manager, 9003).await {
            eprintln!("Server error: {}", e);
        }
    });

    // Give server time to start
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    let url = "ws://127.0.0.1:9003";

    // Test 1: Valid Origin (localhost)
    {
        let mut request = url.into_client_request().unwrap();
        request
            .headers_mut()
            .insert("Origin", "http://localhost:8080".parse().unwrap());

        match connect_async(request).await {
            Ok(_) => println!("Valid origin accepted: OK"),
            Err(e) => panic!("Valid origin rejected: {}", e),
        }
    }

    // Test 2: Invalid Origin
    {
        let mut request = url.into_client_request().unwrap();
        request
            .headers_mut()
            .insert("Origin", "http://evil.com".parse().unwrap());

        match connect_async(request).await {
            Ok(_) => panic!("Invalid origin accepted! Security failure."),
            Err(_) => println!("Invalid origin rejected: OK"),
        }
    }

    // Test 3: No Origin (should be accepted as same-origin/tool)
    {
        let request = url.into_client_request().unwrap();
        // No Origin header added

        match connect_async(request).await {
            Ok(_) => println!("No origin accepted: OK"),
            Err(e) => panic!("No origin rejected: {}", e),
        }
    }
}
