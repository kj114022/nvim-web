use std::net::TcpListener;
use std::sync::Arc;

use axum::{
    body::Body,
    extract::Path,
    http::{header, Response, StatusCode},
    routing::get,
    Router,
};
use nvim_web_host::config::Config;
use nvim_web_host::embedded;
use nvim_web_host::session::AsyncSessionManager;
use nvim_web_host::vfs::{LocalFs, VfsManager};
use nvim_web_host::ws;
use tokio::signal;
use tokio::sync::RwLock;
use tower_http::cors::{Any, CorsLayer};

const VERSION: &str = env!("CARGO_PKG_VERSION");

fn print_banner() {
    // Fresh hip-hop inspired banner - cyan vibes, chill energy
    eprintln!();
    eprintln!("  \x1b[1;36m╔═══════════════════════════════════════════════════════════════════╗\x1b[0m");
    eprintln!("  \x1b[1;36m║                                                                   ║\x1b[0m");
    eprintln!("  \x1b[1;36m║\x1b[0m  \x1b[1;96m███╗   ██╗██╗   ██╗██╗███╗   ███╗    ██╗    ██╗███████╗██████╗ \x1b[1;36m║\x1b[0m");
    eprintln!("  \x1b[1;36m║\x1b[0m  \x1b[1;96m████╗  ██║██║   ██║██║████╗ ████║    ██║    ██║██╔════╝██╔══██╗\x1b[1;36m║\x1b[0m");
    eprintln!("  \x1b[1;36m║\x1b[0m  \x1b[1;96m██╔██╗ ██║██║   ██║██║██╔████╔██║    ██║ █╗ ██║█████╗  ██████╔╝\x1b[1;36m║\x1b[0m");
    eprintln!("  \x1b[1;36m║\x1b[0m  \x1b[1;96m██║╚██╗██║╚██╗ ██╔╝██║██║╚██╔╝██║    ██║███╗██║██╔══╝  ██╔══██╗\x1b[1;36m║\x1b[0m");
    eprintln!("  \x1b[1;36m║\x1b[0m  \x1b[1;96m██║ ╚████║ ╚████╔╝ ██║██║ ╚═╝ ██║    ╚███╔███╔╝███████╗██████╔╝\x1b[1;36m║\x1b[0m");
    eprintln!("  \x1b[1;36m║\x1b[0m  \x1b[1;96m╚═╝  ╚═══╝  ╚═══╝  ╚═╝╚═╝     ╚═╝     ╚══╝╚══╝ ╚══════╝╚═════╝ \x1b[1;36m║\x1b[0m");
    eprintln!("  \x1b[1;36m║                                                                   ║\x1b[0m");
    eprintln!("  \x1b[1;36m║\x1b[0m  \x1b[2;37mNeovim in your browser. No cap.\x1b[0m                                 \x1b[1;36m║\x1b[0m");
    eprintln!("  \x1b[1;36m║\x1b[0m  \x1b[2;35mBuilt different. Stay chill. Edit code.\x1b[0m v{:<21}\x1b[1;36m║\x1b[0m", VERSION);
    eprintln!("  \x1b[1;36m║                                                                   ║\x1b[0m");
    eprintln!("  \x1b[1;36m╚═══════════════════════════════════════════════════════════════════╝\x1b[0m");
    eprintln!();
}

fn print_connection_info(http_port: u16, ws_port: u16, bind: &str, embedded: bool) {
    if embedded {
        eprintln!("  \x1b[1;32m[vibin]\x1b[0m  Single binary mode - all assets embedded");
    }
    eprintln!("  \x1b[1;32m[http]\x1b[0m   Server chillin' at port \x1b[1;96m{}\x1b[0m", http_port);
    eprintln!("  \x1b[1;32m[ws]\x1b[0m     WebSocket vibin' at port \x1b[1;96m{}\x1b[0m", ws_port);
    eprintln!();
    eprintln!("  \x1b[1;37m>\x1b[0m Open: \x1b[4;96mhttp://{}:{}\x1b[0m", bind, http_port);
    eprintln!();
    eprintln!("  \x1b[2mPress Ctrl+C to bounce\x1b[0m");
    eprintln!();
}

/// Graceful start: Check if port is available
fn check_port_available(bind: &str, port: u16) -> bool {
    TcpListener::bind(format!("{}:{}", bind, port)).is_ok()
}

/// Graceful start: Find available port starting from default
fn find_available_port(bind: &str, start: u16) -> Option<u16> {
    (start..start + 10).find(|&port| check_port_available(bind, port))
}

