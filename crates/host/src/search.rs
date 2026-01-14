//! Host-side search module
//!
//! Provides fast file search using ripgrep-style matching.
//! This moves search operations from WASM to the host for performance.

use anyhow::Result;
use regex::Regex;
use std::path::Path;

/// Search result with line number and content
#[derive(Debug, Clone)]
pub struct SearchMatch {
    pub line_number: usize,
    pub line_content: String,
    pub match_start: usize,
    pub match_end: usize,
}

/// Search options
#[derive(Debug, Clone)]
pub struct SearchOptions {
    pub case_insensitive: bool,
    pub regex: bool,
    pub max_results: usize,
}

impl Default for SearchOptions {
    fn default() -> Self {
        Self {
            case_insensitive: true,
            regex: false,
            max_results: 100,
        }
    }
}

/// Search file content for pattern
///
/// Returns matching lines with line numbers and match offsets.
/// This is faster than Neovim's built-in search for large files.
pub fn search_content(
    content: &str,
    pattern: &str,
    options: &SearchOptions,
) -> Result<Vec<SearchMatch>> {
    let mut results = Vec::new();

    // Build regex
    let regex_pattern = if options.regex {
        pattern.to_string()
    } else {
        regex::escape(pattern)
    };

    let regex_pattern = if options.case_insensitive {
        format!("(?i){}", regex_pattern)
    } else {
        regex_pattern
    };

    let re = Regex::new(&regex_pattern)?;

    for (line_idx, line) in content.lines().enumerate() {
        if let Some(m) = re.find(line) {
            results.push(SearchMatch {
                line_number: line_idx + 1,
                line_content: line.to_string(),
                match_start: m.start(),
                match_end: m.end(),
            });

            if results.len() >= options.max_results {
                break;
            }
        }
    }

    Ok(results)
}

/// Search file by path
pub async fn search_file(
    path: &Path,
    pattern: &str,
    options: &SearchOptions,
) -> Result<Vec<SearchMatch>> {
    let content = tokio::fs::read_to_string(path).await?;
    search_content(&content, pattern, options)
}

/// Search all files in directory (non-recursive for now)
pub async fn search_directory(
    dir: &Path,
    pattern: &str,
    options: &SearchOptions,
) -> Result<Vec<(String, Vec<SearchMatch>)>> {
    let mut results = Vec::new();
    let mut entries = tokio::fs::read_dir(dir).await?;

    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();
        if path.is_file() {
            match search_file(&path, pattern, options).await {
                Ok(matches) if !matches.is_empty() => {
                    results.push((path.display().to_string(), matches));
                }
                _ => {}
            }
        }
    }

    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_search_content() {
        let content = "hello world\nHELLO WORLD\nfoo bar";
        let results = search_content(content, "hello", &SearchOptions::default()).unwrap();

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].line_number, 1);
        assert_eq!(results[1].line_number, 2);
    }

    #[test]
    fn test_search_case_sensitive() {
        let content = "hello world\nHELLO WORLD";
        let options = SearchOptions {
            case_insensitive: false,
            ..Default::default()
        };
        let results = search_content(content, "hello", &options).unwrap();

        assert_eq!(results.len(), 1);
    }
}
