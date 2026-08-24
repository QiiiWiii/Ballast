use std::io::{self, Read};

use ballast_backtest::replay;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut input = String::new();
    io::stdin().read_to_string(&mut input)?;
    let value = serde_json::from_str(&input)?;
    let report = replay(value)?;
    serde_json::to_writer_pretty(io::stdout(), &report)?;
    println!();
    Ok(())
}
