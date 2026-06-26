# Boss 系统架构设计

## 总览

Boss 是一个 sandbox-native orchestration system。系统通过统一 API、控制面、调度面和节点代理管理多种沙箱执行环境。

核心架构目标：

- `Pod` 能表达 sandbox intent。
- `Node` 能上报 runtime capabilities。
- `Scheduler` 能基于能力绑定节点和 Provider。
- `Bosslet` 能按选定 Provider 驱动生命周期。
- `Runtime` Provider 暴露统一能力、状态和失败语义。

总体架构：

```text
bossctl / users
      |
      v
boss-apiserver
      |
      v
boss-store  <------------------------------+
      |                                     |
      +--> boss-controller-manager            |
      |        |                            |
      |        v                            |
      |   reconcile Deployments/ReplicaSets |
      |                                     |
      +--> boss-scheduler                   |
      |        |                            |
      |        v                            |
      |   bind Pod: nodeName + provider     |
      |                                     |
      +--> boss-node / bosslet --------------+
               |
               v
          RuntimeManager
               |
     +---------+----------+----------+----------+
     |                    |          |          |
process provider     container    wasm    microvm/vm
```

## Architecture Planes

### User Plane

User Plane 是用户和系统交互的入口。

组件：

- `bossctl`
- YAML/JSON manifests
- SDKs or APIs

职责：

- 提交资源。
- 查询资源。
- watch 资源变化。
- 展示 phase、node、sandbox class、provider、sandbox id、reason/message。

User Plane 不负责调度决策，也不需要知道某个节点本地有哪些 Provider。

### Control Plane

Control Plane 负责资源 API、持久化和事件分发。

组件：

- `boss-apiserver`
- `boss-store`
- `boss-controller-manager`
- `boss-scheduler`

职责：

- 接收 CRUD/list/watch/status 请求。
- 默认 metadata。
- 通过 CAS 保证更新一致性。
- watch store 变化并驱动调度和控制器。
- 将期望状态写回资源对象。

Control Plane 不直接创建沙箱，也不调用 runtime。

### Controller Plane

Controller Plane 将声明式资源持续收敛到期望状态。它负责创建和维护下游对象，但不直接运行 sandbox，也不做节点绑定。

组件：

- `boss-controller-manager`
- `Reflector`
- `LocalCache`
- `WorkQueue`
- `Reconciler`

职责：

- list-watch `Deployment`, `ReplicaSet`, `Pod` 等资源。
- 将资源事件转换为 reconcile key。
- 通过幂等 reconcile 创建、更新或删除下游资源。
- 使用 `ownerReferences` 维护 `Deployment -> ReplicaSet -> Pod` 所有权链。
- 汇总 observed state 并写入 status/conditions。
- 通过 CAS update 处理并发写冲突。

Controller Plane 不选择 Node 或 Provider。Pod template 中的 sandbox intent 必须原样传递给 Pod，由 Scheduling Plane 和 Node Plane 继续处理。

详细设计见 [控制器架构设计](controller-architecture.md)。

### Scheduling Plane

Scheduling Plane 将未绑定的 workload 绑定到合适节点。

组件：

- `boss-scheduler`
- Filter plugins
- Score plugins
- Reserve/Permit/Bind phases

职责：

- watch unbound Pods。
- 解析目标 `sandboxClass`。
- 读取 Nodes 和 `runtimeCapabilities`。
- 过滤不满足能力的节点。
- 写入 `spec.nodeName`。
- 写入 selected Provider annotation。

当前不直接负责：

- CPU/内存资源解析。
- 多维评分。
- 资源预留。
- 抢占。
- 多调度器 profile。

### Node Plane

Node Plane 负责本节点状态和本节点沙箱生命周期。

组件：

- `boss-node`
- `bosslet`
- `RuntimeManager`

职责：

- 初始化本地 Providers。
- 注册 Node。
- 周期性 heartbeat。
- 上报 Provider capabilities。
- watch 绑定到本节点的 Pods。
- 调用 runtime Provider 创建、启动、停止、删除沙箱。
- 上报 Pod status。

