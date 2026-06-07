use anyhow::{bail, Context, Result};
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Child, Command, Stdio};

/// A running resolver subprocess. Killed automatically on drop.
pub struct ResolverProcess {
    child: Child,
    pub port: u16,
}

impl ResolverProcess {
    /// Spawn the executable at `path`. Reads the first stdout line expecting `{"port": N}`.
    pub fn spawn(path: &Path) -> Result<Self> {
        let mut child = Command::new(path)
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .with_context(|| format!("spawn resolver {}", path.display()))?;

        let stdout = child.stdout.take().expect("stdout was piped");
        let mut reader = BufReader::new(stdout);
        let mut line = String::new();
        reader
            .read_line(&mut line)
            .context("read port line from resolver stdout")?;

        let port = parse_port_line(line.trim())
            .with_context(|| format!("parse port from resolver {}", path.display()))?;

        Ok(ResolverProcess { child, port })
    }
}

impl Drop for ResolverProcess {
    fn drop(&mut self) {
        let _ = self.child.kill();
    }
}

fn parse_port_line(s: &str) -> Result<u16> {
    #[derive(serde::Deserialize)]
    struct PortMsg {
        port: u16,
    }
    let msg: PortMsg = serde_json::from_str(s)
        .with_context(|| format!("expected {{\"port\": N}}, got: {s:?}"))?;
    if msg.port == 0 {
        bail!("resolver reported port 0");
    }
    Ok(msg.port)
}

#[cfg(test)]
mod tests {
    use super::parse_port_line;

    #[test]
    fn parses_valid_port() {
        assert_eq!(parse_port_line(r#"{"port": 8765}"#).unwrap(), 8765);
    }

    #[test]
    fn rejects_port_zero() {
        assert!(parse_port_line(r#"{"port": 0}"#).is_err());
    }

    #[test]
    fn rejects_garbage() {
        assert!(parse_port_line("hello world").is_err());
    }
}
