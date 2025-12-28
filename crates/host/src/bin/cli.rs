//! nvim-web CLI
//!
//! Simple command-line interface for opening projects in nvim-web browser.
//!
//! Usage:
//!   nvim-web open [path]    Open a project in browser
//!   nvim-web --help         Show help

use std::env;
use std::path::PathBuf;
use std::process::Command;

const VERSION: &str = env!("CARGO_PKG_VERSION");
const DEFAULT_HOST: &str = "http://127.0.0.1:8080";

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        print_usage();
        return;
    }

    match args[1].as_str() {
        "open" => {
            let path = if args.len() > 2 {
                PathBuf::from(&args[2])
            } else {
                env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
            };
            open_project(path);
        }
        "--help" | "-h" | "help" => print_usage(),
        "--version" | "-v" => println!("nvim-web {}", VERSION),
        _ => {
            eprintln!("Unknown command: {}", args[1]);
            print_usage();
        }
    }
}

fn print_usage() {
    eprintln!();
    eprintln!("  \x1b[1;96mnvim-web\x1b[0m - Open projects in browser-based Neovim");
    eprintln!();
    eprintln!("  \x1b[1mUSAGE:\x1b[0m");
    eprintln!("    nvim-web open [path]    Open project in browser (default: current dir)");
    eprintln!("    nvim-web --help         Show this help");
    eprintln!("    nvim-web --version      Show version");
    eprintln!();
    eprintln!("  \x1b[1mEXAMPLES:\x1b[0m");
    eprintln!("    nvim-web open           # Open current directory");
    eprintln!("    nvim-web open ~/code    # Open specific path");
    eprintln!();
}

fn open_project(path: PathBuf) {
    // Resolve to absolute path
    let abs_path = match path.canonicalize() {
        Ok(p) => p,
        Err(e) => {
            eprintln!(
                "\x1b[1;31m[error]\x1b[0m Path not found: {} ({})",
                path.display(),
                e
            );
            return;
        }
    };

    eprintln!();
    eprintln!("  \x1b[1;96mnvim-web\x1b[0m opening project...");
    eprintln!("  \x1b[2mPath:\x1b[0m {}", abs_path.display());

    // Call the API to create a token
    let client = reqwest::blocking::Client::new();
    let api_url = format!("{}/api/open", DEFAULT_HOST);

    let response = match client
        .post(&api_url)
        .json(&serde_json::json!({ "path": abs_path.display().to_string() }))
        .send()
    {
        Ok(r) => r,
        Err(e) => {
            eprintln!();
            eprintln!("  \x1b[1;31m[error]\x1b[0m Could not connect to nvim-web host");
            eprintln!("  \x1b[2mIs nvim-web-host running?\x1b[0m");
            eprintln!();
            eprintln!("  Start it with: \x1b[1mnvim-web-host\x1b[0m");
            eprintln!();
            eprintln!("  \x1b[2mDetails: {}\x1b[0m", e);
            return;
        }
    };

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().unwrap_or_default();
        eprintln!();
        eprintln!(
            "  \x1b[1;31m[error]\x1b[0m API error: {} - {}",
            status, body
        );
        return;
    }

    // Parse response
    let data: serde_json::Value = match response.json() {
        Ok(d) => d,
        Err(e) => {
            eprintln!("  \x1b[1;31m[error]\x1b[0m Invalid API response: {}", e);
            return;
        }
    };

    let token = data["token"].as_str().unwrap_or("");
    let name = data["name"].as_str().unwrap_or("project");
    let url = data["url"].as_str().unwrap_or("");

    if token.is_empty() || url.is_empty() {
        eprintln!("  \x1b[1;31m[error]\x1b[0m Invalid response from API");
        return;
    }

    eprintln!("  \x1b[2mProject:\x1b[0m {}", name);
    eprintln!();
    eprintln!("  \x1b[1;32m[success]\x1b[0m Opening in browser...");
    eprintln!("  \x1b[4;96m{}\x1b[0m", url);
    eprintln!();

    // Open browser
    open_browser(url);
}

fn open_browser(url: &str) {
    #[cfg(target_os = "macos")]
    {
        let _ = Command::new("open").arg(url).spawn();
    }

    #[cfg(target_os = "linux")]
    {
        let _ = Command::new("xdg-open").arg(url).spawn();
    }

    #[cfg(target_os = "windows")]
    {
        let _ = Command::new("cmd").args(["/C", "start", url]).spawn();
    }
}
