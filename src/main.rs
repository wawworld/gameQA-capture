//! gameQA — non-intrusive game data collection pipeline.
//!
//! CLI entry point: arg parsing, YAML merge load, pipeline bootstrap, signal handler.
#![deny(clippy::unwrap_used, clippy::expect_used)]

use game_qa::automation::quarantine::QuarantineManager;
use game_qa::automation::{SchedulerDecision, SessionOutcome, SessionScheduler};
use game_qa::capture::dxgi::DxgiBackend;
use game_qa::config::{load_merged_profile, ProfileConfig};
use game_qa::hooks::win32::WillhookBackend;
use game_qa::pipeline::metrics::PipelineMetrics;
use game_qa::pipeline::{Pipeline, PipelineStop, build_consumers};
use game_qa::roi::{GraceExceededAction, RoiManager};
use game_qa::session::storage::{SessionWriter, validate_session_artifacts};
use game_qa::session::triggers::{KeyPressTrigger, TemplateMatchTrigger, TimeoutTrigger};
use game_qa::session::{SessionId, SessionMode, SessionRecord, SessionStatus};
use game_qa::{MonotonicNs, WallNs};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

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

// ─── Session ID generation ────────────────────────────────────────────────────

/// Generate a session ID from the current wall clock + game/profile names.
/// Format: `YYYYMMDDTHHMMSSZ-{game}-{profile}`
fn make_session_id(game: &str, profile: &str) -> SessionId {
    // Use UNIX epoch seconds to build a compact timestamp.
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_secs();

    // Manual conversion: secs since epoch → YYYYMMDDTHHMMSSZ
    let (year, month, day, hour, min, sec) = epoch_secs_to_utc(secs);
    let ts = format!(
        "{:04}{:02}{:02}T{:02}{:02}{:02}Z",
        year, month, day, hour, min, sec
    );
    SessionId(format!("{}-{}-{}", ts, game, profile))
}

