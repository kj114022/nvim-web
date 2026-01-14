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
use nvim_web_host::auth;
use nvim_web_host::config::Config;
use nvim_web_host::embedded;
use nvim_web_host::native;
use nvim_web_host::session::AsyncSessionManager;
use nvim_web_host::transport::{serve_webtransport, WebTransportConfig};
use nvim_web_host::vfs::{BrowserFsBackend, FsRequestRegistry, LocalFs, VfsManager};
use nvim_web_host::ws;
use std::fs::File;
use std::io::BufReader;
use tokio::signal;
use tokio::sync::RwLock;
use tokio_rustls::rustls::ServerConfig;
use tokio_rustls::TlsAcceptor;
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
    eprintln!("  \x1b[1;32m[http]\x1b[0m   Server chillin' at port \x1b[1;96m{http_port}\x1b[0m");
    eprintln!("  \x1b[1;32m[ws]\x1b[0m     WebSocket vibin' at port \x1b[1;96m{ws_port}\x1b[0m");
    eprintln!();
    eprintln!("  \x1b[1;37m>\x1b[0m Open: \x1b[4;96mhttp://{bind}:{http_port}\x1b[0m");
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

/// Serve index.html at root with CSP
async fn serve_index() -> Response<Body> {
    let mut builder = Response::builder();
    builder = builder
        .header(header::CONTENT_SECURITY_POLICY, "default-src 'self'; script-src 'self' 'wasm-unsafe-eval' 'unsafe-inline' https://cdn.jsdelivr.net; worker-src 'self' blob:; style-src 'self' 'unsafe-inline' https://cdn.jsdelivr.net; font-src 'self' https://cdn.jsdelivr.net https://fonts.gstatic.com; connect-src 'self' ws: wss:; object-src 'self';")
        .header("Cross-Origin-Opener-Policy", "same-origin")
        .header("Cross-Origin-Embedder-Policy", "require-corp");

    match embedded::get_asset("index.html") {
        Some((data, mime)) => builder
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, mime)
            .body(Body::from(data))
            .unwrap(),
        None => builder
            .status(StatusCode::NOT_FOUND)
            .body(Body::from("index.html not found"))
            .unwrap(),
    }
}

