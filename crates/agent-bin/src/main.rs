#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();
    tracing::info!("vpsmonitor-agent starting (standalone binary, Fase 0 scaffold)");
    println!("Agent collector/reporter loop: not implemented yet (Fase 1)");
}
