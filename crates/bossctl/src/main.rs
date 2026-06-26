use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

mod client;
mod commands;

use client::ApiClient;

/// bossctl — CLI client for the boss apiserver.
#[derive(Parser, Debug)]
#[command(name = "bossctl", version, about = "CLI for the boss orchestrator")]
struct Cli {
    /// Apiserver address.
    #[arg(long, env = "BOSS_API_SERVER", default_value = "http://127.0.0.1:8080")]
    apiserver: String,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand, Debug)]
enum Cmd {
    /// Apply a resource from a YAML/JSON file (create or replace).
    Apply {
        #[arg(short = 'f', long = "file")]
        file: PathBuf,
    },
    /// Get a single resource or list a kind.
    Get {
        /// Resource kind, e.g. `pods`, `nodes`.
        resource: String,
        /// Optional name. If absent, list all.
        name: Option<String>,
        /// Namespace (defaults to "default").
        #[arg(long, default_value = "default")]
        namespace: String,
    },
    /// Delete a resource.
    Delete {
        resource: String,
        name: String,
        #[arg(long, default_value = "default")]
        namespace: String,
    },
    /// Watch a resource kind (stream events until interrupted).
    Watch {
        resource: String,
        #[arg(long, default_value = "default")]
        namespace: String,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    boss_common::log::init();
    let cli = Cli::parse();
    let client = ApiClient::new(&cli.apiserver);
    match cli.cmd {
        Cmd::Apply { file } => commands::apply(&client, &file).await,
        Cmd::Get {
            resource,
            name,
            namespace,
        } => commands::get(&client, &resource, name.as_deref(), &namespace).await,
        Cmd::Delete {
            resource,
            name,
            namespace,
        } => commands::delete(&client, &resource, &name, &namespace).await,
        Cmd::Watch {
            resource,
            namespace,
        } => commands::watch(&client, &resource, &namespace).await,
    }
    .context("bossctl command failed")
}
