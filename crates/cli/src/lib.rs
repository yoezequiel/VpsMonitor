use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(
    name = "monitor",
    version,
    about = "Self-Hosted Infrastructure Monitor"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Clone, Copy, clap::ValueEnum, PartialEq, Eq)]
pub enum InstallMode {
    Monitor,
    Agent,
    MonitorAgent,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Installer: choose Monitor / Agent / Monitor+Agent. Prompts
    /// interactively unless every required flag is passed (used for
    /// scripted/automated installs).
    Install {
        #[arg(long, value_enum)]
        mode: Option<InstallMode>,
        /// Address the Monitor's HTTPS API binds to.
        #[arg(long, default_value = "0.0.0.0:8443")]
        bind_addr: String,
        /// Admin username, created on first install (Monitor / Monitor+Agent).
        #[arg(long)]
        admin_username: Option<String>,
        /// Admin password. If omitted, one is generated and printed once.
        #[arg(long)]
        admin_password: Option<String>,
        /// Monitor URL to pair against (Agent-only mode).
        #[arg(long)]
        monitor_url: Option<String>,
        /// One-time pairing token from `monitor server add` (Agent-only mode).
        #[arg(long)]
        pairing_token: Option<String>,
        /// Display name for this node (Monitor+Agent self-registration).
        #[arg(long)]
        name: Option<String>,
    },
    /// Starts the Monitor API/scheduler and/or the Agent reporting loop,
    /// according to the mode chosen at install time. Long-running.
    Run,
    /// Show the status of this node (Monitor and/or Agent).
    Status,
    /// Manage registered servers.
    Server {
        #[command(subcommand)]
        command: ServerCommand,
    },
    /// Inspect the local Agent.
    Agent {
        #[command(subcommand)]
        command: AgentCommand,
    },
    /// List and manage alert rules.
    Alerts,
    /// Query logs (Agent/Monitor/Docker).
    Logs {
        /// Filter by server name.
        #[arg(long)]
        server: Option<String>,
        /// Filter by log level.
        #[arg(long)]
        level: Option<String>,
    },
    /// Manage incidents.
    Incident {
        #[command(subcommand)]
        command: IncidentCommand,
    },
}

#[derive(Debug, Subcommand)]
pub enum ServerCommand {
    /// List all registered servers.
    List,
    /// Register a new server (local or via SSH in V3).
    Add {
        #[arg(long)]
        name: String,
    },
}

#[derive(Debug, Subcommand)]
pub enum AgentCommand {
    /// Show local Agent status (last heartbeat, connectivity to Monitor).
    Status,
}

#[derive(Debug, Subcommand)]
pub enum IncidentCommand {
    /// List incidents.
    List,
}
