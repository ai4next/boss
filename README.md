# boss

A sandbox-native orchestration and scheduling system written in Rust
(edition 2024). The project focuses on heterogeneous execution: process,
container, VM, microVM, WASM, and remote sandboxes should share one control
plane without being forced into a single runtime model.

The current implementation has a working control plane, node agent,
capability-aware scheduler, controller loop, Deployment/ReplicaSet reconciliation,
and process runtime path. Container, WASM, and VM/microVM providers are
CLI-backed optional providers (`docker`/`podman`, `wasmtime`, `qemu-system-*`)
and report unhealthy when the local backend is unavailable.

## Workspace layout

```
crates/
  boss-common            # errors, logging, id, time
  boss-api               # pure serde data model (Object<T>, Pod, Node, ...)
  boss-store             # Storage trait + in-memory registry + WatchBus
  boss-apiserver         # axum CRUD + newline-delimited watch + CAS
  boss-scheduler         # capability-aware pod binding to node/provider
  boss-controller-manager# watch-driven workqueue + Deployment/ReplicaSet reconciliation
  bosslet                # node agent: watch -> sync -> runtime -> status -> heartbeat
  boss-runtime           # Runtime trait + process/container/wasm/vm providers
  bossctl                # CLI client (apply / get / delete / watch)
bin/
  boss-server            # control plane: apiserver + storage + scheduler + controller manager
  boss-node              # node: bosslet + RuntimeManager providers
```

Key design choices: a generic `Storage` trait over flat string keys
(`/registry/{type}/{ns}/{name}`), `StorageBackend` enum-dispatch for runtime
polymorphism, optimistic concurrency (CAS) via `metadata.resourceVersion`
enforced in the storage layer, and a `BoxStream` watch with a history ring
buffer for replay.

## Design direction

Boss is intentionally shaped around sandbox-native scheduling rather than
container-only placement. The control plane should understand sandbox class,
artifact type, isolation level, provider capability, startup latency, and
artifact locality as first-class scheduling inputs.

Design docs:

- `docs/index.md`: documentation entrypoint and recommended reading order.
- `docs/design-philosophy.md`: design principles and core decision rules.
- `docs/system-architecture.md`: end-to-end system architecture and data flow.
- `docs/controller-architecture.md`: controller manager, workqueue, reconciler, ownership, and status conditions.
- `docs/multi-sandbox-design.md`: detailed multi-sandbox management design.
- `docs/implementation-roadmap.md`: capability roadmap, acceptance criteria, and test scenarios.
- `docs/beyond-kubernetes-roadmap.md`: strategic direction for a sharper heterogeneous sandbox platform.

## Development workflow

The project includes a small Makefile so the common build, lint, and
local-cluster commands stay discoverable:

```bash
make help
make check
make test
make clippy
make fmt
make build
```

Equivalent raw cargo commands still work:

```bash
cargo check --workspace --all-targets
cargo build --workspace
cargo test --workspace
```

## Run the end-to-end flow

Three terminals (or background the first two):

```bash
# 1. control plane
make run-server

# 2. node agent (registers node-A, reports provider capabilities, watches bound pods)
make run-node

# 3. submit a pod; the scheduler binds it to a capable node/provider
make apply-example
make get-example        # pod phase: Running
make get-pods
make watch-pods         # streaming events
make delete-example     # sandbox stopped, process killed
```

You can override the defaults without editing files:

```bash
BOSS_BIND=127.0.0.1:18080 make run-server
BOSS_API_SERVER=http://127.0.0.1:18080 BOSS_NODE_NAME=node-B make run-node
```

`examples/pod.yaml` runs `sleep 300` via the baremetal runtime. `pgrep -fl
"sleep 300"` shows the host process while the pod is Running, and it is killed
on delete.

## Capability status

- ✅ Workspace, data model, in-memory store, apiserver, CLI, bosslet, and baremetal runtime are end-to-end runnable.
- ✅ Pod sandbox intent, Node runtime capabilities, RuntimeManager provider indexing, scheduler binding, and bosslet provider selection have foundation support.
- ✅ Controller manager has watch-driven workqueues, Deployment/ReplicaSet reconciliation, basic garbage collection, and status conditions.
- ✅ Process, container, WASM, and VM/microVM runtime paths are implemented through local providers.
- ⬜ Production artifact cache, richer scheduling scoring, and HA/security hardening remain future work.
- ⬜ Raft-backed store, multi-replica control plane, leader election, AuthN/AuthZ/RBAC, and policy admission are future production-hardening work.
