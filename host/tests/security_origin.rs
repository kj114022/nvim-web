use std::sync::Arc;
use tokio::net::TcpStream;
use tokio::sync::RwLock;
use tokio_tungstenite::{connect_async, tungstenite::client::IntoClientRequest};
use nvim_web_host::ws::serve_multi_async;
use nvim_web_host::session::AsyncSessionManager;

#[tokio::test]
async fn test_origin_validation() {
    // Start server in background
    let manager = Arc::new(RwLock::new(AsyncSessionManager::new()));
    
    // Note: We can't easily dynamically bind port in serve_multi_async without changing signature.
    // For this test, we assume port 9001 is available or we spin up a modified version.
    // Since serve_multi_async binds to 9001 hardcoded, we have to use that.
    
    // Check if port 9001 is available, if not, skip test or fail gracefully
    if TcpStream::connect("127.0.0.1:9001").await.is_ok() {
        eprintln!("Port 9001 in use, skimming origin test");
        return;
    }

    tokio::spawn(async move {
        if let Err(e) = serve_multi_async(manager).await {
            eprintln!("Server error: {}", e);
        }
    });

    // Give server time to start
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    let url = "ws://127.0.0.1:9001";

    // Test 1: Valid Origin (localhost)
    {
        let mut request = url.into_client_request().unwrap();
        request.headers_mut().insert("Origin", "http://localhost:8080".parse().unwrap());
        
        match connect_async(request).await {
            Ok(_) => println!("Valid origin accepted: OK"),
            Err(e) => panic!("Valid origin rejected: {}", e),
        }
    }

    // Test 2: Invalid Origin
    {
        let mut request = url.into_client_request().unwrap();
        request.headers_mut().insert("Origin", "http://evil.com".parse().unwrap());
        
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