/// Startup health checks
fn startup_checks() -> Result<(), String> {
    // Check if nvim is available
    match std::process::Command::new("nvim").arg("--version").output() {
        Ok(output) if output.status.success() => {
            let version = String::from_utf8_lossy(&output.stdout);
            let first_line = version.lines().next().unwrap_or("unknown");
            eprintln!("  \x1b[1;32m[check]\x1b[0m  Neovim found: {}", first_line);
        }
        _ => {
            return Err("Neovim not found. Please install Neovim first.".to_string());
        }
    }
    Ok(())
}

/// Serve embedded static file
async fn serve_static(Path(path): Path<String>) -> Response<Body> {
    let path = if path.is_empty() { "index.html".to_string() } else { path };
    
    match embedded::get_asset(&path) {
        Some((data, mime)) => {
            Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, mime)
                .header(header::CACHE_CONTROL, "public, max-age=3600")
                .body(Body::from(data))
                .unwrap()
        }
        None => {
            // Fallback to index.html for SPA routing
            if let Some((data, mime)) = embedded::get_asset("index.html") {
                Response::builder()
                    .status(StatusCode::OK)
                    .header(header::CONTENT_TYPE, mime)
                    .body(Body::from(data))
                    .unwrap()
            } else {
                Response::builder()
                    .status(StatusCode::NOT_FOUND)
                    .body(Body::from("Not Found"))
                    .unwrap()
            }
        }
    }
}

/// Serve index.html at root
async fn serve_index() -> Response<Body> {
    match embedded::get_asset("index.html") {
        Some((data, mime)) => {
            Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, mime)
                .body(Body::from(data))
                .unwrap()
        }
        None => {
            Response::builder()
                .status(StatusCode::NOT_FOUND)
                .body(Body::from("index.html not found"))
                .unwrap()
        }
    }
}