Node Plane 不应该做全局调度决策。

### Runtime Plane

Runtime Plane 封装具体执行技术。

组件：

- `boss-runtime`
- `BareMetalRuntime`
- container runtime Provider
- wasm runtime Provider
- microVM/VM Provider
- remote Provider

职责：

- 声明 runtime capabilities。
- 执行 sandbox lifecycle。
- 查询 sandbox status。
- 输出 logs/metrics。
- 将 runtime-specific failure 映射为标准 reason。

Provider 可以有复杂内部实现，但对上层必须提供稳定合同。

### Storage Plane

Storage Plane 负责资源状态存储和 watch。

组件：

- `Storage` trait
- `MemoryStorage`
- raft-backed storage
- `WatchBus`

职责：

- 保存对象 JSON。
- 分配 `resourceVersion`。
- 执行 CAS update。
- 支持 prefix list。
- 支持 watch replay。

当前实现使用 in-memory storage；目标架构允许替换为 raft-backed storage 或其他强一致后端。

### Observability Surface

Observability Surface 是跨平面状态输出。

核心输出：

- Pod phase。
- Pod reason/message。
- Sandbox id。
- Sandbox class。
- Selected provider。
- Node Ready condition。
- Provider healthy/reason。

扩展输出：

- Runtime logs。
- Runtime metrics。
- Provider queue depth。
- Artifact cache hit/miss。
- Scheduling decision trace。

## Public API Contracts

### Pod Intent

`PodSpec` 保留现有字段，同时新增 sandbox intent：

```yaml
spec:
  sandboxClass: wasm
  sandbox:
    artifact:
      type: wasmModule
      uri: oci://example.com/app.wasm:1.0.0
    isolation: runtimeSandboxed
    network:
      mode: none
```

兼容规则：

```text
sandboxClass > runtimeClass > process
```

兼容映射：

```text
baremetal -> process
container -> container
wasm -> wasm
vm -> vm
microvm -> microvm
firecracker -> microvm
```

### Node Runtime Capabilities

`NodeStatus` 新增 `runtimeCapabilities`：

```yaml
status:
  runtimeCapabilities:
    providers:
      - name: baremetal
        healthy: true
        version: "0.1.0"
        classes: ["process"]
        artifactTypes: ["executable"]
        networkModes: ["host", "none"]
        isolationLevels: ["sharedHost"]
      - name: wasmtime
        healthy: false
        reason: "NotConfigured"
        classes: ["wasm"]
        artifactTypes: ["wasmModule", "ociWasm"]
        networkModes: ["none"]
        isolationLevels: ["runtimeSandboxed"]
```

调度器只信任节点上报的 `runtimeCapabilities`。

### Scheduler Binding

Scheduler bind 时写入：

```yaml
spec:
  nodeName: node-a
metadata:
  annotations:
    boss.io/resolved-sandbox-class: wasm
    boss.io/selected-provider: wasmtime
```

Bosslet 必须优先使用 `boss.io/selected-provider`。

### Runtime Contract

`Runtime` trait 提供：

```rust
async fn capabilities(&self) -> RuntimeCapabilities;
```

生命周期接口保留：

```rust
create()
start()
stop()
remove()
status()
list()
```

这样可以先建立能力模型，不强行一次重写所有 Provider。

## Core Data Flows

### Apply Flow

```text
bossctl apply
  -> apiserver create/update
  -> store CAS write
  -> watch event published
```

### Scheduling Flow

```text
scheduler watches Pods
  -> find unbound Pod
  -> resolve sandbox class
  -> list Nodes
  -> filter Ready nodes with healthy matching Provider
  -> choose stable candidate
  -> update Pod nodeName + selected-provider annotation
```

### Node Execution Flow

```text
bosslet watches Pods
  -> ignore Pods not bound to this node
  -> read selected-provider annotation
  -> find local Provider
  -> build SandboxSpec
  -> provider.create()
  -> provider.start()
  -> update PodStatus Running
```

