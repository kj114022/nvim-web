use std::sync::Arc;
use tokio::sync::RwLock;
use nvim_web_host::session::AsyncSessionManager;
use nvim_web_host::ws;

const VERSION: &str = env!("CARGO_PKG_VERSION");

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

fn print_connection_info() {
    eprintln!("  \x1b[1;32m[ready]\x1b[0m WebSocket server listening");
    eprintln!();
    eprintln!("  \x1b[1mConnect:\x1b[0m  \x1b[4;34mhttp://localhost:8080\x1b[0m");
    eprintln!("  \x1b[1mServer:\x1b[0m   \x1b[2mws://127.0.0.1:9001\x1b[0m");
    eprintln!();
    eprintln!("  \x1b[2mServe UI:\x1b[0m cd ui && python3 -m http.server 8080");
    eprintln!();
    eprintln!("  \x1b[2mPress Ctrl+C to stop\x1b[0m");
    eprintln!();
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
    
    // Create async session manager
    let session_manager = Arc::new(RwLock::new(AsyncSessionManager::new()));
    
    print_connection_info();
    
    // Start async WebSocket server (this blocks)
    ws::serve_multi_async(session_manager).await?;
    
    Ok(())
}
