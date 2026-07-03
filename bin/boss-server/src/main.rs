use std::sync::Arc;

use anyhow::Result;
use boss_apiserver::AppState;
use boss_store::{MemoryStorage, StorageBackend};
use clap::Parser;

/// boss-server — control plane (apiserver + storage + scheduler + controllers).
#[derive(Parser)]
#[command(name = "boss-server", version)]
struct Args {
    /// Address to bind the apiserver on.
    #[arg(long, env = "BOSS_BIND", default_value = "127.0.0.1:8080")]
    bind: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    boss_common::log::init();
    let args = Args::parse();

    let storage = Arc::new(StorageBackend::Memory(MemoryStorage::arc()));
    let state = AppState::new(storage.clone());

    // Spawn capability-aware scheduler.
    let scheduler = Arc::new(boss_scheduler::Scheduler::new(storage.clone()));
    tokio::spawn(async move {
        if let Err(e) = scheduler.run().await {
            tracing::error!(error = %e, "scheduler exited");
        }
    });

    // Spawn controller manager reconcilers.
    let cm = Arc::new(boss_controller_manager::ControllerManager::new(storage));
    tokio::spawn(async move {
        if let Err(e) = cm.run().await {
            tracing::error!(error = %e, "controller-manager exited");
        }
    });

    tracing::info!("boss-server starting on {}", args.bind);

    boss_apiserver::serve(&args.bind, state).await?;
    Ok(())
}
