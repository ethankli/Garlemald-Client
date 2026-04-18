//! Login flow: opens the selected server's loginUrl in a native webview
//! and waits for it to redirect to `ffxiv://login_success?sessionId=…`, at
//! which point we extract the 56-character session id and hand it off to
//! the launch pipeline.
//!
//! Because wry+tao and eframe's winit event loop can't both own the main
//! thread (a macOS constraint), the webview runs in a *subprocess*: the
//! binary re-enters itself with `--login-webview <URL>` and communicates the
//! result on stdout. The egui parent process spawns this child and polls a
//! channel for the outcome each frame, never blocking the UI.

mod subprocess;
mod webview;

pub use subprocess::{LoginOutcome, LoginTask};
pub use webview::{run_webview, CANCEL_SENTINEL, ERROR_PREFIX, SESSION_PREFIX};
