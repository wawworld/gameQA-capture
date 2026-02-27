//! gameQA — non-intrusive game data collection pipeline.
//!
//! CLI entry point: arg parsing, YAML merge load, pipeline bootstrap, signal handler.
#![deny(clippy::unwrap_used, clippy::expect_used)]

use game_qa::config::load_merged_profile;
use std::path::PathBuf;

/// Subcommand dispatched from CLI args.
#[allow(dead_code)]
enum Command {
    /// Run a capture session.
    Capture {
        game: String,
        profile: String,
        auto: bool,
        debug: bool,
        mock_consumer: bool,
    },
    /// Dump the merged config and exit.
    ConfigDump { game: String, profile: String },
    /// Validate an existing session directory.
    Validate { session_path: PathBuf },
}

fn parse_args() -> Result<Command, String> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        return Err("Usage: game-qa <capture|config|validate> [options]".to_string());
    }

    match args[1].as_str() {
        "capture" => {
            let game = flag_value(&args, "--game").unwrap_or_else(|| "chrome_dino".to_string());
            let profile = flag_value(&args, "--profile").unwrap_or_else(|| "test".to_string());
            let auto = args.contains(&"--auto".to_string());
            let debug = args.contains(&"--debug".to_string());
            let mock_consumer = args.contains(&"--mock-consumer".to_string());
            Ok(Command::Capture {
                game,
                profile,
                auto,
                debug,
                mock_consumer,
            })
        }
        "config" if args.get(2).map(|s| s.as_str()) == Some("dump") => {
            let game = flag_value(&args, "--game").unwrap_or_else(|| "chrome_dino".to_string());
            let profile = flag_value(&args, "--profile").unwrap_or_else(|| "test".to_string());
            Ok(Command::ConfigDump { game, profile })
        }
        "validate" => {
            let path = args
                .get(2)
                .ok_or_else(|| "validate requires a session path argument".to_string())?;
            Ok(Command::Validate {
                session_path: PathBuf::from(path),
            })
        }
        other => Err(format!("Unknown command: {}", other)),
    }
}

fn flag_value(args: &[String], flag: &str) -> Option<String> {
    let pos = args.iter().position(|a| a == flag)?;
    args.get(pos + 1).cloned()
}

fn main() {
    let cmd = match parse_args() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    };

    let config_dir = PathBuf::from("config");

    match cmd {
        Command::ConfigDump { game, profile } => {
            match load_merged_profile(&config_dir, &game, &profile) {
                Ok(cfg) => match serde_json::to_string_pretty(&cfg) {
                    Ok(json) => println!("{}", json),
                    Err(e) => {
                        eprintln!("Serialization error: {}", e);
                        std::process::exit(1);
                    }
                },
                Err(e) => {
                    eprintln!("Config load error: {}", e);
                    std::process::exit(1);
                }
            }
        }

        Command::Capture {
            game,
            profile,
            auto: _,
            debug: _,
            mock_consumer: _,
        } => {
            let _cfg = match load_merged_profile(&config_dir, &game, &profile) {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("Config load error: {}", e);
                    std::process::exit(1);
                }
            };
            println!("[INFO] Loading profile: {} + {}", game, profile);
            // TODO(T036): bootstrap full pipeline here.
            println!("[INFO] Pipeline bootstrap not yet implemented.");
        }

        Command::Validate { session_path } => {
            // TODO(T036): validate session artifacts against contracts/session-schema.md.
            println!("[INFO] Validating session: {}", session_path.display());
        }
    }
}
