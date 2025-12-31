use std::net::TcpListener;
use std::sync::Arc;

use axum::{
    body::Body,
    extract::{Path, State},
    http::{header, Response, StatusCode},
    routing::get,
    Router,
};
use nvim_web_host::api;
use nvim_web_host::config::Config;
use nvim_web_host::embedded;
use nvim_web_host::native;
use nvim_web_host::session::AsyncSessionManager;
use nvim_web_host::vfs::{LocalFs, VfsManager};
use nvim_web_host::ws;
use tokio::signal;
use tokio::sync::RwLock;
use tower_http::cors::{Any, CorsLayer};

const VERSION: &str = env!("CARGO_PKG_VERSION");

fn print_banner() {
    // Banner - cyan vibes, chill energy
    eprintln!();
    eprintln!(
        "  \x1b[1;36m╔═══════════════════════════════════════════════════════════════════╗\x1b[0m"
    );
    eprintln!(
        "  \x1b[1;36m║                                                                   ║\x1b[0m"
    );
    eprintln!("  \x1b[1;36m║\x1b[0m  \x1b[1;96m███╗   ██╗██╗   ██╗██╗███╗   ███╗    ██╗    ██╗███████╗██████╗ \x1b[1;36m║\x1b[0m");
    eprintln!("  \x1b[1;36m║\x1b[0m  \x1b[1;96m████╗  ██║██║   ██║██║████╗ ████║    ██║    ██║██╔════╝██╔══██╗\x1b[1;36m║\x1b[0m");
    eprintln!("  \x1b[1;36m║\x1b[0m  \x1b[1;96m██╔██╗ ██║██║   ██║██║██╔████╔██║    ██║ █╗ ██║█████╗  ██████╔╝\x1b[1;36m║\x1b[0m");
    eprintln!("  \x1b[1;36m║\x1b[0m  \x1b[1;96m██║╚██╗██║╚██╗ ██╔╝██║██║╚██╔╝██║    ██║███╗██║██╔══╝  ██╔══██╗\x1b[1;36m║\x1b[0m");
    eprintln!("  \x1b[1;36m║\x1b[0m  \x1b[1;96m██║ ╚████║ ╚████╔╝ ██║██║ ╚═╝ ██║    ╚███╔███╔╝███████╗██████╔╝\x1b[1;36m║\x1b[0m");
    eprintln!("  \x1b[1;36m║\x1b[0m  \x1b[1;96m╚═╝  ╚═══╝  ╚═══╝  ╚═╝╚═╝     ╚═╝     ╚══╝╚══╝ ╚══════╝╚═════╝ \x1b[1;36m║\x1b[0m");
    eprintln!(
        "  \x1b[1;36m║                                                                   ║\x1b[0m"
    );
    eprintln!("  \x1b[1;36m║\x1b[0m  \x1b[2;37mNeovim in your browser. No cap.\x1b[0m                                 \x1b[1;36m║\x1b[0m");
    eprintln!("  \x1b[1;36m║\x1b[0m  \x1b[2;35mBuilt different. Stay chill. Edit code.\x1b[0m v{VERSION:<21}\x1b[1;36m║\x1b[0m");
    eprintln!(
        "  \x1b[1;36m║                                                                   ║\x1b[0m"
    );
    eprintln!(
        "  \x1b[1;36m╚═══════════════════════════════════════════════════════════════════╝\x1b[0m"
    );
    eprintln!();
}

fn print_connection_info(http_port: u16, ws_port: u16, bind: &str, embedded: bool) {
    if embedded {
        eprintln!("  \x1b[1;32m[vibin]\x1b[0m  Single binary mode - all assets embedded");
    }
    eprintln!(
        "  \x1b[1;32m[http]\x1b[0m   Server chillin' at port \x1b[1;96m{http_port}\x1b[0m"
    );
    eprintln!(
        "  \x1b[1;32m[ws]\x1b[0m     WebSocket vibin' at port \x1b[1;96m{ws_port}\x1b[0m"
    );
    eprintln!();
    eprintln!(
        "  \x1b[1;37m>\x1b[0m Open: \x1b[4;96mhttp://{bind}:{http_port}\x1b[0m"
    );
    eprintln!();
    eprintln!("  \x1b[2mPress Ctrl+C to bounce\x1b[0m");
    eprintln!();
}