/// Handle 'nvim-web open [PATH]' command
async fn handle_open_command(args: &[String]) -> anyhow::Result<()> {
    use nvim_web_host::project::ProjectConfig;
    use std::path::PathBuf;
    
    // Get path from args, default to current directory
    let path_arg = args.get(2).map(|s| s.as_str()).unwrap_or(".");
    
    // Resolve to absolute path
    let path = if path_arg.starts_with('~') {
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
        PathBuf::from(path_arg.replacen('~', &home, 1))
    } else {
        PathBuf::from(path_arg)
    };
    
    let abs_path = std::fs::canonicalize(&path).map_err(|e| {
        anyhow::anyhow!("Path '{}' not found: {}", path.display(), e)
    })?;
    
    // Load project config
    let config = ProjectConfig::load(&abs_path);
    let name = config.display_name(&abs_path);
    
    eprintln!();
    eprintln!("  \x1b[1;96m[open]\x1b[0m   Project: \x1b[1m{}\x1b[0m", name);
    eprintln!("  \x1b[1;96m[open]\x1b[0m   Path: {}", abs_path.display());
    
    // Generate token
    let token = nvim_web_host::project::store_token(abs_path.clone(), config);
    
    // Build URL
    let url = format!("http://localhost:8080/?open={}", token);
    eprintln!("  \x1b[1;96m[open]\x1b[0m   URL: {}", url);
    eprintln!();
    eprintln!("  \x1b[1;32m>\x1b[0m Opening in browser...");
    eprintln!();
    
    // Open browser
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open").arg(&url).spawn();
    }
    #[cfg(target_os = "linux")]
    {
        let _ = std::process::Command::new("xdg-open").arg(&url).spawn();
    }
    #[cfg(target_os = "windows")]
    {
        let _ = std::process::Command::new("cmd").args(["/C", "start", &url]).spawn();
    }
    
    // Note: The server needs to be running for this to work
    eprintln!("  \x1b[2m(Make sure nvim-web server is running in another terminal)\x1b[0m");
    eprintln!();
    
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
                println!("    nvim-web [COMMAND] [OPTIONS]");
                println!();
                println!("COMMANDS:");
                println!("    open [PATH]      Open a project in the browser");
                println!("    serve            Start server (default)");
                println!();
                println!("OPTIONS:");
                println!("    -h, --help       Print help information");
                println!("    -v, --version    Print version");
                println!();
                println!("CONFIG:");
                println!("    ~/.config/nvim-web/config.toml");
                println!();
                println!("EXAMPLES:");
                println!("    nvim-web              Start server");
                println!("    nvim-web open .       Open current directory in browser");
                println!("    nvim-web open ~/code  Open ~/code in browser");
                return Ok(());
            }
            "open" => {
                // Magic link: Open a project in the browser
                return handle_open_command(&args).await;
            }
            "serve" => {
                // Explicit serve command - fall through to server start
            }
            _ => {}
        }
    }

    print_banner();

    // === LOAD CONFIGURATION ===
    Config::create_default_if_missing();
    let config = Config::load();
    eprintln!(
        "  \x1b[1;32m[config]\x1b[0m Loaded from {}",
        Config::default_config_path().display()
    );

    // === GRACEFUL START ===
    eprintln!("  \x1b[1;33m[init]\x1b[0m   Running startup checks...");

    // Check Neovim availability
    if let Err(e) = startup_checks() {
        eprintln!("  \x1b[1;31m[error]\x1b[0m  {}", e);
        std::process::exit(1);
    }

    // Check HTTP port availability
    let http_port = if check_port_available(&config.server.bind, config.server.http_port) {
        config.server.http_port
    } else {
        eprintln!(
            "  \x1b[1;33m[warn]\x1b[0m   Port {} in use, finding alternative...",
            config.server.http_port
        );
        match find_available_port(&config.server.bind, config.server.http_port + 1) {
            Some(p) => {
                eprintln!("  \x1b[1;32m[check]\x1b[0m  Using HTTP port {}", p);
                p
            }
            None => {
                eprintln!(
                    "  \x1b[1;31m[error]\x1b[0m  No available HTTP ports in range {}-{}",
                    config.server.http_port,
                    config.server.http_port + 10
                );
                std::process::exit(1);
            }
        }
    };

    // Check WS port availability
    let ws_port = if check_port_available(&config.server.bind, config.server.ws_port) {
        config.server.ws_port
    } else {
        eprintln!(
            "  \x1b[1;33m[warn]\x1b[0m   WS Port {} in use, finding alternative...",
            config.server.ws_port
        );
        match find_available_port(&config.server.bind, config.server.ws_port + 1) {
            Some(p) => {
                eprintln!("  \x1b[1;32m[check]\x1b[0m  Using WS port {}", p);
                p
            }
            None => {
                eprintln!(
                    "  \x1b[1;31m[error]\x1b[0m  No available WS ports in range {}-{}",
                    config.server.ws_port,
                    config.server.ws_port + 10
                );
                std::process::exit(1);
            }
        }
    };

    // Create async session manager
    let session_manager = Arc::new(RwLock::new(AsyncSessionManager::new()));
    let session_manager_shutdown = session_manager.clone();

    // Create VFS manager with local filesystem backend
    let mut vfs = VfsManager::new();
    let home_dir = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    vfs.register_backend("local", Box::new(LocalFs::new(&home_dir)));
    let vfs_manager = Arc::new(RwLock::new(vfs));
    eprintln!("  \x1b[1;32m[vfs]\x1b[0m    Backend: local (root: {})", home_dir);

    print_connection_info(http_port, ws_port, &config.server.bind, true);

    // === START EMBEDDED HTTP SERVER (axum) ===
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let app = Router::new()
        .route("/", get(serve_index))
        .route("/*path", get(serve_static))
        .layer(cors);

    let http_addr = format!("{}:{}", config.server.bind, http_port);
    let http_listener = tokio::net::TcpListener::bind(&http_addr).await?;
    
    let http_server = axum::serve(http_listener, app);

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
        eprintln!("  \x1b[1;33m[peace]\x1b[0m  Graceful shutdown initiated...");

        // Cleanup sessions
        let mut mgr = session_manager_shutdown.write().await;
        let session_count = mgr.session_count();
        eprintln!(
            "  \x1b[1;33m[cleanup]\x1b[0m Cleaning up {} sessions...",
            session_count
        );

        // Clean up all sessions
        let ids: Vec<String> = mgr.session_ids();
        for id in ids {
            mgr.remove_session(&id);
        }

        eprintln!("  \x1b[1;32m[done]\x1b[0m   Later! Stay chill.");
        eprintln!();
    };

    // Run all servers concurrently with shutdown handler
    tokio::select! {
        result = ws::serve_multi_async(session_manager, ws_port, None, Some(vfs_manager)) => {
            result?;
        }
        result = http_server => {
            if let Err(e) = result {
                eprintln!("  \x1b[1;31m[error]\x1b[0m  HTTP server error: {}", e);
            }
        }
        _ = shutdown_signal => {
            // Shutdown was triggered
        }
    }

    Ok(())
}
