mod developer_window;
mod launcher_window;
mod patcher_window;
mod settings_window;

use anyhow::Result;

pub fn run() -> Result<()> {
    launcher_window::run()
}