/// Graceful start: Check if port is available
fn check_port_available(bind: &str, port: u16) -> bool {
    TcpListener::bind(format!("{bind}:{port}")).is_ok()
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
            eprintln!("  \x1b[1;32m[check]\x1b[0m  Neovim found: {first_line}");
        }
        _ => {
            return Err("Neovim not found. Please install Neovim first.".to_string());
        }
    }
    Ok(())
}

/// Serve embedded static file
async fn serve_static(Path(path): Path<String>) -> Response<Body> {
    let path = if path.is_empty() {
        "index.html".to_string()
    } else {
        path
    };

    match embedded::get_asset(&path) {
        Some((data, mime)) => {
            // Use application/javascript for .js files (override detected mime)
            let content_type = if std::path::Path::new(&path)
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("js"))
            {
                "application/javascript"
            } else {
                mime
            };

            Response::builder()
                .status(StatusCode::OK)
                .header(header::CONTENT_TYPE, content_type)
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

/// Serve /config.js with dynamic WS port
async fn serve_config_js(State(state): State<api::AppState>) -> Response<Body> {
    let js = format!("window.NVIM_CONFIG = {{ wsPort: {} }};", state.ws_port);
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/javascript")
        .body(Body::from(js))
        .unwrap()
}

/// Serve index.html at root
async fn serve_index() -> Response<Body> {
    match embedded::get_asset("index.html") {
        Some((data, mime)) => Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, mime)
            .body(Body::from(data))
            .unwrap(),
        None => Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Body::from("index.html not found"))
            .unwrap(),
    }
}

/// Handle 'nvim-web open [PATH]' command
fn handle_open_command(args: &[String]) -> anyhow::Result<()> {
    use std::path::PathBuf;
    use std::time::Duration;

    use nvim_web_host::project::{ProjectConfig, TokenOptions, TokenMode};

    // Parse arguments
    let mut path_arg = ".".to_string();
    let mut target_file: Option<String> = None;
    let mut target_line: Option<u32> = None;
    let mut show_qr = false;
    let mut share_mode = false;
    let mut share_duration: Option<Duration> = None;

    let mut i = 2;
    while i < args.len() {
        match args[i].as_str() {
            "--file" | "-f" if i + 1 < args.len() => {
                target_file = Some(args[i + 1].clone());
                i += 2;
            }
            "--line" | "-l" if i + 1 < args.len() => {
                target_line = args[i + 1].parse().ok();
                i += 2;
            }
            "--qr" => {
                show_qr = true;
                i += 1;
            }
            "--share" => {
                share_mode = true;
                i += 1;
            }
            "--duration" if i + 1 < args.len() => {
                // Parse duration like "1h", "30m", "1d"
                let dur_str = &args[i + 1];
                share_duration = parse_duration(dur_str);
                i += 2;
            }
            arg if !arg.starts_with('-') => {
                // Check for file:line format (e.g., src/main.rs:42)
                if let Some((file_part, line_part)) = arg.rsplit_once(':') {
                    if let Ok(line) = line_part.parse::<u32>() {
                        // This is a file:line reference
                        if std::path::Path::new(file_part).exists() {
                            // It's an existing file with line number
                            path_arg = std::path::Path::new(file_part)
                                .parent()
                                .map_or(".".to_string(), |p| p.to_string_lossy().to_string());
                            target_file = Some(file_part.to_string());
                            target_line = Some(line);
                        } else {
                            path_arg = arg.to_string();
                        }
                    } else {
                        path_arg = arg.to_string();
                    }
                } else {
                    path_arg = arg.to_string();
                }
                i += 1;
            }
            _ => i += 1,
        }
    }

    // Check if path_arg is a GitHub URL
    let (resolved_path, github_target_file, github_target_line) = if is_github_url(&path_arg) {
        match clone_github_repo(&path_arg) {
            Ok((clone_path, file, line)) => {
                eprintln!("  \x1b[1;35m[github]\x1b[0m Cloned repository");
                (clone_path, file, line)
            }
            Err(e) => {
                return Err(anyhow::anyhow!("Failed to clone GitHub repo: {e}"));
            }
        }
    } else {
        // Resolve to absolute path
        let path = if path_arg.starts_with('~') {
            let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
            PathBuf::from(path_arg.replacen('~', &home, 1))
        } else {
            PathBuf::from(&path_arg)
        };

        let abs_path = std::fs::canonicalize(&path)
            .map_err(|e| anyhow::anyhow!("Path '{}' not found: {}", path.display(), e))?;
        (abs_path, None, None)
    };

    // Merge GitHub-detected file/line with CLI-specified ones (CLI takes priority)
    let final_target_file = target_file.or(github_target_file);
    let final_target_line = target_line.or(github_target_line);

    // Load project config
    let config = ProjectConfig::load(&resolved_path);
    let name = config.display_name(&resolved_path);

    eprintln!();
    eprintln!(
        "  \x1b[1;96m[open]\x1b[0m   Project: \x1b[1m{name}\x1b[0m"
    );
    eprintln!("  \x1b[1;96m[open]\x1b[0m   Path: {}", resolved_path.display());
    
    if let Some(ref file) = final_target_file {
        let line_str = final_target_line.map_or(String::new(), |l| format!(":{l}"));
        eprintln!("  \x1b[1;96m[open]\x1b[0m   File: {file}{line_str}");
    }

    // Build token options
    let options = TokenOptions {
        target_file: final_target_file,
        target_line: final_target_line,
        mode: if share_mode { TokenMode::Share } else { TokenMode::SingleUse },
        duration: share_duration,
        max_claims: if share_mode { Some(100) } else { None },
    };

    // Generate token
    let token = nvim_web_host::project::store_token_with_options(resolved_path, config, options);

    // Build URL
    let url = format!("http://localhost:8080/?open={token}");
    eprintln!("  \x1b[1;96m[open]\x1b[0m   URL: {url}");
    
    if share_mode {
        eprintln!("  \x1b[1;35m[share]\x1b[0m  Shareable link (up to 100 uses)");
        if let Some(dur) = share_duration {
            eprintln!("  \x1b[1;35m[share]\x1b[0m  Expires in: {:?}", dur);
        }
    }
    eprintln!();

    // Show QR code if requested
    if show_qr {
        print_qr_code(&url);
        eprintln!();
    }

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
        let _ = std::process::Command::new("cmd")
            .args(["/C", "start", &url])
            .spawn();
    }

    // Note: The server needs to be running for this to work
    eprintln!("  \x1b[2m(Make sure nvim-web server is running in another terminal)\x1b[0m");
    eprintln!();

    Ok(())
}

