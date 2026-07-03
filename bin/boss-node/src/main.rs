use std::sync::Arc;

use anyhow::Result;
use boss_runtime::{
    BareMetalRuntime, ContainerRuntime, RuntimeClass, RuntimeManager, VmRuntime, WasmRuntime,
};
use bosslet::{ApiServerClient, Bosslet};
use clap::Parser;

/// boss-node — node agent (bosslet + runtime).
#[derive(Parser)]
#[command(name = "boss-node", version)]
struct Args {
    /// Name of this node. Must match `spec.nodeName` on pods you want here.
    #[arg(long, env = "BOSS_NODE_NAME")]
    node: String,

    /// Apiserver address.
    #[arg(long, env = "BOSS_API_SERVER", default_value = "http://127.0.0.1:8080")]
    apiserver: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    boss_common::log::init();
    let args = Args::parse();

    let client = ApiServerClient::new(&args.apiserver);

    let runtime = RuntimeManager::new();
    runtime.register(RuntimeClass::BareMetal, Arc::new(BareMetalRuntime::new()));
    runtime.register(RuntimeClass::Container, Arc::new(ContainerRuntime::new()));
    runtime.register_provider(
        Arc::new(VmRuntime::new()),
        vec!["vm".to_string(), "microvm".to_string()],
    );
    runtime.register(RuntimeClass::Wasm, Arc::new(WasmRuntime::new()));

    let bosslet = Arc::new(Bosslet::new(args.node.clone(), client, runtime));
    tracing::info!(node = %args.node, apiserver = %args.apiserver, "boss-node starting");
    bosslet.run().await?;
    Ok(())
}
