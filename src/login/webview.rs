//! Child-process entry point: hosts a single tao window with a wry WebView
//! pointed at the server's login page and intercepts navigation to the
//! `ffxiv://login_success?sessionId=…` custom scheme.
//!
//! Communicates the outcome to the parent over stdout using three
//! one-line sentinels defined below. The parent (see
//! `super::subprocess`) reads stdout line-by-line and maps them to a
//! [`LoginOutcome`].

use std::io::Write;

use anyhow::{Context, Result};
use tao::dpi::LogicalSize;
use tao::event::{Event, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoop};
use tao::window::WindowBuilder;
use wry::WebViewBuilder;

use crate::crypto::SESSION_ID_LEN;
use crate::version::APP_NAME;

/// Printed on success, followed by the 56-char session id.
pub const SESSION_PREFIX: &str = "SESSION_ID=";
/// Printed when the user closes the webview without logging in.
pub const CANCEL_SENTINEL: &str = "LOGIN_CANCELLED";
/// Printed when the webview itself errors out (e.g. fails to load).
pub const ERROR_PREFIX: &str = "LOGIN_ERROR=";

pub fn run_webview(login_url: &str) -> Result<()> {
    let event_loop: EventLoop<()> = EventLoop::new();
    let window = WindowBuilder::new()
        .with_title(format!("{APP_NAME} — Login"))
        .with_inner_size(LogicalSize::new(760.0, 600.0))
        .build(&event_loop)
        .context("building tao login window")?;

    let _webview = build_webview(&window, login_url)?;

    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Wait;
        if let Event::WindowEvent {
            event: WindowEvent::CloseRequested,
            ..
        } = event
        {
            report_line(CANCEL_SENTINEL);
            *control_flow = ControlFlow::Exit;
        }
    });
}

#[cfg(any(target_os = "linux", target_os = "windows"))]
fn build_webview<'a>(window: &'a tao::window::Window, login_url: &str) -> Result<wry::WebView> {
    WebViewBuilder::new(window)
        .with_url(login_url)
        .with_navigation_handler(navigation_handler)
        .build()
        .context("building wry webview")
}

#[cfg(target_os = "macos")]
fn build_webview<'a>(window: &'a tao::window::Window, login_url: &str) -> Result<wry::WebView> {
    // On macOS wry 0.44 defaults to a child NSView — that is fine for us
    // since the window has no other content.
    WebViewBuilder::new(window)
        .with_url(login_url)
        .with_navigation_handler(navigation_handler)
        .build()
        .context("building wry webview")
}

fn navigation_handler(uri: String) -> bool {
    if uri.starts_with("ffxiv://login_success") {
        if let Some(session_id) = parse_session_id(&uri) {
            report_line(&format!("{SESSION_PREFIX}{session_id}"));
            std::process::exit(0);
        } else {
            report_line(&format!(
                "{ERROR_PREFIX}login_success URL missing a {SESSION_ID_LEN}-char sessionId"
            ));
            std::process::exit(1);
        }
    }
    if uri.starts_with("ffxiv://") {
        // Other ffxiv:// targets aren't ours — cancel the navigation so the
        // engine doesn't try to handle them itself.
        return false;
    }
    true
}

fn parse_session_id(uri: &str) -> Option<String> {
    let (_, query) = uri.split_once('?')?;
    for pair in query.split('&') {
        let (key, value) = pair.split_once('=')?;
        if key == "sessionId" && value.len() == SESSION_ID_LEN {
            return Some(value.to_string());
        }
    }
    None
}

fn report_line(line: &str) {
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "{line}");
    let _ = out.flush();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_session_id_from_full_uri() {
        let uri = format!(
            "ffxiv://login_success?sessionId={}",
            "a".repeat(SESSION_ID_LEN)
        );
        let got = parse_session_id(&uri);
        assert_eq!(got.as_deref(), Some(&*"a".repeat(SESSION_ID_LEN)));
    }

    #[test]
    fn rejects_wrong_length_session_id() {
        let uri = "ffxiv://login_success?sessionId=tooshort";
        assert!(parse_session_id(uri).is_none());
    }

    #[test]
    fn ignores_extra_query_params() {
        let good = "a".repeat(SESSION_ID_LEN);
        let uri = format!("ffxiv://login_success?other=1&sessionId={good}&trailing=2");
        assert_eq!(parse_session_id(&uri).as_deref(), Some(&*good));
    }

    #[test]
    fn returns_none_for_missing_query() {
        assert!(parse_session_id("ffxiv://login_success").is_none());
    }
}
