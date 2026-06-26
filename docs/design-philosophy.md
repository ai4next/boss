# Boss 设计哲学

Boss 的设计目标是构建一个 sandbox-native orchestration system。它应当把不同执行环境作为一等对象，而不是把非容器 workload 作为边缘插件处理。

## 核心信念

### 1. Sandbox is the unit of execution

Boss 的基本执行单元是 sandbox。sandbox 可以由 process、container、WASM runtime、microVM、VM 或 remote provider 承载。

这意味着上层 API 应表达运行意图，而不是暴露某个 runtime 的内部参数。用户描述的是：

- 要运行什么 artifact。
- 需要什么 isolation。
- 需要什么 network、storage、security 能力。
- 是否有调度偏好或硬性约束。

具体由哪个 Provider 执行，应由 scheduler 和 node agent 根据能力、策略和健康状态共同决定。

### 2. Capability-driven scheduling

Scheduler 不应该假设每个节点能力一致，也不应该硬编码某个 runtime 的存在。

节点必须上报真实能力：

- 支持哪些 `sandboxClass`。
- 支持哪些 artifact types。
- 支持哪些 isolation levels。
- 支持哪些 network/storage modes。
- Provider 是否 healthy。
- Provider 当前容量和依赖是否满足。

Scheduler 只基于这些能力做过滤和评分。节点没有声明的能力，调度器就不应该假设存在。

### 3. Minimal core, extensible edges

Boss core 应保持小而稳定。core 负责跨沙箱类型通用的能力，Provider 负责 runtime-specific 的细节。

Core owns:

- Object model。
- Storage and watch。
- Scheduling pipeline。
- Node capability model。
- Sandbox lifecycle state。
- Normalized status, events, logs, metrics, errors。

Providers own:

- Artifact format details。
- Runtime dependency checks。
- Runtime-specific create/start/stop/remove。
- Runtime-specific logs and metrics collection。
- Local cleanup and garbage collection。

如果一个能力只对单个 Provider 有意义，默认不进入 core API。它应当作为 Provider metadata、capability 或 Provider-specific config 存在。

### 4. Isolation is schedulable

Isolation 不应只是 runtime 选择后的副作用。它应该是 workload intent 的一部分，并且可被 scheduler 理解。

Boss 应支持明确的 isolation levels：

- `sharedHost`: 与宿主共享较多边界，例如本机进程。
- `namespaced`: 使用 namespace/cgroup 形成隔离。
- `runtimeSandboxed`: 使用 WASM 或 userspace sandbox。
- `kernelIsolated`: 使用独立 guest kernel 或等价边界。
- `hardwareVirtualized`: 使用硬件虚拟化边界。

调度规则：

- Workload 可以声明最低 isolation 要求。
- Provider 必须声明自己能提供哪些 isolation levels。
- Scheduler 不能把高隔离 workload 放到低隔离 Provider。
- Bosslet 必须在启动前再次校验 Provider 是否满足要求。

### 5. Failures must be observable and stable

多沙箱系统如果只返回 runtime-specific error string，会很难调试和自动化。

Boss 应把错误归一为稳定 reason：

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

Provider-specific detail 可以放进 message 或 metadata，但顶层 reason 必须稳定，方便 CLI、controller、告警和自动修复逻辑消费。

### 6. Fast path matters

Boss 要适合短任务、边缘任务、函数、插件和强隔离任务。这些场景对 cold start、artifact locality、Provider health 很敏感。

因此调度不能只看 CPU/内存。长期应把以下信号纳入评分：

- Artifact 是否已缓存。
- Provider cold start latency。
- Provider 当前排队深度。
- microVM slot、WASM concurrency 等 Provider-specific capacity。
- 数据源和网络入口距离。

实现上可以先做能力过滤，再逐步引入 scoring 和 reserve，但架构上二者都属于 capability-driven scheduling。

## 进入 Core 的判断标准

一个新能力进入 core 前，应满足至少多数条件：

- 是否适用于多个 sandbox classes？
- 是否能表达为 capability、policy、status 或 artifact metadata？
- Scheduler 是否需要理解它才能做正确决策？
- Bosslet 是否能在启动前校验它？
- 用户是否需要稳定、跨 Provider 的状态或失败原因？

如果答案大多是否，能力应留在 Provider 内部。

## 明确不做

Boss 不应：

- 把所有 workload 强行抽象成 container。
- 让 scheduler 依赖 runtime-specific 分支逻辑。
- 让 apiserver 参与 Provider 选择。
- 让 Provider 私有错误直接泄漏为唯一用户状态。
- 为单个 Provider 的便利污染通用 API。
- 在同一次实现中同时追求真实多 runtime、生产 HA、完整安全策略和复杂调度评分。

## 设计默认值

- 公开入口继续使用 `Pod`，不新增独立 `Workload`。
- `sandboxClass` 是新的主字段。
- `runtimeClass` 是兼容字段。
- 缺省 sandbox class 是 `process`。
- 调度器只信任 `NodeStatus.runtimeCapabilities`。
- Bosslet 尊重 scheduler 选择的 Provider。
- Provider 能力上报必须来自本地初始化和健康检查。
