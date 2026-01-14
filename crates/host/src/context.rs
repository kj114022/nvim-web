use regex::Regex;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TakeoverMode {
    Always,
    Once,
    Empty,
    NonEmpty,
    Never,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SiteConfig {
    pub filetype: String,
    pub takeover: TakeoverMode,
    pub cmdline: String, // "neovim" | "firenvim" | "none"
}

impl Default for SiteConfig {
    fn default() -> Self {
        Self {
            filetype: "text".to_string(),
            takeover: TakeoverMode::Always,
            cmdline: "firenvim".to_string(),
        }
    }
}

pub struct ContextManager {
    rules: Vec<(Regex, SiteConfig)>,
}

impl Default for ContextManager {
    fn default() -> Self {
        Self::new()
    }
}

impl ContextManager {
    pub fn new() -> Self {
        let mut rules = Vec::new();

        // Default: GitHub -> Markdown
        // Firenvim uses .* for defaults, and specific regexes for overrides.
        // We hardcode some defaults here for the "Best Nvim Ever" experience.
        if let Ok(re) = Regex::new(r"github\.com.*") {
            rules.push((
                re,
                SiteConfig {
                    filetype: "markdown".to_string(),
                    takeover: TakeoverMode::Always,
                    cmdline: "neovim".to_string(),
                },
            ));
        }

        if let Ok(re) = Regex::new(r"gmail\.com.*") {
            rules.push((
                re,
                SiteConfig {
                    filetype: "text".to_string(),  // Plain text for emails
                    takeover: TakeoverMode::Empty, // Only if empty body?
                    cmdline: "firenvim".to_string(),
                },
            ));
        }

        Self { rules }
    }

    pub fn get_config(&self, url: &str) -> SiteConfig {
        for (re, config) in &self.rules {
            if re.is_match(url) {
                return config.clone();
            }
        }
        SiteConfig::default()
    }
}
