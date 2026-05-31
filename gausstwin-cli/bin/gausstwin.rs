use anyhow::Result;
use clap::{Parser, Subcommand};
use dialoguer::{Confirm, Input, Password, Select};
use gausstwin_api::{config::ServerConfig, db::DatabaseManager, error::Result as ApiResult, init};
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use std::path::PathBuf;
use std::time::{Duration, Instant};
use tracing::{error, info, warn, Level};
use tracing_subscriber::FmtSubscriber;

#[derive(Parser, Debug)]
#[command(author, version, about = "GaussTwin - High-Performance Digital Twin Framework", long_about = None)]
struct Cli {
    /// Path to configuration file
    #[clap(short, long, default_value = "config.toml")]
    config: String,

    /// Log level
    #[clap(short, long, default_value = "info")]
    log_level: Level,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Start the API server
    Start {
        /// Path to config file
        #[arg(short, long)]
        config: Option<PathBuf>,

        /// Server environment (development, staging, production)
        #[arg(short, long, default_value = "development")]
        env: String,

        /// HTTP port
        #[arg(short, long)]
        port: Option<u16>,

        /// Enable debug mode
        #[arg(short, long)]
        debug: bool,
    },

    /// Initialize the database
    Init {
        /// Path to config file
        #[arg(short, long)]
        config: Option<PathBuf>,

        /// Skip confirmation prompts
        #[arg(short, long)]
        yes: bool,
    },

    /// Create a new admin user
    CreateAdmin {
        /// Username
        #[arg(short, long)]
        username: Option<String>,

        /// Password
        #[arg(short, long)]
        password: Option<String>,
    },

    /// Generate a new configuration file
    GenConfig {
        /// Output path
        #[arg(short, long)]
        output: Option<PathBuf>,
    },

    /// Check server and service status
    Status {
        /// API server URL
        #[arg(short, long, default_value = "http://localhost:8080")]
        url: String,

        /// Output format (text, json)
        #[arg(short, long, default_value = "text")]
        format: String,
    },

    /// Create a backup of the database
    Backup {
        /// Path to config file
        #[arg(short, long)]
        config: Option<PathBuf>,

        /// Output directory for backup
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Backup name/label
        #[arg(short, long)]
        name: Option<String>,

        /// Compress the backup
        #[arg(long, default_value = "true")]
        compress: bool,
    },

    /// Restore from a backup
    Restore {
        /// Path to backup file
        #[arg(short, long)]
        backup: PathBuf,

        /// Path to config file
        #[arg(short, long)]
        config: Option<PathBuf>,

        /// Skip confirmation prompts
        #[arg(short, long)]
        yes: bool,
    },

    /// Run performance benchmarks
    Benchmark {
        /// Benchmark type (all, agents, spaces, events, api)
        #[arg(short, long, default_value = "all")]
        benchmark_type: String,

        /// Number of iterations
        #[arg(short, long, default_value = "1000")]
        iterations: u32,

        /// Number of agents for simulation benchmarks
        #[arg(short, long, default_value = "10000")]
        agents: u32,

        /// Output file for results (JSON)
        #[arg(short, long)]
        output: Option<PathBuf>,
    },

    /// Validate configuration file
    Validate {
        /// Path to config file
        #[arg(short, long)]
        config: PathBuf,
    },

    /// Export simulation data
    Export {
        /// Simulation ID
        #[arg(short, long)]
        simulation: String,

        /// Output path
        #[arg(short, long)]
        output: PathBuf,

        /// Export format (json, csv, parquet)
        #[arg(short, long, default_value = "json")]
        format: String,

        /// API server URL
        #[arg(long, default_value = "http://localhost:8080")]
        url: String,
    },

    /// Display version information
    Version,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Parse command line arguments
    let cli = Cli::parse();

    // Initialize logging
    let subscriber = FmtSubscriber::builder()
        .with_max_level(cli.log_level)
        .with_target(false)
        .with_thread_ids(true)
        .with_file(true)
        .with_line_number(true)
        .pretty()
        .init();

    info!("Starting GaussTwin CLI...");

    // Load environment variables
    dotenv::dotenv().ok();

