use std::process::{Command, Stdio, ChildStdin, ChildStdout};
use anyhow::Result;

pub struct Nvim {
    pub child: std::process::Child,
    pub stdin: Option<ChildStdin>,
    pub stdout: Option<ChildStdout>,
}

impl Nvim {
    pub fn spawn() -> Result<Self> {
        let mut child = Command::new("nvim")
            .arg("--embed")
            .arg("--headless")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()?;

        let stdin = child.stdin.take();
        let stdout = child.stdout.take();

        Ok(Self { child, stdin, stdout })
    }
}
