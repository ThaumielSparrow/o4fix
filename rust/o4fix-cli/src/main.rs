mod args;
use clap::Parser;
use std::sync::atomic::AtomicBool;

fn main() -> std::process::ExitCode {
    let cli = match args::Cli::parse().validate() {
        Ok(c) => c,
        Err(e) => e.exit(), // usage errors: exit 2, argparse-compatible
    };
    let cfg = cli.to_config();
    let cancel = AtomicBool::new(false); // v1: Ctrl+C kills the process; GUI adds real cancel
    let mut failed = false;
    for video in &cli.videos {
        println!(
            "== {}",
            video.file_name().unwrap_or_default().to_string_lossy()
        );
        let r = o4core::pipeline::process(
            video,
            cli.output.as_deref(),
            &cfg,
            &|p| println!("{}", p.message),
            &cancel,
        );
        match r {
            Ok(_) => {} // Repaired and Healthy both print their own lines
            Err(e) => {
                eprintln!("   ERROR: {e}");
                failed = true;
            }
        }
    }
    if failed {
        std::process::ExitCode::FAILURE
    } else {
        std::process::ExitCode::SUCCESS
    }
}
