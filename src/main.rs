mod settings;
mod database;
mod scim_schemas;
mod http_server;
mod grpc_server;
mod client;

use clap::{Parser, Subcommand};
use database::Database;
use http_server::AppState;

#[derive(Parser)]
#[command(name = "scim", version = "0.1.0", about = "SCIM 2.0 Rust server and control client")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the SCIM 2.0 HTTP server and gRPC control server concurrently
    Server {
        /// Path to the configuration file (default is config/config.toml)
        #[arg(long, default_value = "config/config.toml")]
        config: String,

        /// Override HTTP server host
        #[arg(long)]
        http_host: Option<String>,

        /// Override HTTP server port
        #[arg(long)]
        http_port: Option<u16>,

        /// Override gRPC server host
        #[arg(long)]
        grpc_host: Option<String>,

        /// Override gRPC server port
        #[arg(long)]
        grpc_port: Option<u16>,

        /// Override SCIM API authorization token
        #[arg(long)]
        auth_token: Option<String>,
    },
    /// Control the SCIM server using gRPC commands
    Client {
        /// Target SCIM gRPC endpoint to connect to
        #[arg(long, default_value = "http://127.0.0.1:50051")]
        endpoint: String,

        #[command(subcommand)]
        cmd: client::ClientCommand,
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Server {
            config: config_path,
            http_host,
            http_port,
            grpc_host,
            grpc_port,
            auth_token,
        } => {
            println!("Loading configuration from: {}", config_path);
            
            let mut settings = settings::Settings::new(&config_path)?;

            // Apply overrides if provided
            if let Some(h) = http_host {
                settings.http_host = h;
            }
            if let Some(p) = http_port {
                settings.http_port = p;
            }
            if let Some(h) = grpc_host {
                settings.grpc_host = h;
            }
            if let Some(p) = grpc_port {
                settings.grpc_port = p;
            }
            if let Some(t) = auth_token {
                settings.auth_token = t;
            }

            println!("Configuration loaded successfully.");
            println!("HTTP Server Settings: http://{}:{}", settings.http_host, settings.http_port);
            println!("gRPC Server Settings: {}:{}", settings.grpc_host, settings.grpc_port);
            println!("Bearer Token for API: {}", settings.auth_token);

            let db = Database::new();

            let app_state = AppState {
                db: db.clone(),
                settings: settings.clone(),
            };

            // Run HTTP server and gRPC server concurrently
            let http_handle = http_server::run(app_state);
            let grpc_handle = grpc_server::run(db, settings);

            println!("Starting both servers concurrently...");
            tokio::join!(http_handle, grpc_handle);
        }

        Commands::Client { endpoint, cmd } => {
            client::run_client(endpoint, cmd).await?;
        }
    }

    Ok(())
}