/// Check if argument is a GitHub URL
fn is_github_url(s: &str) -> bool {
    s.starts_with("github.com/")
        || s.starts_with("https://github.com/")
        || s.starts_with("http://github.com/")
}

/// Clone a GitHub repository and return (path, optional_file, optional_line)
fn clone_github_repo(url: &str) -> anyhow::Result<(std::path::PathBuf, Option<String>, Option<u32>)> {
    // Normalize URL: remove protocol prefix
    let normalized = url
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .trim_start_matches("github.com/");

    // Parse: owner/repo[/blob/branch/path] or owner/repo[/tree/branch/path]
    let parts: Vec<&str> = normalized.split('/').collect();
    if parts.len() < 2 {
        return Err(anyhow::anyhow!("Invalid GitHub URL format"));
    }

    let owner = parts[0];
    let repo = parts[1];
    let clone_url = format!("https://github.com/{owner}/{repo}.git");

    // Parse optional path and line from URL
    let (target_file, target_line) = if parts.len() > 4 && (parts[2] == "blob" || parts[2] == "tree") {
        // Format: owner/repo/blob/branch/path/to/file
        let branch = parts[3];
        let file_path = parts[4..].join("/");
        
        // Check for line number fragment (e.g., #L42 or #L42-L50)
        let (clean_path, line) = if let Some((path, fragment)) = file_path.rsplit_once('#') {
            let line_num = fragment
                .trim_start_matches('L')
                .split('-')
                .next()
                .and_then(|s| s.parse().ok());
            (path.to_string(), line_num)
        } else {
            (file_path, None)
        };
        
        eprintln!("  \x1b[1;35m[github]\x1b[0m Repo: {owner}/{repo} (branch: {branch})");
        if !clean_path.is_empty() {
            eprintln!("  \x1b[1;35m[github]\x1b[0m File: {clean_path}");
        }
        
        (Some(clean_path).filter(|s| !s.is_empty()), line)
    } else {
        eprintln!("  \x1b[1;35m[github]\x1b[0m Repo: {owner}/{repo}");
        (None, None)
    };

    // Clone to temp directory
    let temp_dir = std::env::temp_dir().join("nvim-web-github");
    let repo_dir = temp_dir.join(format!("{owner}-{repo}"));

    // If directory exists and is a git repo, do a pull instead
    if repo_dir.exists() && repo_dir.join(".git").exists() {
        eprintln!("  \x1b[1;35m[github]\x1b[0m Updating existing clone...");
        let _ = std::process::Command::new("git")
            .args(["pull", "--ff-only"])
            .current_dir(&repo_dir)
            .output();
    } else {
        // Remove existing directory if it's not a valid git repo
        if repo_dir.exists() {
            let _ = std::fs::remove_dir_all(&repo_dir);
        }
        
        eprintln!("  \x1b[1;35m[github]\x1b[0m Cloning {clone_url}...");
        let output = std::process::Command::new("git")
            .args(["clone", "--depth", "1", &clone_url, repo_dir.to_str().unwrap()])
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow::anyhow!("git clone failed: {stderr}"));
        }
    }

    Ok((repo_dir, target_file, target_line))
}