### Status Flow

```text
bosslet polling loop
  -> provider.status()
  -> running=true keeps Running
  -> running=false and exit_code=0 => Succeeded
  -> running=false and exit_code!=0 => Failed
  -> update PodStatus reason/message
```

### Delete Flow

```text
apiserver delete Pod
  -> store emits Deleted watch event
  -> bosslet receives event
  -> provider.stop()
  -> provider.remove()
  -> local state cleanup
```

## Crate Boundaries

### `boss-api`

Owns public resource types and serde contracts.

Should contain:

- `PodSpec`
- `PodStatus`
- `NodeStatus`
- sandbox intent structs
- runtime capability structs shared through API

Should not contain:

- runtime implementation logic
- scheduler algorithms
- provider-specific details

### `boss-apiserver`

Owns HTTP routing and generic resource operations.

Should contain:

- CRUD/list/watch/status routes
- metadata defaulting
- URL-to-resource consistency
- response mapping

Should not contain:

- scheduling
- Provider selection
- runtime calls

### `boss-store`

Owns storage consistency and watch.

Should contain:

- `Storage` trait
- memory backend
- watch bus
- resourceVersion handling

Should not contain:

- API-specific business rules
- scheduler-specific indexing

### `boss-controller-manager`

Owns declarative resource reconciliation.

Should contain:

- reflectors and local cache
- workqueue and retry policy
- Deployment reconciler
- ReplicaSet reconciler
- status/condition aggregation
- ownerReference-based cleanup hooks

Should not contain:

- node/provider placement rules
- runtime lifecycle operations
- provider-specific behavior

### `boss-scheduler`

Owns global placement decisions.

Should contain:

- pending Pod watch loop
- filter pipeline
- score pipeline
- bind logic
- scheduling event reason

Should not contain:

- runtime lifecycle operations
- node-local Provider initialization

### `bosslet`

Owns node-local reconciliation.

Should contain:

- node registration
- heartbeat
- capability reporting
- bound Pod watch
- Provider selection
- Pod status reporting
- local sandbox cleanup

Should not contain:

- global placement decisions
- API storage internals

### `boss-runtime`

Owns Provider contract and shared runtime types.

Should contain:

- `Runtime` trait
- `RuntimeManager`
- `SandboxSpec`
- `SandboxStatus`
- `RuntimeCapabilities`
- built-in Provider implementations

Should not contain:

- apiserver routes
- scheduler binding logic
- CLI presentation

### `bossctl`

Owns user-facing command-line workflows.

Should contain:

- apply/get/delete/watch
- readable summaries
- logs/describe commands

Should not contain:

- scheduling rules
- Provider lifecycle logic

## Standard Failure Reasons

Boss should normalize common failures:

- `UnsupportedClass`
- `UnsupportedArtifact`
- `IsolationMismatch`
- `ProviderUnavailable`
- `ArtifactFetchFailed`
- `InvalidSpec`
- `InsufficientResources`
- `PermissionDenied`
- `CreateFailed`
- `StartFailed`
- `StatusUnknown`

Provider-specific details go into `message` or structured metadata. Top-level `reason` stays stable.

## Architecture Boundaries

核心架构包括：

- sandbox intent API
- runtime capabilities API
- Provider capabilities
- node capability reporting
- capability-aware scheduler filter
- selected-provider annotation
- controller reconcile loop
- Deployment/ReplicaSet ownership chain
- bosslet Provider selection
- CLI visibility improvements

核心组件不直接负责：

- controller-specific rollout policy
- real container runtime
- real WASM runtime
- real microVM runtime
- resource scoring
- artifact cache
- RBAC
- admission policy
- multi-replica control plane
- raft-backed storage

## Extension Direction

目标架构继续扩展：

- `SandboxClass` resource
- `SandboxProfile` resource
- `ArtifactResolver`
- resource request parsing
- filter/score/reserve scheduling
- real WASM Provider
- real container Provider
- optional microVM Provider
- logs and metrics API
- controller manager reconcilers
- raft-backed storage