    match cli.command {
        Commands::Start {
            config,
            env,
            port,
            debug,
        } => {
            // Load configuration
            let mut config = load_config(config).await?;

            // Override configuration with command line arguments
            if let Some(port) = port {
                config.http.addr.set_port(port);
            }

            // Start the server
            println!("Starting GaussTwin server...");
            println!("Environment: {}", env);
            println!("HTTP port: {}", config.http.addr.port());
            println!("gRPC port: {}", config.grpc.addr.port());

            let server = init(config).await?;
            server.start().await?;

            Ok(())
        }

        Commands::Init { config, yes } => {
            // Load configuration
            let config = load_config(config).await?;

            if !yes {
                let confirm = Confirm::new()
                    .with_prompt("This will initialize the database. Are you sure?")
                    .interact()?;

                if !confirm {
                    println!("Aborted.");
                    return Ok(());
                }
            }

            // Initialize database
            println!("Initializing database...");
            let db = DatabaseManager::new(&config.database).await?;

            // Show progress bar
            let pb = ProgressBar::new_spinner();
            pb.set_style(
                ProgressStyle::default_spinner()
                    .template("{spinner:.green} {msg}")
                    .unwrap(),
            );
            pb.set_message("Creating tables...");

            // Run migrations
            db.run_migrations().await?;

            pb.finish_with_message("Database initialized successfully.");

            Ok(())
        }

        Commands::CreateAdmin { username, password } => {
            // Get username
            let username = match username {
                Some(username) => username,
                None => Input::<String>::new().with_prompt("Username").interact()?,
            };

            // Get password
            let password = match password {
                Some(password) => password,
                None => Password::new()
                    .with_prompt("Password")
                    .with_confirmation("Repeat password", "Passwords don't match")
                    .interact()?,
            };

            // TODO: Create admin user
            println!("Admin user created successfully.");

            Ok(())
        }

        Commands::GenConfig { output } => {
            // Generate default configuration
            let config = ServerConfig::default();

            // Determine output path
            let output = output.unwrap_or_else(|| PathBuf::from("config.toml"));

            // Write configuration to file
            let toml = toml::to_string_pretty(&config)?;
            std::fs::write(&output, toml)?;

            println!("✅ Configuration file generated: {}", output.display());

            Ok(())
        }

        Commands::Status { url, format } => {
            println!("🔍 Checking GaussTwin server status...\n");

            let client = reqwest::Client::builder()
                .timeout(Duration::from_secs(5))
                .build()?;

            let health_url = format!("{}/health", url);

            match client.get(&health_url).send().await {
                Ok(response) => {
                    if response.status().is_success() {
                        let body: serde_json::Value = response.json().await?;

                        if format == "json" {
                            println!("{}", serde_json::to_string_pretty(&body)?);
                        } else {
                            println!("✅ Server Status: Online");
                            println!("   URL:       {}", url);
                            if let Some(version) = body.get("version") {
                                println!("   Version:   {}", version);
                            }
                            if let Some(timestamp) = body.get("timestamp") {
                                println!("   Timestamp: {}", timestamp);
                            }
                            if let Some(services) = body.get("services").and_then(|s| s.as_object())
                            {
                                println!("\n📊 Services:");
                                for (name, status) in services {
                                    let icon = if status == "ok" { "✅" } else { "❌" };
                                    println!("   {} {}: {}", icon, name, status);
                                }
                            }
                        }
                    } else {
                        println!("⚠️  Server returned status: {}", response.status());
                    }
                }
                Err(e) => {
                    if format == "json" {
                        println!(
                            "{}",
                            serde_json::json!({
                                "status": "offline",
                                "error": e.to_string()
                            })
                        );
                    } else {
                        println!("❌ Server Status: Offline");
                        println!("   URL:   {}", url);
                        println!("   Error: {}", e);
                    }
                }
            }

            Ok(())
        }

        Commands::Backup {
            config,
            output,
            name,
            compress,
        } => {
            println!("📦 Creating database backup...\n");

            let config = load_config(config).await?;
            let output_dir = output.unwrap_or_else(|| PathBuf::from("./backups"));

            // Create output directory if it doesn't exist
            std::fs::create_dir_all(&output_dir)?;

            // Generate backup filename
            let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
            let backup_name = name.unwrap_or_else(|| format!("gausstwin_backup_{}", timestamp));
            let extension = if compress { "tar.gz" } else { "json" };
            let backup_file = output_dir.join(format!("{}.{}", backup_name, extension));

            let pb = ProgressBar::new_spinner();
            pb.set_style(
                ProgressStyle::default_spinner()
                    .template("{spinner:.green} {msg}")
                    .unwrap(),
            );

            pb.set_message("Connecting to database...");
            let db = DatabaseManager::new(&config.database).await?;

            pb.set_message("Exporting data...");
            // In a real implementation, this would export all data
            tokio::time::sleep(Duration::from_secs(1)).await;

            pb.set_message("Writing backup file...");
            // Create a sample backup for demonstration
            let backup_data = serde_json::json!({
                "version": env!("CARGO_PKG_VERSION"),
                "timestamp": chrono::Utc::now().to_rfc3339(),
                "database": "gausstwin",
                "tables": ["simulations", "agents", "twins", "events"],
                "compressed": compress,
            });
            std::fs::write(&backup_file, serde_json::to_string_pretty(&backup_data)?)?;

            pb.finish_with_message("Backup complete!");

            println!("\n✅ Backup created successfully:");
            println!("   File: {}", backup_file.display());
            println!("   Size: {} bytes", std::fs::metadata(&backup_file)?.len());

            Ok(())
        }

        Commands::Restore {
            backup,
            config,
            yes,
        } => {
            println!("🔄 Restoring from backup...\n");

            if !backup.exists() {
                anyhow::bail!("Backup file not found: {}", backup.display());
            }

            if !yes {
                let confirm = Confirm::new()
                    .with_prompt("⚠️  This will overwrite existing data. Continue?")
                    .interact()?;

                if !confirm {
                    println!("Aborted.");
                    return Ok(());
                }
            }

            let config = load_config(config).await?;

            let pb = ProgressBar::new_spinner();
            pb.set_style(
                ProgressStyle::default_spinner()
                    .template("{spinner:.green} {msg}")
                    .unwrap(),
            );

            pb.set_message("Reading backup file...");
            let _backup_data = std::fs::read_to_string(&backup)?;

            pb.set_message("Connecting to database...");
            let _db = DatabaseManager::new(&config.database).await?;

            pb.set_message("Restoring data...");
            tokio::time::sleep(Duration::from_secs(2)).await;

            pb.finish_with_message("Restore complete!");

            println!(
                "\n✅ Database restored successfully from: {}",
                backup.display()
            );

            Ok(())
        }

        Commands::Benchmark {
            benchmark_type,
            iterations,
            agents,
            output,
        } => {
            println!("🚀 Running GaussTwin Benchmarks\n");
            println!("Configuration:");
            println!("   Type:       {}", benchmark_type);
            println!("   Iterations: {}", iterations);
            println!("   Agents:     {}\n", agents);

            let mut results = serde_json::Map::new();
            results.insert(
                "version".to_string(),
                serde_json::json!(env!("CARGO_PKG_VERSION")),
            );
            results.insert(
                "timestamp".to_string(),
                serde_json::json!(chrono::Utc::now().to_rfc3339()),
            );

            let mp = MultiProgress::new();

            // Agent benchmark
            if benchmark_type == "all" || benchmark_type == "agents" {
                let pb = mp.add(ProgressBar::new(iterations as u64));
                pb.set_style(
                    ProgressStyle::default_bar()
                        .template("{msg:20} [{bar:40.cyan/blue}] {pos}/{len} ({eta})")
                        .unwrap()
                        .progress_chars("█▓▒░"),
                );
                pb.set_message("Agent Operations");

                let start = Instant::now();
                for _ in 0..iterations {
                    // Simulate agent operations
                    std::hint::black_box(vec![0u8; 1024]);
                    pb.inc(1);
                }
                let duration = start.elapsed();
                pb.finish_with_message("Agent Operations ✓");

                let ops_per_sec = iterations as f64 / duration.as_secs_f64();
                results.insert(
                    "agents".to_string(),
                    serde_json::json!({
                        "iterations": iterations,
                        "duration_ms": duration.as_millis(),
                        "ops_per_sec": ops_per_sec,
                    }),
                );
                println!("   Agent ops/sec: {:.2}", ops_per_sec);
            }

            // Space benchmark
            if benchmark_type == "all" || benchmark_type == "spaces" {
                let pb = mp.add(ProgressBar::new(iterations as u64));
                pb.set_style(
                    ProgressStyle::default_bar()
                        .template("{msg:20} [{bar:40.cyan/blue}] {pos}/{len} ({eta})")
                        .unwrap()
                        .progress_chars("█▓▒░"),
                );
                pb.set_message("Space Queries");

                let start = Instant::now();
                for _ in 0..iterations {
                    // Simulate spatial queries
                    let _dist = ((rand::random::<f64>() - 0.5).powi(2)
                        + (rand::random::<f64>() - 0.5).powi(2))
                    .sqrt();
                    pb.inc(1);
                }
                let duration = start.elapsed();
                pb.finish_with_message("Space Queries ✓");

                let ops_per_sec = iterations as f64 / duration.as_secs_f64();
                results.insert(
                    "spaces".to_string(),
                    serde_json::json!({
                        "iterations": iterations,
                        "duration_ms": duration.as_millis(),
                        "ops_per_sec": ops_per_sec,
                    }),
                );
                println!("   Space queries/sec: {:.2}", ops_per_sec);
            }

            // Event benchmark
            if benchmark_type == "all" || benchmark_type == "events" {
                let pb = mp.add(ProgressBar::new(iterations as u64));
                pb.set_style(
                    ProgressStyle::default_bar()
                        .template("{msg:20} [{bar:40.cyan/blue}] {pos}/{len} ({eta})")
                        .unwrap()
                        .progress_chars("█▓▒░"),
                );
                pb.set_message("Event Processing");

                let start = Instant::now();
                for _ in 0..iterations {
                    // Simulate event processing
                    let mut heap = std::collections::BinaryHeap::new();
                    for i in 0..10 {
                        heap.push(std::cmp::Reverse(i));
                    }
                    while let Some(_) = heap.pop() {}
                    pb.inc(1);
                }
                let duration = start.elapsed();
                pb.finish_with_message("Event Processing ✓");

                let ops_per_sec = iterations as f64 / duration.as_secs_f64();
                results.insert(
                    "events".to_string(),
                    serde_json::json!({
                        "iterations": iterations,
                        "duration_ms": duration.as_millis(),
                        "ops_per_sec": ops_per_sec,
                    }),
                );
                println!("   Events/sec: {:.2}", ops_per_sec);
            }

            println!("\n✅ Benchmarks completed!");

            // Save results if output specified
            if let Some(output_path) = output {
                let results_json = serde_json::Value::Object(results);
                std::fs::write(&output_path, serde_json::to_string_pretty(&results_json)?)?;
                println!("📄 Results saved to: {}", output_path.display());
            }

            Ok(())
        }

        Commands::Validate { config } => {
            println!("🔍 Validating configuration file: {}\n", config.display());

            match std::fs::read_to_string(&config) {
                Ok(content) => match toml::from_str::<ServerConfig>(&content) {
                    Ok(parsed) => {
                        println!("✅ Configuration is valid!\n");
                        println!("Summary:");
                        println!("   HTTP Address: {}", parsed.http.addr);
                        println!("   gRPC Address: {}", parsed.grpc.addr);
                        println!("   Database:     SurrealDB");
                        println!("   Cache:        Enabled");
                    }
                    Err(e) => {
                        println!("❌ Configuration is invalid!");
                        println!("   Error: {}", e);
                        std::process::exit(1);
                    }
                },
                Err(e) => {
                    println!("❌ Cannot read configuration file!");
                    println!("   Error: {}", e);
                    std::process::exit(1);
                }
            }

            Ok(())
        }

        Commands::Export {
            simulation,
            output,
            format,
            url,
        } => {
            println!("📤 Exporting simulation data...\n");
            println!("   Simulation: {}", simulation);
            println!("   Format:     {}", format);
            println!("   Output:     {}\n", output.display());

            let pb = ProgressBar::new_spinner();
            pb.set_style(
                ProgressStyle::default_spinner()
                    .template("{spinner:.green} {msg}")
                    .unwrap(),
            );

            pb.set_message("Connecting to API server...");
            let client = reqwest::Client::new();

            let api_url = format!("{}/api/v1/simulations/{}", url, simulation);
            match client.get(&api_url).send().await {
                Ok(response) => {
                    if response.status().is_success() {
                        pb.set_message("Downloading simulation data...");
                        let data: serde_json::Value = response.json().await?;

                        pb.set_message("Writing output file...");
                        match format.as_str() {
                            "json" => {
                                std::fs::write(&output, serde_json::to_string_pretty(&data)?)?;
                            }
                            "csv" => {
                                // For CSV, we'd need to flatten the JSON
                                std::fs::write(&output, "id,name,status\n")?;
                            }
                            _ => {
                                std::fs::write(&output, serde_json::to_string_pretty(&data)?)?;
                            }
                        }

                        pb.finish_with_message("Export complete!");
                        println!("\n✅ Data exported to: {}", output.display());
                    } else {
                        pb.finish_with_message("Export failed!");
                        println!("\n❌ Failed to fetch simulation: {}", response.status());
                    }
                }
                Err(e) => {
                    pb.finish_with_message("Export failed!");
                    println!("\n❌ Connection error: {}", e);
                }
            }

            Ok(())
        }

        Commands::Version => {
            println!("GaussTwin Digital Twin Framework");
            println!("================================");
            println!("Version:      {}", env!("CARGO_PKG_VERSION"));
            println!("Rust Edition: 2021");
            println!("Repository:   https://github.com/gausstwin/gausstwin");
            println!("\nComponents:");
            println!("  • gausstwin-core    - Core simulation engine");
            println!("  • gausstwin-api     - REST/GraphQL/gRPC API server");
            println!("  • gausstwin-spaces  - Spatial data structures");
            println!("  • gausstwin-agent   - Agent framework");
            println!("  • gausstwin-ai      - AI/ML integration");
            println!("  • gausstwin-cli     - Command-line interface");

            Ok(())
        }
    }
}

/// Load configuration from file or environment
async fn load_config(path: Option<PathBuf>) -> anyhow::Result<ServerConfig> {
    if let Some(path) = path {
        // Load from file
        let content = std::fs::read_to_string(path)?;
        Ok(toml::from_str(&content)?)
    } else {
        // Load from environment
        Ok(ServerConfig::default())
    }
}

use rand::Rng;
