use std::process::{Command, Stdio, Child};
use std::path::{Path, PathBuf};
use std::io::Write;
use tempfile::TempDir;
use anyhow::{Result, Context};

/// Test harness for integration tests with real Neovim
pub struct TestHarness {
    pub nvim: Child,
    pub tmp_dir: TempDir,
    stdin_writer: Option<Box<dyn Write + Send>>,
}

impl TestHarness {
    /// Create new test harness with headless Neovim
    pub fn new() -> Result<Self> {
        let tmp_dir = TempDir::new().context("Failed to create temp directory")?;
        
        // Spawn Neovim in headless mode
        let mut nvim = Command::new("nvim")
            .arg("--headless")
            .arg("--clean")  // Don't load user config
            .arg("--noplugin")  // Don't load plugins
            .arg("-n")  // No swapfile
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .context("Failed to spawn Neovim - is it installed?")?;
        
        let stdin_writer = nvim.stdin.take().map(|s| Box::new(s) as Box<dyn Write + Send>);
        
        eprintln!("Neovim spawned with PID: {}", nvim.id());
        
        Ok(TestHarness {
            nvim,
            tmp_dir,
            stdin_writer,
        })
    }
    
    /// Get absolute path for file in temp directory
    pub fn path(&self, rel_path: &str) -> PathBuf {
        self.tmp_dir.path().join(rel_path)
    }
    
    /// Write a file to the temp directory
    pub fn write_file(&self, rel_path: &str, content: &str) -> Result<()> {
        let path = self.path(rel_path);
        
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        
        std::fs::write(&path, content)
            .with_context(|| format!("Failed to write file: {}", path.display()))?;
        
        Ok(())
    }
    
    /// Read a file from the temp directory
    pub fn read_file(&self, rel_path: &str) -> Result<String> {
        let path = self.path(rel_path);
        std::fs::read_to_string(&path)
            .with_context(|| format!("Failed to read file: {}", path.display()))
    }
    
    /// Make file read-only
    pub fn make_readonly(&self, rel_path: &str) -> Result<()> {
        let path = self.path(rel_path);
        let mut perms = std::fs::metadata(&path)?.permissions();
        perms.set_readonly(true);
        std::fs::set_permissions(&path, perms)?;
        Ok(())
    }
    
    /// Execute Vim command via stdin
    /// Note: This is simplified - real implementation would use RPC
    pub fn exec_vim(&mut self, cmd: &str) -> Result<()> {
        if let Some(ref mut stdin) = self.stdin_writer {
            // Send command via stdin (simplified)
            // In production, this would use proper RPC
            writeln!(stdin, "{}", cmd)?;
            stdin.flush()?;
        }
        Ok(())
    }
}

impl Drop for TestHarness {
    fn drop(&mut self) {
        // Clean shutdown
        let _ = self.nvim.kill();
        let _ = self.nvim.wait();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_harness_creates_temp_dir() {
        let harness = TestHarness::new().expect("Failed to create harness");
        assert!(harness.tmp_dir.path().exists());
    }
    
    #[test]
    fn test_harness_writes_files() {
        let harness = TestHarness::new().unwrap();
        harness.write_file("test.txt", "hello").unwrap();
        
        let content = harness.read_file("test.txt").unwrap();
        assert_eq!(content, "hello");
    }
}
