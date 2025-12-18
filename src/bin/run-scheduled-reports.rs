//! CLI tool for running scheduled reports
//!
//! This binary can be invoked by cron to execute due scheduled reports.
//!
//! Usage:
//!   run-scheduled-reports [--config <path>] [--schedule-id <uuid>]
//!
//! Options:
//!   --config      Path to configuration file (default: config/config.yaml)
//!   --schedule-id Run a specific schedule by ID instead of all due schedules
//!   --dry-run     Show what would be run without executing
//!   --verbose     Enable verbose output
//!
//! Example cron entry (run every minute):
//!   * * * * * /usr/local/bin/run-scheduled-reports --config /etc/openvox-webui/config.yaml

use std::env;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use openvox_webui::AppConfig;
use sqlx::sqlite::SqlitePoolOptions;
use tracing::{error, info, warn, Level};
use tracing_subscriber::FmtSubscriber;
use uuid::Uuid;

#[tokio::main]
async fn main() -> Result<()> {
    // Parse command line arguments
    let args: Vec<String> = env::args().collect();
    let mut config_path: Option<PathBuf> = None;
    let mut schedule_id: Option<Uuid> = None;
    let mut dry_run = false;
    let mut verbose = false;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--config" => {
                if i + 1 < args.len() {
                    config_path = Some(PathBuf::from(&args[i + 1]));
                    i += 1;
                }
            }
            "--schedule-id" => {
                if i + 1 < args.len() {
                    schedule_id = Some(Uuid::parse_str(&args[i + 1])?);
                    i += 1;
                }
            }
            "--dry-run" => {
                dry_run = true;
            }
            "--verbose" | "-v" => {
                verbose = true;
            }
            "--help" | "-h" => {
                print_help();
                return Ok(());
            }
            _ => {
                eprintln!("Unknown argument: {}", args[i]);
                print_help();
                std::process::exit(1);
            }
        }
        i += 1;
    }

    // Initialize logging
    let log_level = if verbose { Level::DEBUG } else { Level::INFO };
    let subscriber = FmtSubscriber::builder()
        .with_max_level(log_level)
        .with_target(false)
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;

    info!("OpenVox WebUI - Scheduled Reports Runner");

    // Load configuration
    let config = if let Some(path) = config_path {
        info!("Config file: {}", path.display());
        std::env::set_var("OPENVOX_CONFIG", path.to_str().unwrap_or(""));
        AppConfig::load()?
    } else {
        info!("Using default configuration paths");
        AppConfig::load()?
    };

    // Connect to database
    let db_url = &config.database.url;

    let pool = SqlitePoolOptions::new()
        .max_connections(2)
        .connect(db_url)
        .await?;

    info!("Connected to database: {}", db_url);

    // Run migrations
    sqlx::migrate!("./migrations").run(&pool).await?;

    // Create PuppetDB client if configured
    let puppetdb = if let Some(ref puppetdb_config) = config.puppetdb {
        match openvox_webui::services::PuppetDbClient::new(puppetdb_config) {
            Ok(client) => {
                info!("Connected to PuppetDB at {}", puppetdb_config.url);
                Some(Arc::new(client))
            }
            Err(e) => {
                warn!(
                    "Failed to connect to PuppetDB: {}. Reports requiring PuppetDB will fail.",
                    e
                );
                None
            }
        }
    } else {
        warn!("PuppetDB not configured. Reports requiring PuppetDB will fail.");
        None
    };

    // Create scheduler
    let scheduler = openvox_webui::services::ReportScheduler::new(pool.clone(), puppetdb);

    if dry_run {
        info!("Dry run mode - showing what would be executed");

        use openvox_webui::db::repository::ReportScheduleRepository;
        let repo = ReportScheduleRepository::new(&pool);

        if let Some(id) = schedule_id {
            if let Some(schedule) = repo.get_by_id(id).await? {
                println!("Would execute schedule:");
                println!("  ID: {}", schedule.id);
                println!("  Report ID: {}", schedule.report_id);
                println!("  Cron: {}", schedule.schedule_cron);
                println!("  Next run: {:?}", schedule.next_run_at);
            } else {
                eprintln!("Schedule not found: {}", id);
                std::process::exit(1);
            }
        } else {
            let due = repo.get_due().await?;
            if due.is_empty() {
                println!("No schedules due to run");
            } else {
                println!("Schedules due to run:");
                for schedule in due {
                    println!(
                        "  - {} (report: {}, cron: {})",
                        schedule.id, schedule.report_id, schedule.schedule_cron
                    );
                }
            }
        }
        return Ok(());
    }

    // Execute schedules
    let results = if let Some(id) = schedule_id {
        info!("Running specific schedule: {}", id);
        vec![scheduler.run_schedule(id).await?]
    } else {
        info!("Running all due schedules");
        scheduler.run_due_schedules().await?
    };

    // Report results
    let successful = results.iter().filter(|r| r.success).count();
    let failed = results.iter().filter(|r| !r.success).count();

    if results.is_empty() {
        info!("No schedules were due to run");
    } else {
        info!(
            "Executed {} schedules: {} successful, {} failed",
            results.len(),
            successful,
            failed
        );

        for result in &results {
            if result.success {
                info!(
                    "  [OK] Schedule {} completed in {}ms",
                    result.schedule_id, result.execution_time_ms
                );
            } else {
                error!(
                    "  [FAIL] Schedule {}: {}",
                    result.schedule_id,
                    result.error.as_deref().unwrap_or("Unknown error")
                );
            }
        }
    }

    // Exit with error code if any failed
    if failed > 0 {
        std::process::exit(1);
    }

    Ok(())
}

fn print_help() {
    println!("OpenVox WebUI - Scheduled Reports Runner");
    println!();
    println!("Usage:");
    println!("  run-scheduled-reports [OPTIONS]");
    println!();
    println!("Options:");
    println!("  --config <path>       Path to configuration file (default: config/config.yaml)");
    println!("  --schedule-id <uuid>  Run a specific schedule by ID instead of all due schedules");
    println!("  --dry-run             Show what would be run without executing");
    println!("  -v, --verbose         Enable verbose output");
    println!("  -h, --help            Show this help message");
    println!();
    println!("Example cron entries:");
    println!("  # Run every minute");
    println!(
        "  * * * * * /usr/local/bin/run-scheduled-reports --config /etc/openvox-webui/config.yaml"
    );
    println!();
    println!("  # Run every 5 minutes");
    println!("  */5 * * * * /usr/local/bin/run-scheduled-reports --config /etc/openvox-webui/config.yaml");
}