/// Handle 'nvim-web open [PATH]' command
fn handle_open_command(args: &[String]) -> anyhow::Result<()> {
    use std::path::PathBuf;
    use std::time::Duration;

    use nvim_web_host::project::{ProjectConfig, TokenMode, TokenOptions};

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
    eprintln!("  \x1b[1;96m[open]\x1b[0m   Project: \x1b[1m{name}\x1b[0m");
    eprintln!(
        "  \x1b[1;96m[open]\x1b[0m   Path: {}",
        resolved_path.display()
    );

    if let Some(ref file) = final_target_file {
        let line_str = final_target_line.map_or(String::new(), |l| format!(":{l}"));
        eprintln!("  \x1b[1;96m[open]\x1b[0m   File: {file}{line_str}");
    }

    // Build token options
    let options = TokenOptions {
        target_file: final_target_file,
        target_line: final_target_line,
        mode: if share_mode {
            TokenMode::Share
        } else {
            TokenMode::SingleUse
        },
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
fn clone_github_repo(
    url: &str,
) -> anyhow::Result<(std::path::PathBuf, Option<String>, Option<u32>)> {
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
    let (target_file, target_line) =
        if parts.len() > 4 && (parts[2] == "blob" || parts[2] == "tree") {
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
            .args([
                "clone",
                "--depth",
                "1",
                &clone_url,
                repo_dir.to_str().unwrap(),
            ])
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

/// Load TLS configuration from certificates and key
fn load_tls_config(cert_path: &str, key_path: &str) -> anyhow::Result<Arc<ServerConfig>> {
    let certs = rustls_pemfile::certs(&mut BufReader::new(File::open(cert_path)?))
        .collect::<Result<Vec<_>, _>>()?;

    let key = rustls_pemfile::private_key(&mut BufReader::new(File::open(key_path)?))?
        .ok_or_else(|| anyhow::anyhow!("No private key found"))?;

    let config = ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, key)?;

    Ok(Arc::new(config))
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
    let mut config = Config::load();
    eprintln!(
        "  \x1b[1;32m[config]\x1b[0m Loaded from {}",
        Config::default_config_path().display()
    );

    // Override config with CLI arguments
    let args: Vec<String> = std::env::args().collect();
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--remote" => {
                if i + 1 < args.len() {
                    config.remote.enabled = true;
                    config.remote.address = args[i + 1].clone();
                    eprintln!(
                        "  \x1b[1;36m[remote]\x1b[0m Target: {}",
                        config.remote.address
                    );
                    i += 1;
                } else {
                    eprintln!("  \x1b[1;33m[warn]\x1b[0m   --remote requires an address argument");
                }
            }
            "--auth-token" => {
                if i + 1 < args.len() {
                    config.remote.auth_token = Some(args[i + 1].clone());
                    eprintln!("  \x1b[1;36m[auth]\x1b[0m   Token provided via CLI");
                    i += 1;
                } else {
                    eprintln!("  \x1b[1;33m[warn]\x1b[0m   --auth-token requires a token argument");
                }
            }
            "--auth-token-file" => {
                if i + 1 < args.len() {
                    config.remote.auth_token_file = Some(args[i + 1].clone());
                    eprintln!("  \x1b[1;36m[auth]\x1b[0m   Token file: {}", args[i + 1]);
                    i += 1;
                } else {
                    eprintln!(
                        "  \x1b[1;33m[warn]\x1b[0m   --auth-token-file requires a path argument"
                    );
                }
            }
            _ => {}
        }
        i += 1;
    }

    // Auto-generate auth token if remote is enabled but no token configured
    if config.remote.enabled
        && config.remote.auth_token.is_none()
        && config.remote.auth_token_file.is_none()
    {
        let token = auth::generate_secure_token();
        eprintln!("  \x1b[1;35m[auth]\x1b[0m   Auto-generated token: {token}");
        eprintln!("           (Use --auth-token to specify a fixed token)");
        config.remote.auth_token = Some(token);
    }

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
    vfs.register_backend("local", Box::new(LocalFs::new(&home_dir)))
        .await;

    // Setup BrowserFS backend (wired to WS)
    let fs_registry = Arc::new(FsRequestRegistry::new());
    let (fs_req_tx, _) = tokio::sync::broadcast::channel(1024);
    vfs.register_backend(
        "browser",
        Box::new(BrowserFsBackend::new(
            "default",
            fs_req_tx.clone(),
            fs_registry.clone(),
        )),
    )
    .await;

    // Setup GitHub backend (for vfs://github/owner/repo/path)
    vfs.register_backend(
        "github",
        Box::new(nvim_web_vfs::GitHubFsBackend::from_env().unwrap_or_default()),
    )
    .await;

    let vfs_manager = Arc::new(RwLock::new(vfs));
    eprintln!("  \x1b[1;32m[vfs]\x1b[0m    Backend: local (root: {home_dir}) + browser + github");

    // Create async session manager with VFS access
    let mut mgr = AsyncSessionManager::new(vfs_manager.clone());

    // Configure remote backend if enabled
    if config.remote.enabled {
        mgr.set_remote_address(config.remote.address.clone());

        // Resolve auth token (CLI > Inline > File)
        match auth::resolve_token(
            config.remote.auth_token.as_deref(),
            config.remote.auth_token_file.as_deref(),
        ) {
            Ok(token) => mgr.set_auth_token(token),
            Err(e) => {
                eprintln!("  \x1b[1;31m[error]\x1b[0m  Failed to resolve auth token: {e}");
                std::process::exit(1);
            }
        }
    }

    let session_manager = Arc::new(RwLock::new(mgr));
    let session_manager_shutdown = session_manager.clone();

    print_connection_info(http_port, ws_port, &config.server.bind, true);

    // === START EMBEDDED HTTP SERVER (axum) ===
    let tls_config =
        if let (Some(cert), Some(key)) = (&config.server.ssl_cert, &config.server.ssl_key) {
            eprintln!("  \x1b[1;36m[tls]\x1b[0m    Enabled (Cert: {})", cert);
            let config = match load_tls_config(cert, key) {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("  \x1b[1;31m[error]\x1b[0m  Failed to load TLS config: {e}");
                    std::process::exit(1);
                }
            };
            Some(config)
        } else {
            None
        };

    // === WEBTRANSPORT SERVER (requires TLS) ===
    let webtransport_config = if let (Some(cert), Some(key), Some(wt_port)) = (
        &config.server.ssl_cert,
        &config.server.ssl_key,
        config.server.webtransport_port,
    ) {
        eprintln!("  \x1b[1;36m[wt]\x1b[0m     WebTransport on port \x1b[1;96m{wt_port}\x1b[0m (QUIC/HTTP3)");
        Some(WebTransportConfig {
            port: wt_port,
            cert_path: cert.clone(),
            key_path: key.clone(),
        })
    } else if config.server.webtransport_port.is_some() {
        eprintln!("  \x1b[1;33m[warn]\x1b[0m   WebTransport requires TLS (ssl_cert/ssl_key)");
        None
    } else {
        None
    };

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

    // HTTP Server Future
    let http_tls_config = tls_config.clone();
    let http_server = async move {
        if let Some(tls_config) = http_tls_config {
            // HTTPS Mode (Manual Loop)
            let tls_acceptor = TlsAcceptor::from(tls_config);
            loop {
                // Accept TCP connection
                let (stream, _addr) = match http_listener.accept().await {
                    Ok(conn) => conn,
                    Err(e) => {
                        tracing::warn!("HTTP accept error: {}", e);
                        continue;
                    }
                };

                // Wrap in TLS and serve
                let tls_acceptor = tls_acceptor.clone();
                let app = app.clone();

                tokio::spawn(async move {
                    match tls_acceptor.accept(stream).await {
                        Ok(tls_stream) => {
                            let io = hyper_util::rt::TokioIo::new(tls_stream);

                            // Adapter service to convert hyper::Request<Incoming> to axum::Request<Body>
                            let service = hyper::service::service_fn(
                                move |req: hyper::Request<hyper::body::Incoming>| {
                                    let app = app.clone();
                                    async move {
                                        use tower::ServiceExt; // for oneshot
                                        let (parts, body) = req.into_parts();
                                        let req = http::Request::from_parts(parts, Body::new(body));
                                        app.oneshot(req).await
                                    }
                                },
                            );

                            if let Err(err) = hyper_util::server::conn::auto::Builder::new(
                                hyper_util::rt::TokioExecutor::new(),
                            )
                            .serve_connection(io, service)
                            .await
                            {
                                tracing::warn!("HTTPS connection error: {}", err);
                            }
                        }
                        Err(e) => {
                            tracing::debug!("TLS handshake failed: {}", e);
                        }
                    }
                });
            }
        } else {
            // HTTP Mode (Standard Axum)
            if let Err(e) = axum::serve(http_listener, app).await {
                tracing::error!("HTTP server error: {}", e);
            }
        }
    };

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
        eprintln!("  \x1b[1;33m[cleanup]\x1b[0m Cleaning up {session_count} sessions...");

        // Trigger graceful shutdown (auto-save) for all sessions
        mgr.shutdown_all().await;

        // Clean up all sessions (remove from map)
        let ids: Vec<String> = mgr.session_ids();
        for id in ids {
            mgr.remove_session(&id);
        }
        drop(mgr); // Explicit drop to satisfy clippy significant_drop_tightening

        eprintln!("  \x1b[1;32m[done]\x1b[0m   Later! Stay chill.");
        eprintln!();
    };

    // Run all servers concurrently with shutdown handler
    // Clone session_manager for WebTransport
    let wt_session_manager = session_manager.clone();

    tokio::select! {
        result = ws::serve_multi_async(session_manager, ws_port, Some(fs_registry), Some(vfs_manager), Some(fs_req_tx), tls_config) => {
            result?;
        }
        _ = http_server => {
             // HTTP server finished (should typically loop forever)
        }
        // WebTransport server (if configured)
        _ = async {
            if let Some(wt_config) = webtransport_config {
                if let Err(e) = serve_webtransport(wt_session_manager, wt_config, None, None).await {
                    eprintln!("  \x1b[1;31m[error]\x1b[0m  WebTransport server error: {e}");
                }
            } else {
                // No WebTransport configured, just wait forever
                std::future::pending::<()>().await;
            }
        } => {}
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
