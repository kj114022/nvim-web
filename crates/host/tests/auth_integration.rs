use anyhow::Result;
use nvim_web_host::auth::{perform_client_handshake, verify_hmac};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

#[tokio::test]
async fn test_auth_handshake_flow() -> Result<()> {
    // 1. Setup Mock Server
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;
    let token = "integration_test_secret_token";
    let nonce = [42u8; 32]; // Fixed nonce for test

    // Spawn Server
    let server_handle = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();

        // S1: Send Nonce
        socket.write_all(&nonce).await.unwrap();

        // S2: Read HMAC
        let mut client_hmac = [0u8; 32];
        socket.read_exact(&mut client_hmac).await.unwrap();

        // S3: Verify
        if verify_hmac(&nonce, token, &client_hmac) {
            // Keep connection open (success)
            // Send a "mock" byte to signal we are still here (not strictly part of protocol but good for test sync)
            socket.write_all(b"OK").await.unwrap();
        } else {
            // Close connection
            drop(socket);
        }
    });

    // 2. Run Client
    let mut client_stream = tokio::net::TcpStream::connect(addr).await?;
    perform_client_handshake(&mut client_stream, token).await?;

    // 3. Verify success (Client should be able to read "OK")
    let mut response = [0u8; 2];
    client_stream.read_exact(&mut response).await?;
    assert_eq!(&response, b"OK");

    server_handle.await?;
    Ok(())
}

#[tokio::test]
async fn test_auth_handshake_failure() -> Result<()> {
    // 1. Setup Mock Server
    let listener = TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;
    let server_token = "server_token";
    let client_token = "wrong_client_token";
    let nonce = [99u8; 32];

    // Spawn Server
    let server_handle = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();

        socket.write_all(&nonce).await.unwrap();

        let mut client_hmac = [0u8; 32];
        socket.read_exact(&mut client_hmac).await.unwrap();

        // Verify with SERVER token
        if verify_hmac(&nonce, server_token, &client_hmac) {
            socket.write_all(b"OK").await.unwrap();
        } else {
            // Close connection immediately
            // socket dropped
        }
    });

    // 2. Run Client
    let mut client_stream = tokio::net::TcpStream::connect(addr).await?;
    perform_client_handshake(&mut client_stream, client_token).await?;

    // 3. Verify failure (Reading should fail with EOF)
    let mut response = [0u8; 2];
    match client_stream.read_exact(&mut response).await {
        Ok(_) => panic!("Client should have been disconnected"),
        Err(e) => assert_eq!(e.kind(), std::io::ErrorKind::UnexpectedEof),
    }

    server_handle.await?;
    Ok(())
}
