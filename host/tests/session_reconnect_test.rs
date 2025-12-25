use std::sync::Arc;

use futures::StreamExt;
use nvim_web_host::session::AsyncSessionManager;
use nvim_web_host::ws::serve_multi_async;
use rmpv::Value;
use tokio::net::TcpStream;
use tokio::sync::RwLock;
use tokio_tungstenite::connect_async;

#[tokio::test]
async fn test_session_reconnection() {
    // Start server in background
    let manager = Arc::new(RwLock::new(AsyncSessionManager::new()));

    if TcpStream::connect("127.0.0.1:9002").await.is_ok() {
        eprintln!("Port 9002 in use, skimming reconnection test");
        return;
    }

    tokio::spawn(async move {
        if let Err(e) = serve_multi_async(manager, 9002).await {
            eprintln!("Server error: {}", e);
        }
    });

    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    let url_base = "ws://127.0.0.1:9002";

    // Step 1: Connect and get Session ID
    let session_id = {
        let (mut stream, _) = connect_async(url_base).await.expect("Failed to connect");

        let msg = stream
            .next()
            .await
            .expect("No message")
            .expect("Error reading");
        let data = msg.into_data();
        let val: Value = rmpv::decode::read_value(&mut &data[..]).expect("Invalid msgpack");

        // Expect ["session", "uuid"]
        if let Value::Array(arr) = val {
            assert_eq!(arr.len(), 2);
            assert_eq!(arr[0].as_str(), Some("session"));
            let id = arr[1].as_str().expect("Session ID not string").to_string();
            println!("Got session ID: {}", id);
            id
        } else {
            panic!("Unexpected message format");
        }
    };

    // Step 2: Reconnect with same Session ID
    {
        let reconnect_url = format!("{}/?session={}", url_base, session_id);
        println!("Reconnecting to: {}", reconnect_url);

        let (mut stream, _) = connect_async(&reconnect_url)
            .await
            .expect("Failed to reconnect");

        let msg = stream
            .next()
            .await
            .expect("No message")
            .expect("Error reading");
        let data = msg.into_data();
        let val: Value = rmpv::decode::read_value(&mut &data[..]).expect("Invalid msgpack");

        if let Value::Array(arr) = val {
            assert_eq!(arr.len(), 2);
            assert_eq!(arr[0].as_str(), Some("session"));
            let new_id = arr[1].as_str().expect("Session ID not string").to_string();

            assert_eq!(new_id, session_id, "Session ID mismatch on reconnection!");
            println!("Reconnection successful, session preserved: {}", new_id);
        } else {
            panic!("Unexpected message format on reconnect");
        }
    }

    // Step 3: Connect with NEW session explicitly
    {
        let new_url = format!("{}/?session=new", url_base);
        println!("Requesting new session: {}", new_url);

        let (mut stream, _) = connect_async(&new_url)
            .await
            .expect("Failed to connect new");

        let msg = stream
            .next()
            .await
            .expect("No message")
            .expect("Error reading");
        let data = msg.into_data();
        let val: Value = rmpv::decode::read_value(&mut &data[..]).expect("Invalid msgpack");

        if let Value::Array(arr) = val {
            let new_id = arr[1].as_str().unwrap().to_string();
            assert_ne!(new_id, session_id, "Should have created DIFFERENT session");
            println!("New session created as requested: {}", new_id);
        }
    }
}