/// Convert UNIX seconds to (year, month, day, hour, min, sec) UTC.
///
/// Handles dates from 1970 through approximately 2099; sufficient for
/// session IDs that will never appear more than a few decades out.
fn epoch_secs_to_utc(secs: u64) -> (u32, u32, u32, u32, u32, u32) {
    let sec = (secs % 60) as u32;
    let min = ((secs / 60) % 60) as u32;
    let hour = ((secs / 3600) % 24) as u32;
    let days = secs / 86400;

    // Gregorian calendar arithmetic starting from 1970-01-01.
    let mut year = 1970u32;
    let mut remaining = days;
    loop {
        let days_in_year = if is_leap(year) { 366 } else { 365 };
        if remaining < days_in_year {
            break;
        }
        remaining -= days_in_year;
        year += 1;
    }
    let leap = is_leap(year);
    let month_days = [31, if leap { 29 } else { 28 }, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let mut month = 1u32;
    for &d in &month_days {
        if remaining < d {
            break;
        }
        remaining -= d;
        month += 1;
    }
    let day = remaining as u32 + 1;
    (year, month, day, hour, min, sec)
}

fn is_leap(year: u32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

// ─── Single-session pipeline run ─────────────────────────────────────────────

/// Run one capture session end-to-end.
///
/// Returns `SessionOutcome::Success` if the pipeline stopped via a trigger
/// and the session was committed successfully; `SessionOutcome::Failure` on
/// any error.
fn run_single_session(cfg: &ProfileConfig, game: &str, profile: &str) -> SessionOutcome {
    let session_id = make_session_id(game, profile);
    let data_root = PathBuf::from("data");
    let session_start = Instant::now();

    // --- Session writer (two-phase commit) ---
    let writer = match SessionWriter::new(data_root.clone(), session_id.clone()) {
        Ok(w) => w,
        Err(e) => return SessionOutcome::Failure(format!("SessionWriter init failed: {e}")),
    };

    // --- Determine session mode from consumer config ---
    let mode = match (cfg.consumer.recording_enabled, cfg.consumer.realtime_enabled) {
        (true, true) => SessionMode::Both,
        (false, true) => SessionMode::Realtime,
        _ => SessionMode::Recording,
    };

    // --- Disk recorder consumers ---
    let fps = cfg.capture.target_fps as f64;
    let metrics = PipelineMetrics::new();
    let consumers_result = match build_consumers(
        &cfg.consumer,
        writer.session_dir().to_path_buf(),
        fps,
        Arc::clone(&metrics),
    ) {
        Ok(r) => r,
        Err(e) => {
            let _ = writer.abort();
            return SessionOutcome::Failure(format!("build_consumers failed: {e}"));
        }
    };

    // --- DXGI capture backend ---
    let backend: Box<dyn game_qa::capture::CaptureBackend> =
        match DxgiBackend::new(cfg.capture.clone(), session_start) {
            Ok(b) => Box::new(b),
            Err(e) => {
                let _ = writer.abort();
                return SessionOutcome::Failure(format!("DxgiBackend init failed: {e}"));
            }
        };

    // --- Input hook backend ---
    let mut hook_backend = WillhookBackend::new(cfg.events.clone(), session_start);
    if let Err(e) = game_qa::hooks::InputHookBackend::start(&mut hook_backend) {
        let _ = writer.abort();
        return SessionOutcome::Failure(format!("Hook start failed: {e}"));
    }

    // --- Triggers ---
    let mut triggers: Vec<Box<dyn game_qa::session::triggers::TerminationTrigger>> = vec![
        Box::new(KeyPressTrigger::new(cfg.trigger.stop_key)),
        Box::new(TimeoutTrigger::new(session_start, cfg.trigger.timeout_seconds)),
    ];
    if let Some(tmpl_cfg) = &cfg.trigger.game_over_template {
        triggers.push(Box::new(TemplateMatchTrigger::new(
            tmpl_cfg.template_path.clone(),
            tmpl_cfg.threshold,
            tmpl_cfg.confirm_frames,
        )));
    }

    // --- ROI manager ---
    let roi_manager = RoiManager::new(cfg.roi.clone(), GraceExceededAction::MarkLowQuality);

    // --- Build and run pipeline ---
    let mut pipeline = Pipeline::new(
        cfg.clone(),
        session_start,
        backend,
        Box::new(hook_backend),
        consumers_result.consumers,
        triggers,
        roi_manager,
        Arc::clone(&metrics),
    );

    let stop_reason = pipeline.run();

    // --- Build session record ---
    let wall_now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_nanos() as u64;
    let mono_end = session_start.elapsed().as_nanos() as u64;

    let mut record = SessionRecord::new(
        session_id.as_str().to_string(),
        WallNs(wall_now),
        game.to_string(),
        profile.to_string(),
        mode,
    );
    record.ts_wall_end_ns = Some(WallNs(wall_now));
    record.ts_mono_end_ns = Some(MonotonicNs(mono_end));

    match stop_reason {
        PipelineStop::TriggerFired(reason) => {
            eprintln!("[INFO] Session stopped: {}", reason);
            record.status = SessionStatus::Complete;
            match writer.commit(&record) {
                Ok(path) => {
                    eprintln!("[INFO] Session committed: {}", path.display());
                    SessionOutcome::Success
                }
                Err(e) => SessionOutcome::Failure(format!("Commit failed: {e}")),
            }
        }
        PipelineStop::Error(e) => {
            record.status = SessionStatus::Incomplete;
            let _ = writer.abort();
            SessionOutcome::Failure(format!("Pipeline error: {e}"))
        }
    }
}

// ─── Batch automation loop ────────────────────────────────────────────────────

fn run_batch(cfg: &ProfileConfig, game: &str, profile: &str) {
    let mut scheduler = SessionScheduler::new(cfg.automation.clone());
    let mut quarantine = QuarantineManager::new();

    eprintln!(
        "[INFO] Batch automation: target {} sessions, max_consecutive_failures {}",
        cfg.automation.target_session_count, cfg.automation.max_consecutive_failures
    );

    loop {
        let outcome = run_single_session(cfg, game, profile);
        let session_id = make_session_id(game, profile); // for quarantine label

        match scheduler.record_outcome(outcome) {
            SchedulerDecision::Done => {
                eprintln!(
                    "[INFO] Batch complete: {} sessions",
                    scheduler.sessions_completed()
                );
                break;
            }
            SchedulerDecision::Quarantine(reason) => {
                quarantine.quarantine(session_id, reason.clone());
                eprintln!("[WARN] Session quarantined ({}); advancing.", reason);
                std::thread::sleep(Duration::from_secs_f64(cfg.automation.restart_delay_seconds));
            }
            SchedulerDecision::RestartAfterDelay(delay) => {
                eprintln!(
                    "[INFO] Restarting in {:.1} s ({} done, {} quarantined)",
                    delay,
                    scheduler.sessions_completed(),
                    quarantine.count()
                );
                std::thread::sleep(Duration::from_secs_f64(delay));
            }
        }
    }
}

// ─── Entry point ─────────────────────────────────────────────────────────────

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

        Command::Capture { game, profile, auto, debug: _, mock_consumer: _ } => {
            let cfg = match load_merged_profile(&config_dir, &game, &profile) {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("Config load error: {}", e);
                    std::process::exit(1);
                }
            };

            if auto && cfg.automation.enabled {
                run_batch(&cfg, &game, &profile);
            } else {
                match run_single_session(&cfg, &game, &profile) {
                    SessionOutcome::Success => {}
                    SessionOutcome::Failure(reason) => {
                        eprintln!("Session failed: {}", reason);
                        std::process::exit(1);
                    }
                }
            }
        }

        Command::Validate { session_path } => {
            println!("[INFO] Validating session: {}", session_path.display());
            match validate_session_artifacts(&session_path) {
                Err(e) => {
                    eprintln!("[ERROR] Validation failed: {}", e);
                    std::process::exit(1);
                }
                Ok(report) => {
                    println!("[INFO] session_id   : {}", report.session_id);
                    println!("[INFO] frame_count  : {}", report.frame_count);
                    println!("[INFO] event_count  : {}", report.event_count);
                    println!("[INFO] has_video    : {}", report.has_video);
                    if report.passed() {
                        println!("[PASS] All schema checks passed.");
                    } else {
                        println!("[FAIL] Schema violations found ({}):", report.errors.len());
                        for err in &report.errors {
                            eprintln!("  - {}", err);
                        }
                        std::process::exit(1);
                    }
                }
            }
        }
    }
}