/// Parse duration string like "1h", "30m", "1d"
fn parse_duration(s: &str) -> Option<std::time::Duration> {
    use std::time::Duration;
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    
    let (num_str, unit) = s.split_at(s.len() - 1);
    let num: u64 = num_str.parse().ok()?;
    
    match unit {
        "s" => Some(Duration::from_secs(num)),
        "m" => Some(Duration::from_secs(num * 60)),
        "h" => Some(Duration::from_secs(num * 3600)),
        "d" => Some(Duration::from_secs(num * 86400)),
        _ => None,
    }
}

/// Print QR code to terminal using qr2term
fn print_qr_code(url: &str) {
    eprintln!("  \x1b[1;37m[QR Code]\x1b[0m");
    eprintln!();
    
    // Generate real QR code using qr2term
    match qr2term::generate_qr_string(url) {
        Ok(qr_string) => {
            // Indent each line for consistent formatting
            for line in qr_string.lines() {
                eprintln!("    {line}");
            }
        }
        Err(e) => {
            // Fallback: show URL if QR generation fails
            eprintln!("    \x1b[31m[QR generation failed: {e}]\x1b[0m");
            eprintln!();
            eprintln!("    URL: {url}");
        }
    }
    
    eprintln!();
    let short_url = if url.len() > 60 {
        format!("{}...", &url[..57])
    } else {
        url.to_string()
    };
    eprintln!("    \x1b[2mScan or copy: {short_url}\x1b[0m");
}

