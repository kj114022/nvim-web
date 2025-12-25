use std::net::TcpListener;
use std::sync::Arc;

use nvim_web_host::session::AsyncSessionManager;
use nvim_web_host::ws;
use tokio::signal;
use tokio::sync::RwLock;

const VERSION: &str = env!("CARGO_PKG_VERSION");
const DEFAULT_WS_PORT: u16 = 9001;
const DEFAULT_BIND: &str = "127.0.0.1";

fn print_banner() {
    eprintln!();
    eprintln!("  \x1b[1;36m  _   ___     _____ __  __    __        _______ ____  \x1b[0m");
    eprintln!("  \x1b[1;36m | \\ | \\ \\   / /_ _|  \\/  |   \\ \\      / / ____| __ ) \x1b[0m");
    eprintln!("  \x1b[1;36m |  \\| |\\ \\ / / | || |\\/| |____\\ \\ /\\ / /|  _| |  _ \\ \x1b[0m");
    eprintln!("  \x1b[1;36m | |\\  | \\ V /  | || |  | |_____\\ V  V / | |___| |_) |\x1b[0m");
    eprintln!("  \x1b[1;36m |_| \\_|  \\_/  |___|_|  |_|      \\_/\\_/  |_____|____/ \x1b[0m");
    eprintln!();
    eprintln!("  \x1b[2mNeovim in the Browser  v{}\x1b[0m", VERSION);
    eprintln!();
}

fn print_connection_info(port: u16) {
    eprintln!(
        "  \x1b[1;32m[ready]\x1b[0m WebSocket server listening on port {}",
        port
    );
    eprintln!();
    eprintln!("  \x1b[1mConnect:\x1b[0m  \x1b[4;34mhttp://localhost:8080\x1b[0m");
    eprintln!(
        "  \x1b[1mServer:\x1b[0m   \x1b[2mws://{}:{}\x1b[0m",
        DEFAULT_BIND, port
    );
    eprintln!();
    eprintln!("  \x1b[2mServe UI:\x1b[0m cd ui && python3 -m http.server 8080");
    eprintln!();
    eprintln!("  \x1b[2mPress Ctrl+C to stop\x1b[0m");
    eprintln!();
}

/// Graceful start: Check if port is available
fn check_port_available(port: u16) -> bool {
    TcpListener::bind(format!("{}:{}", DEFAULT_BIND, port)).is_ok()
}

/// Graceful start: Find available port starting from default
fn find_available_port(start: u16) -> Option<u16> {
    (start..start + 10).find(|&port| check_port_available(port))
}

/// Startup health checks
fn startup_checks() -> Result<(), String> {
    // Check if nvim is available
    match std::process::Command::new("nvim").arg("--version").output() {
        Ok(output) if output.status.success() => {
            let version = String::from_utf8_lossy(&output.stdout);
            let first_line = version.lines().next().unwrap_or("unknown");
            eprintln!("  \x1b[1;32m[check]\x1b[0m Neovim found: {}", first_line);
        }
        _ => {
            return Err("Neovim not found. Please install Neovim first.".to_string());
        }
    }
    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Handle --version and --help
    let args: Vec<String> = std::env::args().collect();
    if args.len() > 1 {
        match args[1].as_str() {
            "--version" | "-v" => {
                println!("nvim-web {}", VERSION);
                return Ok(());
            }
            "--help" | "-h" => {
                println!("nvim-web - Neovim in the Browser");
                println!();
                println!("USAGE:");
                println!("    nvim-web [OPTIONS]");
                println!();
                println!("OPTIONS:");
                println!("    -h, --help       Print help information");
                println!("    -v, --version    Print version");
                println!();
                println!("QUICKSTART:");
                println!("    1. Run: nvim-web");
                println!("    2. Serve UI: cd ui && python3 -m http.server 8080");
                println!("    3. Open: http://localhost:8080");
                return Ok(());
            }
            _ => {}
        }
    }

    print_banner();

    // === GRACEFUL START ===
    eprintln!("  \x1b[1;33m[starting]\x1b[0m Running startup checks...");

    // Check Neovim availability
    if let Err(e) = startup_checks() {
        eprintln!("  \x1b[1;31m[error]\x1b[0m {}", e);
        std::process::exit(1);
    }

    // Check port availability
    let port = if check_port_available(DEFAULT_WS_PORT) {
        DEFAULT_WS_PORT
    } else {
        eprintln!(
            "  \x1b[1;33m[warn]\x1b[0m Port {} in use, finding alternative...",
            DEFAULT_WS_PORT
        );
        match find_available_port(DEFAULT_WS_PORT + 1) {
            Some(p) => {
                eprintln!("  \x1b[1;32m[check]\x1b[0m Using port {}", p);
                p
            }
            None => {
                eprintln!(
                    "  \x1b[1;31m[error]\x1b[0m No available ports in range {}-{}",
                    DEFAULT_WS_PORT,
                    DEFAULT_WS_PORT + 10
                );
                std::process::exit(1);
            }
        }
    };

    // Create async session manager
    let session_manager = Arc::new(RwLock::new(AsyncSessionManager::new()));
    let session_manager_shutdown = session_manager.clone();

    print_connection_info(port);

    // === GRACEFUL SHUTDOWN HANDLER ===
    let shutdown_signal = async {
        // Wait for Ctrl+C or SIGTERM
        let ctrl_c = async {
            signal::ctrl_c()
                .await
                .expect("Failed to install Ctrl+C handler");
        };

        #[cfg(unix)]
        let terminate = async {
            signal::unix::signal(signal::unix::SignalKind::terminate())
                .expect("Failed to install SIGTERM handler")
                .recv()
                .await;
        };

        #[cfg(not(unix))]
        let terminate = std::future::pending::<()>();

        tokio::select! {
            _ = ctrl_c => {},
            _ = terminate => {},
        }

        eprintln!();
        eprintln!("  \x1b[1;33m[shutdown]\x1b[0m Graceful shutdown initiated...");

        // Cleanup sessions
        let mut mgr = session_manager_shutdown.write().await;
        let session_count = mgr.session_count();
        eprintln!(
            "  \x1b[1;33m[shutdown]\x1b[0m Cleaning up {} sessions...",
            session_count
        );

        // Clean up all sessions
        let ids: Vec<String> = mgr.session_ids();
        for id in ids {
            mgr.remove_session(&id);
        }

        eprintln!("  \x1b[1;32m[shutdown]\x1b[0m Cleanup complete. Goodbye!");
        eprintln!();
    };

    // Run server with shutdown handler
    tokio::select! {
        result = ws::serve_multi_async(session_manager, port) => {
            result?;
        }
        _ = shutdown_signal => {
            // Shutdown was triggered
        }
    }

    Ok(())
}
