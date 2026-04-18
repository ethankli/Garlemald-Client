//! Parent-side orchestration: spawns the binary as `--login-webview <URL>`,
//! reads one outcome line from its stdout on a worker thread, and hands the
//! result back to the UI through an `mpsc::Receiver`.

use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::thread::{self, JoinHandle};

use anyhow::{anyhow, Context, Result};

use super::webview::{CANCEL_SENTINEL, ERROR_PREFIX, SESSION_PREFIX};

pub const LOGIN_WEBVIEW_ARG: &str = "--login-webview";

#[derive(Debug, Clone)]
pub enum LoginOutcome {
    Success(String),
    Cancelled,
    Error(String),
}

pub struct LoginTask {
    receiver: mpsc::Receiver<LoginOutcome>,
    child: Option<Child>,
    worker: Option<JoinHandle<()>>,
}

impl LoginTask {
    pub fn start(login_url: String) -> Result<Self> {
        if login_url.is_empty() {
            return Err(anyhow!("login URL is empty"));
        }
        let exe = std::env::current_exe().context("resolving current exe path")?;

        let mut child = Command::new(&exe)
            .arg(LOGIN_WEBVIEW_ARG)
            .arg(&login_url)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .with_context(|| {
                format!(
                    "spawning login webview subprocess ({} {LOGIN_WEBVIEW_ARG} {login_url})",
                    exe.display()
                )
            })?;

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| anyhow!("failed to capture login child stdout"))?;
        let (tx, rx) = mpsc::channel();

        let worker = thread::Builder::new()
            .name("garlemald-login-reader".into())
            .spawn(move || {
                let reader = BufReader::new(stdout);
                let mut outcome = LoginOutcome::Error(
                    "login window exited without reporting an outcome".into(),
                );
                for line_result in reader.lines() {
                    let line = match line_result {
                        Ok(l) => l,
                        Err(e) => {
                            outcome = LoginOutcome::Error(format!("reading login stdout: {e}"));
                            break;
                        }
                    };
                    if let Some(sid) = line.strip_prefix(SESSION_PREFIX) {
                        outcome = LoginOutcome::Success(sid.to_string());
                        break;
                    }
                    if line == CANCEL_SENTINEL {
                        outcome = LoginOutcome::Cancelled;
                        break;
                    }
                    if let Some(msg) = line.strip_prefix(ERROR_PREFIX) {
                        outcome = LoginOutcome::Error(msg.to_string());
                        break;
                    }
                    // Any other output (e.g. log lines) is ignored.
                }
                let _ = tx.send(outcome);
            })
            .context("spawning login stdout reader thread")?;

        Ok(Self {
            receiver: rx,
            child: Some(child),
            worker: Some(worker),
        })
    }

    pub fn try_recv(&mut self) -> Option<LoginOutcome> {
        self.receiver.try_recv().ok()
    }

    pub fn cancel(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

impl Drop for LoginTask {
    fn drop(&mut self) {
        self.cancel();
        if let Some(handle) = self.worker.take() {
            let _ = handle.join();
        }
    }
}