#[tokio::main]
#[allow(clippy::too_many_lines)]
async fn main() -> anyhow::Result<()> {
    // Initialize structured logging (tracing)
    tracing_subscriber::fmt()
        .with_target(false)
        .with_level(true)
        .init();

    // Handle --version and --help
    let args: Vec<String> = std::env::args().collect();
    if args.len() > 1 {
        match args[1].as_str() {
            "--version" | "-v" => {
                println!("nvim-web {VERSION}");
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
                println!("OPEN OPTIONS:");
                println!("    PATH             Path to open (supports file:line syntax, e.g., src/main.rs:42)");
                println!("    -f, --file FILE  Specific file to open");
                println!("    -l, --line LINE  Line number to jump to");
                println!("    --qr             Display QR code for mobile access");
                println!("    --share          Create shareable link (multi-use)");
                println!("    --duration DUR   Link expiration (e.g., 1h, 30m, 1d)");
                println!();
                println!("GLOBAL OPTIONS:");
                println!("    -h, --help       Print help information");
                println!("    -v, --version    Print version");
                println!();
                println!("CONFIG:");
                println!("    ~/.config/nvim-web/config.toml");
                println!();
                println!("EXAMPLES:");
                println!("    nvim-web                        Start server");
                println!("    nvim-web open .                 Open current directory");
                println!("    nvim-web open ~/code            Open ~/code directory");
                println!("    nvim-web open src/main.rs:100   Open file at line 100");
                println!("    nvim-web open . --qr            Open with QR code for mobile");
                println!("    nvim-web open . --share --duration 1h");
                println!("                                    Create 1-hour shareable link");
                println!();
                println!("    nvim-web open github.com/neovim/neovim");
                println!("                                    Clone and open GitHub repo");
                println!("    nvim-web open github.com/owner/repo/blob/main/src/lib.rs#L42");
                println!("                                    Open GitHub file at line");
                return Ok(());
            }
            "open" => {
                // Magic link: Open a project in the browser
                return handle_open_command(&args);
            }
            #[allow(clippy::match_same_arms)]
            "serve" | "--native" => {}
            _ => {}
        }
    }

    let is_native = args.iter().any(|a| a == "--native");
    if is_native {
        // In native mode, we don't print banner to stderr to keep logs clean
        eprintln!("nvim-web starting in native mode...");
    } else {
        print_banner();
    }

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
        eprintln!("  \x1b[1;31m[error]\x1b[0m  {e}");
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
        if let Some(p) = find_available_port(&config.server.bind, config.server.http_port + 1) {
            eprintln!("  \x1b[1;32m[check]\x1b[0m  Using HTTP port {p}");
            p
        } else {
            eprintln!(
                "  \x1b[1;31m[error]\x1b[0m  No available HTTP ports in range {}-{}",
                config.server.http_port,
                config.server.http_port + 10
            );
            std::process::exit(1);
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
        if let Some(p) = find_available_port(&config.server.bind, config.server.ws_port + 1) {
            eprintln!("  \x1b[1;32m[check]\x1b[0m  Using WS port {p}");
            p
        } else {
            eprintln!(
                "  \x1b[1;31m[error]\x1b[0m  No available WS ports in range {}-{}",
                config.server.ws_port,
                config.server.ws_port + 10
            );
            std::process::exit(1);
        }
    };

    // Create VFS manager with local filesystem backend
    let vfs = VfsManager::new();
    let home_dir = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    vfs.register_backend("local", Box::new(LocalFs::new(&home_dir))).await;
    let vfs_manager = Arc::new(RwLock::new(vfs));
    eprintln!(
        "  \x1b[1;32m[vfs]\x1b[0m    Backend: local (root: {home_dir})"
    );

    // Create async session manager with VFS access
    let session_manager = Arc::new(RwLock::new(AsyncSessionManager::new(vfs_manager.clone())));
    let session_manager_shutdown = session_manager.clone();

    print_connection_info(http_port, ws_port, &config.server.bind, true);

    // === START EMBEDDED HTTP SERVER (axum) ===
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let app_state = api::AppState {
        session_manager: session_manager.clone(),
        ws_port,
    };

    let app = Router::new()
        .route("/", get(serve_index))
        .route("/config.js", get(serve_config_js)) // Serve dynamic config
        .route("/*path", get(serve_static))
        .nest("/api", api::api_router())
        .with_state(app_state) // Verify this usage?
        .layer(cors);

    let http_addr = format!("{}:{}", config.server.bind, http_port);
    let http_listener = tokio::net::TcpListener::bind(&http_addr).await?;

    let http_server = axum::serve(http_listener, app);

    // === GRACEFUL SHUTDOWN HANDLER ===
    let (native_shutdown_tx, native_shutdown_rx) = tokio::sync::oneshot::channel::<()>();
    
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
        
        // Wait for Native Messaging channel to close (Browser quit)
        let native_exit = async {
            if is_native {
                let _ = native_shutdown_rx.await;
                eprintln!("  \x1b[1;33m[native]\x1b[0m Native host disconnected.");
            } else {
                std::future::pending::<()>().await;
            }
        };

        tokio::select! {
            () = ctrl_c => {},
            () = terminate => {},
            () = native_exit => {},
        }

        eprintln!();
        eprintln!("  \x1b[1;33m[peace]\x1b[0m  Graceful shutdown initiated...");

        // Cleanup sessions
        let mut mgr = session_manager_shutdown.write().await;
        let session_count = mgr.session_count();
        eprintln!(
            "  \x1b[1;33m[cleanup]\x1b[0m Cleaning up {session_count} sessions..."
        );

        // Clean up all sessions
        let ids: Vec<String> = mgr.session_ids();
        for id in ids {
            mgr.remove_session(&id);
        }
        drop(mgr); // Explicit drop to satisfy clippy significant_drop_tightening

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
                eprintln!("  \x1b[1;31m[error]\x1b[0m  HTTP server error: {e}");
            }
        }
        () = async {
            if is_native {
                if let Err(e) = native::run(native_shutdown_tx) {
                     eprintln!("  \x1b[1;31m[error]\x1b[0m  Native loop error: {e}");
                }
            } else {
                std::future::pending::<()>().await;
            }
        } => {}
        () = shutdown_signal => {
            // Shutdown was triggered
        }
    }

    Ok(())
}
