use std::path::PathBuf;

use garlemald_client::patch_format::apply_patch_file;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 3 {
        eprintln!("usage: apply_patch <patch_file> <game_root>");
        std::process::exit(1);
    }
    let patch = PathBuf::from(&args[1]);
    let game = PathBuf::from(&args[2]);
    std::fs::create_dir_all(&game).unwrap();
    match apply_patch_file(&patch, &game) {
        Ok(res) => {
            println!("OK, {} messages", res.messages.len());
            for m in res.messages.iter().take(5) {
                println!("  {m}");
            }
        }
        Err(e) => {
            eprintln!("FAIL: {e:#}");
            std::process::exit(1);
        }
    }
}
