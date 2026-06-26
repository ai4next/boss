.PHONY: help build build-dev test test-verbose check clippy fmt fmt-check clean run-server run-node apply-example get-pods get-example watch-pods delete-example e2e

BOSS_BIND ?= 127.0.0.1:8080
BOSS_API_SERVER ?= http://$(BOSS_BIND)
BOSS_NODE_NAME ?= node-A
EXAMPLE_POD ?= examples/pod.yaml

BOLD := \033[1m
GREEN := \033[0;32m
YELLOW := \033[1;33m
NC := \033[0m

help: ## Show this help message
	@printf "$(BOLD)boss development commands$(NC)\n\n"
	@printf "$(BOLD)Available targets:$(NC)\n"
	@awk 'BEGIN {FS = ":.*?## "} /^[a-zA-Z_-]+:.*?## / {printf "  $(GREEN)%-18s$(NC) %s\n", $$1, $$2}' $(MAKEFILE_LIST)

build: ## Build all workspace crates in release mode
	@printf "$(GREEN)Building release workspace...$(NC)\n"
	cargo build --workspace --release

build-dev: ## Build all workspace crates in debug mode
	@printf "$(GREEN)Building debug workspace...$(NC)\n"
	cargo build --workspace

test: ## Run all tests
	@printf "$(GREEN)Running tests...$(NC)\n"
	cargo test --workspace

test-verbose: ## Run all tests with output
	@printf "$(GREEN)Running tests with output...$(NC)\n"
	cargo test --workspace -- --nocapture

check: ## Check all targets
	@printf "$(GREEN)Checking all targets...$(NC)\n"
	cargo check --workspace --all-targets

clippy: ## Run clippy with warnings as errors
	@printf "$(GREEN)Running clippy...$(NC)\n"
	cargo clippy --workspace --all-targets --all-features -- -D warnings

fmt: ## Format all Rust code
	@printf "$(GREEN)Formatting Rust code...$(NC)\n"
	cargo fmt --all

fmt-check: ## Check Rust formatting
	@printf "$(GREEN)Checking Rust formatting...$(NC)\n"
	cargo fmt --all -- --check

clean: ## Clean build artifacts
	@printf "$(YELLOW)Cleaning build artifacts...$(NC)\n"
	cargo clean

run-server: ## Run the control plane locally
	cargo run -p boss-server -- --bind $(BOSS_BIND)

run-node: ## Run the node agent locally
	cargo run -p boss-node -- --node $(BOSS_NODE_NAME) --apiserver $(BOSS_API_SERVER)

apply-example: ## Apply the example pod
	cargo run -p bossctl -- apply -f $(EXAMPLE_POD)

get-pods: ## List pods
	cargo run -p bossctl -- get pods

get-example: ## Get the example pod
	cargo run -p bossctl -- get pod sleep-pod

watch-pods: ## Watch pod events
	cargo run -p bossctl -- watch pods

delete-example: ## Delete the example pod
	cargo run -p bossctl -- delete pod sleep-pod

e2e: ## Print the local end-to-end command sequence
	@printf "$(BOLD)Run these in separate terminals:$(NC)\n"
	@printf "  make run-server\n"
	@printf "  make run-node\n"
	@printf "  make apply-example get-example\n"
	@printf "  make delete-example\n"
