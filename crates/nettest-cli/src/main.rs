use clap::Parser;

/// Diagnose home internet / ISP problems.
#[derive(Parser)]
#[command(name = "nettest", version)]
struct Cli {}

fn main() {
    let _cli = Cli::parse();
    println!("nettest {}", nettest_core::version());
}
