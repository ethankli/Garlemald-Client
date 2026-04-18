use anyhow::{anyhow, Result};

fn main() -> Result<()> {
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
