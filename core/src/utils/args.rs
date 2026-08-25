use std::sync::OnceLock;

use clap::Parser;

// Tarwyn server configuration
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
pub struct TarwynArgs {
    /// Enable logging for the Tarwyn server
    #[arg(short, long, default_value_t = false)]
    pub log: bool,
}

pub static CONFIG: OnceLock<TarwynArgs> = OnceLock::new();
