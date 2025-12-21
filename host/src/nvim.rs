use std::process::{Command, Stdio};
use anyhow::Result;

pub struct Nvim {
    pub child: std::process::Child,
    pub stdin: std::process::ChildStdin,
    pub stdout: std::process::ChildStdout,
}

impl Nvim {
    pub fn spawn() -> Result<Self> {
        let mut child = Command::new("nvim")
            .arg("--embed")
            .arg("--headless")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()?;

        let stdin = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();

        Ok(Self { child, stdin, stdout })
    }
}
