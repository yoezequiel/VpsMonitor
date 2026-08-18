mod commands;
mod logtap;

use clap::Parser;
use cli::{AgentCommand, Cli, Command, IncidentCommand, ServerCommand};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::util::SubscriberInitExt;

    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::from_default_env())
        .with(tracing_subscriber::fmt::layer())
        .with(logtap::LogTapLayer)
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
        Command::Alerts => commands::alerts::list().await?,
        Command::Logs { server, level } => commands::logs::run(server, level).await?,
        Command::Incident { command } => match command {
            IncidentCommand::List => commands::incident::list().await?,
        },
    }

    Ok(())
}
