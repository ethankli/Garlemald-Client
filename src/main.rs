use anyhow::{anyhow, Result};

fn main() -> Result<()> {
    // Surface panics that happen inside FFI callbacks (e.g. AppKit's
    // `applicationDidFinishLaunching`) where `panic_cannot_unwind` would
    // otherwise abort before the message is printed.
    std::panic::set_hook(Box::new(|info| {
        eprintln!("PANIC: {info}");
        eprintln!("{}", std::backtrace::Backtrace::force_capture());
    }));

    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let mut args = std::env::args().skip(1);
    if let Some(first) = args.next() {
        if first == "--login-webview" {
            let url = args
                .next()
                .ok_or_else(|| anyhow!("--login-webview requires a URL argument"))?;
            return garlemald_client::login::run_webview(&url);
        }
    }

    garlemald_client::run()
}
