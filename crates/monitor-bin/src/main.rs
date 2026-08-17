mod commands;

use clap::Parser;
use cli::{AgentCommand, Cli, Command, IncidentCommand, ServerCommand};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    // Multiple dependencies (axum-server's rustls backend, reqwest's TLS
    // client) pull in rustls without agreeing on a single crypto backend,
    // so it can't auto-select one. Pin aws-lc-rs once, up front.
    rustls::crypto::aws_lc_rs::default_provider()
        .install_default()
        .expect("no CryptoProvider installed yet");

    let cli = Cli::parse();

    match cli.command {
        Command::Install {
            mode,
            bind_addr,
            admin_username,
            admin_password,
            monitor_url,
            pairing_token,
            name,
        } => {
            commands::install::run(
                mode,
                bind_addr,
                admin_username,
                admin_password,
                monitor_url,
                pairing_token,
                name,
            )
            .await?
        }
        Command::Run => {
            let config = config::Config::load()
                .await?
                .ok_or_else(|| anyhow::anyhow!("not installed. Run `monitor install` first."))?;
            commands::run::run(config).await?
        }
        Command::Status => commands::status::run().await?,
        Command::Server { command } => match command {
            ServerCommand::List => commands::server::list().await?,
            ServerCommand::Add { name } => commands::server::add(name).await?,
        },
        Command::Agent { command } => match command {
            AgentCommand::Status => commands::agent::status().await?,
        },
        Command::Alerts => println!("monitor alerts: not implemented yet (V2)"),
        Command::Logs { server, level } => {
            println!("monitor logs (server={server:?}, level={level:?}): not implemented yet (V2)")
        }
        Command::Incident { command } => match command {
            IncidentCommand::List => println!("monitor incident list: not implemented yet (V2)"),
        },
    }

    Ok(())
}
