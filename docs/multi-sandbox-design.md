# 多类型沙箱管理设计

本文定位：这是 Boss 多沙箱模型的深入设计文档，重点展开 SandboxClass、SandboxProvider、WorkloadIntent、调度、节点能力、状态、日志、指标和安全模型。阅读系统总览请先看 [系统架构设计](system-architecture.md)，阅读控制器设计请看 [控制器架构设计](controller-architecture.md)，阅读实现顺序请看 [实施路线](implementation-roadmap.md)。

## 背景与目标

当前项目已经具备控制面、节点代理、存储、命令行工具与基础运行时接口，可以用 baremetal 运行时完成一个端到端 Pod 流程。下一阶段需要把系统从“单一进程型沙箱”扩展为“多类型沙箱统一管理平台”，让同一套 API、调度、节点代理和状态回报流程可以管理进程、容器、虚拟机、微虚拟机、WASM、远程执行环境等更多沙箱形态。

设计目标：

- 统一抽象：上层 API 不直接绑定某一种执行技术，通过稳定的沙箱抽象表达工作负载意图。
- 多运行时并存：同一个节点可以注册多个沙箱 Provider，不同节点可以暴露不同能力。
- 可调度：控制面能够基于沙箱类型、资源需求、隔离等级、镜像/模块格式和节点能力选择合适节点。
- 可观测：所有沙箱类型输出一致的状态、事件、日志、退出码和资源使用信息。
- 可演进：新增沙箱类型时尽量只新增 Provider 与能力声明，不要求重写调度器和节点代理核心流程。
- 向后兼容：现有 `runtimeClass`、`RuntimeManager`、`SandboxSpec`、`SandboxStatus` 和 baremetal 流程继续可用。

## 范围

本设计覆盖：

- API 数据模型如何表达更多沙箱类型。
- 节点如何声明自身可运行的沙箱能力。
- 调度器如何基于沙箱需求匹配节点。
- 节点代理如何选择 Provider 并驱动生命周期。
- 运行时 Provider 如何实现统一接口。
- 状态、事件、错误、日志和资源指标如何标准化。
- 从当前代码结构迁移到多沙箱管理的分阶段路径。

本设计暂不覆盖：

- 多租户认证授权的完整方案。
- 生产级镜像仓库、制品分发和密钥管理。
- 跨集群联邦调度。
- 具体某个 Provider 的完整生产实现细节。

## 核心概念

### SandboxClass

`SandboxClass` 表示一种可调度、可运行的沙箱类别。它不只是当前的 `runtimeClass` 字符串，而是包含隔离能力、制品格式、资源模型和 Provider 选择策略的完整对象。

建议内置类别：

| 类别 | 适用场景 | 隔离强度 | 典型 Provider |
| --- | --- | --- | --- |
| `process` | 本机进程、开发测试、轻量任务 | 低 | baremetal |
| `container` | Linux 容器、常规服务 | 中 | containerd / runc |
| `microvm` | 强隔离短任务、多租户工作负载 | 高 | firecracker / cloud-hypervisor |
| `vm` | 完整虚拟机、专用内核、特殊系统依赖 | 高 | qemu / libvirt |
| `wasm` | 函数、插件、边缘任务、快速冷启动 | 中高 | wasmtime / wasmedge |
| `remote` | 外部执行池、云端任务、专有平台 | 取决于远端 | custom remote provider |

`runtimeClass` 可以作为向后兼容字段继续存在，但内部建议解析为 `SandboxClassRef`。

### SandboxProvider

`SandboxProvider` 是节点本地真正执行沙箱生命周期的组件。每个 Provider 必须实现统一的 `Runtime` trait，并上报自身能力。

Provider 需要回答三个问题：

- 能不能运行：支持哪些 `SandboxClass`、平台、制品类型、网络模式、存储模式和资源类型。
- 如何运行：根据 `SandboxSpec` 创建、启动、停止、删除和查询状态。
- 当前状态：本地已有沙箱、资源占用、健康状态和失败原因。

### WorkloadIntent

`WorkloadIntent` 是用户提交的运行意图，对应当前 `PodSpec` 的增强方向。它描述“想运行什么”，不描述“由哪个具体 Provider 运行”。

建议包含：

- `sandboxClass`: 期望沙箱类别，例如 `container`、`microvm`、`wasm`。
- `isolation`: 隔离要求，例如 `shared-host`、`namespaced`、`kernel-isolated`、`hardware-virtualized`。
- `artifact`: 制品描述，例如镜像、可执行文件、WASM 模块、磁盘镜像、远程任务模板。
- `resources`: CPU、内存、磁盘、GPU、网络带宽、临时存储等资源请求与限制。
- `network`: 网络模式，例如 `host`、`none`、`nat`、`bridge`、`tap`、`overlay`。
- `storage`: 挂载、临时盘、只读资源、持久卷声明。
- `security`: 权限、特权模式、系统调用策略、设备访问、密钥引用。
- `scheduling`: 节点选择、亲和性、容忍度、地域、硬件能力偏好。

## API 模型设计

### 保留现有 Pod 入口

短期建议继续使用当前 `PodSpec` 作为用户入口，避免大规模 API 破坏。新增字段时保持可选：

```yaml
apiVersion: boss/v1
kind: Pod
metadata:
  namespace: default
  name: demo-wasm
spec:
  sandboxClass: wasm
  runtimeClass: wasm
  containers:
    - name: app
      image: ghcr.io/example/app:latest
      wasmModule: oci://ghcr.io/example/app.wasm:1.0.0
  resources:
    requests:
      cpu: "100m"
      memory: "128Mi"
  sandbox:
    isolation: namespaced
    artifact:
      type: wasm-module
      uri: oci://ghcr.io/example/app.wasm:1.0.0
    network:
      mode: none
    security:
      allowHostAccess: false
```

兼容规则：

- 如果只有 `runtimeClass`，按旧逻辑解析。
- 如果同时存在 `sandboxClass` 与 `runtimeClass`，优先使用 `sandboxClass`，并校验二者不冲突。
- 如果二者都缺省，默认使用节点可用的 `process` 或当前 `baremetal` 行为。

### 新增 SandboxProfile

`SandboxProfile` 用于复用一组沙箱运行策略，避免每个 Pod 重复填写安全、网络、存储和资源默认值。

示例：

```yaml
apiVersion: boss/v1
kind: SandboxProfile
metadata:
  namespace: default
  name: secure-microvm
spec:
  class: microvm
  isolation: hardware-virtualized
  defaults:
    network:
      mode: nat
    security:
      allowPrivilegeEscalation: false
      readonlyRootfs: true
    resources:
      requests:
        cpu: "1"
        memory: "512Mi"
```

Pod 可引用：

```yaml
spec:
  sandboxProfile: secure-microvm
```

解析顺序：

1. 读取 `SandboxProfile` 默认值。
2. 合并 Pod 自身字段。
3. 校验沙箱类型、制品类型和节点能力是否匹配。
4. 生成内部 `ResolvedSandboxSpec`。

### 新增 RuntimeClass / SandboxClass 资源

建议把硬编码枚举逐步迁移为可配置资源：

```yaml
apiVersion: boss/v1
kind: SandboxClass
metadata:
  name: microvm
spec:
  displayName: Micro VM
  isolationLevel: hardware-virtualized
  artifactTypes:
    - container-image
    - rootfs-image
    - kernel-image
  networkModes:
    - none
    - nat
    - tap
  providerSelector:
    matchLabels:
      boss.io/provider-family: firecracker
  scheduling:
    requiredNodeCapabilities:
      - virtualization.kvm
      - network.tap
```

收益：

- 新增沙箱类型不需要修改核心枚举。
- 调度器可以通过声明式能力做匹配。
- 节点和 Provider 的能力可以版本化与灰度发布。

## 节点能力模型

每个节点需要在 `NodeStatus` 中上报 Provider 能力。建议新增 `runtimeCapabilities` 字段：

```yaml
status:
  runtimeCapabilities:
    providers:
      - name: baremetal
        classes: [process]
        healthy: true
        version: 0.1.0
        artifactTypes: [executable]
        networkModes: [host, none]
        isolationLevels: [shared-host]
      - name: wasmtime
        classes: [wasm]
        healthy: true
        version: 18.0.0
        artifactTypes: [wasm-module, oci-wasm]
        networkModes: [none]
        isolationLevels: [sandboxed]
      - name: firecracker
        classes: [microvm]
        healthy: false
        reason: MissingKvm
        artifactTypes: [container-image, rootfs-image]
        networkModes: [none, nat, tap]
        isolationLevels: [hardware-virtualized]
    resources:
      cpu: "16"
      memory: "64Gi"
      ephemeralStorage: "500Gi"
      devices:
        - name: /dev/kvm
          type: kvm
          available: true
```

节点代理启动时：

1. 加载本地 Provider 配置。
2. 初始化每个 Provider。
3. 调用 Provider 的 `capabilities()`。
4. 将能力合并到 `NodeStatus`。
5. 周期性更新健康状态和资源可用量。

## 调度设计

调度器输入：

- 待调度 Pod。
- 解析后的 `ResolvedSandboxSpec`。
- 所有 Node 的 `runtimeCapabilities`、资源状态和标签。
- `SandboxClass` / `SandboxProfile` 约束。

调度流程建议分为 Filter、Score、Reserve、Bind 四个阶段。

### Filter

过滤不满足硬性条件的节点：

- 节点没有健康 Provider 支持目标 `sandboxClass`。
- Provider 不支持目标制品类型。
- Provider 不支持目标网络模式、存储模式或安全要求。
- 节点资源不足。
- 节点缺少必要设备，例如 `/dev/kvm`、GPU、特定内核模块。
- 节点标签、选择器、污点容忍不匹配。

### Score

对候选节点评分：

- 资源余量更合理的节点得分更高。
- Provider 冷启动更快的节点得分更高。
- 已缓存目标镜像、模块或 rootfs 的节点得分更高。
- 更接近数据源或网络入口的节点得分更高。
- 对高隔离任务，优先选择专用池或低负载节点。
- 对短任务，优先选择启动延迟更低的 Provider。

### Reserve

绑定前预留资源，降低并发调度导致的超卖：

- 内存、CPU、磁盘、设备数量。
- 特定 Provider 并发槽位，例如同时运行 microvm 数量。
- 镜像拉取带宽和临时磁盘空间。

### Bind

将 Pod 的 `spec.nodeName` 写入目标节点，并可追加调度结果注解：

```yaml
metadata:
  annotations:
    boss.io/selected-provider: firecracker
    boss.io/resolved-sandbox-class: microvm
    boss.io/scheduling-reason: matched secure microvm profile
```

## 节点代理设计

节点代理负责把已绑定到本节点的 Pod 转换为具体 Provider 操作。

### Provider 选择

节点代理收到 Pod 后：

1. 解析 `sandboxClass` / `runtimeClass`。
2. 合并 `SandboxProfile`。
3. 根据调度注解中的 `selected-provider` 优先选择 Provider。
4. 如果没有指定 Provider，则在本地支持该 class 的健康 Provider 中选择默认 Provider。
5. 构建 `SandboxSpec`。
6. 调用 Provider 生命周期接口。

### 生命周期状态机

建议统一状态机：

```text
Pending -> Preparing -> Created -> Starting -> Running -> Stopping -> Exited -> Removed
                    \-> Failed
```

状态说明：

- `Preparing`: 拉取镜像、下载模块、创建 rootfs、准备网络和存储。
- `Created`: 沙箱元数据已创建，但入口进程未启动。
- `Starting`: Provider 正在启动执行环境。
- `Running`: 工作负载正在运行。
- `Stopping`: 正常停止或强制停止中。
- `Exited`: 工作负载已退出，保留状态和日志。
- `Removed`: 本地资源已清理。
- `Failed`: 生命周期任意阶段失败。

当前 `PodPhase` 可以继续作为用户可见摘要，内部新增更细粒度 `SandboxState`。

### 幂等要求

所有 Provider 操作必须尽量幂等：

- `create(spec)`：相同 Pod UID 重复创建时返回已有沙箱 ID 或明确冲突。
- `start(id)`：已运行时返回成功。
- `stop(id, force)`：已停止时返回成功。
- `remove(id)`：不存在时返回成功或 `NotFound`，由节点代理统一视为已清理。
- `status(id)`：必须能区分 `NotFound`、`Exited`、`Running`、`Unknown`。

## Runtime Trait 演进

当前 trait 已包含 `create/start/stop/remove/status/list`。建议演进为：

```rust
#[async_trait]
pub trait Runtime: Send + Sync {
    fn name(&self) -> &'static str;
    async fn capabilities(&self) -> RuntimeCapabilities;
    async fn prepare(&self, spec: &SandboxSpec) -> RuntimeResult<PreparedSandbox>;
    async fn create(&self, spec: SandboxSpec) -> RuntimeResult<SandboxId>;
    async fn start(&self, id: &SandboxId) -> RuntimeResult<()>;
    async fn stop(&self, id: &SandboxId, grace: StopOptions) -> RuntimeResult<()>;
    async fn remove(&self, id: &SandboxId) -> RuntimeResult<()>;
    async fn status(&self, id: &SandboxId) -> RuntimeResult<SandboxStatus>;
    async fn logs(&self, id: &SandboxId, options: LogOptions) -> RuntimeResult<LogStream>;
    async fn metrics(&self, id: &SandboxId) -> RuntimeResult<SandboxMetrics>;
    async fn list(&self) -> RuntimeResult<Vec<SandboxSummary>>;
}
```

其中：

- `capabilities()` 用于节点能力上报。
- `prepare()` 用于可重试的制品与环境准备。
- `logs()` 提供统一日志读取。
- `metrics()` 提供统一资源指标。
- `StopOptions` 替代简单 `force: bool`，表达优雅退出时间、信号和强杀策略。

为了兼容当前实现，可以先新增扩展 trait：

```rust
pub trait RuntimeCapabilitiesProvider {
    async fn capabilities(&self) -> RuntimeCapabilities;
}
```

再逐步合并进主 trait。

## SandboxSpec 演进

当前 `SandboxSpec` 已包含 `pod_uid`、`runtime_class`、`command`、`args`、`env`、`image`、`wasm_module`、`network` 等字段。建议拆成更通用的结构：

```rust
pub struct SandboxSpec {
    pub identity: SandboxIdentity,
    pub class: SandboxClassRef,
    pub artifact: SandboxArtifact,
    pub entrypoint: Entrypoint,
    pub env: Vec<(String, String)>,
    pub resources: SandboxResources,
    pub network: SandboxNetwork,
    pub storage: SandboxStorage,
    pub security: SandboxSecurity,
    pub metadata: BTreeMap<String, String>,
}
```

关键子结构：

```rust
pub enum SandboxArtifact {
    Executable { path: String },
    ContainerImage { image: String, pull_policy: PullPolicy },
    WasmModule { uri: String },
    RootFsImage { uri: String },
    DiskImage { uri: String },
    RemoteTemplate { provider: String, template: String },
}
```

这样可以避免在通用结构里不断新增 `wasm_module`、`vm_image`、`rootfs` 等类型专属字段。

## Provider 类型设计

### Process Provider

用于本机进程执行，适合开发、测试和可信任务。

能力：

- 支持 `Executable` 或 `command + args`。
- 支持 `host` / `none` 网络模式。
- 可提供基础 stdout/stderr 日志。
- 资源隔离较弱，可后续接入 cgroup 限制。

限制：

- 不适合不可信多租户。
- 文件系统与网络隔离有限。

### Container Provider

用于 OCI 镜像和容器运行时。

能力：

- 支持 `ContainerImage`。
- 支持镜像拉取、rootfs、namespace、cgroup。
- 支持常规网络和挂载模式。
- 可提供容器级日志与资源指标。

关键点：

- Provider 内部屏蔽 containerd、runc 等具体实现差异。
- `SandboxStatus.id` 建议使用稳定前缀，例如 `container://...`。
- 镜像拉取失败、认证失败和启动失败需要标准化错误原因。

### MicroVM Provider

用于强隔离轻量虚拟机。

能力：

- 支持 `ContainerImage` 转 rootfs 或 `RootFsImage`。
- 支持独立 kernel、rootfs、tap/nat 网络。
- 支持更高隔离等级和较低冷启动时间。

关键点：

- 节点必须具备 KVM 或等价虚拟化能力。
- 需要限制单节点并发启动数量，避免 IO 和内存尖峰。
- 日志和退出码需要从 guest agent、串口或 Provider shim 统一采集。

### VM Provider

用于完整虚拟机或长生命周期系统环境。

能力：

- 支持 `DiskImage`、`RootFsImage`。
- 支持固定 vCPU、内存、磁盘、设备透传。
- 支持更复杂的网络和存储挂载。

关键点：

- 启动时间长，调度评分应考虑冷启动成本。
- 适合长期运行或特殊内核依赖任务。
- 状态回报可能依赖 guest agent。

### WASM Provider

用于 WASM 模块和轻量函数。

能力：

- 支持 `WasmModule` 和 OCI 分发的 WASM 制品。
- 快速启动、低资源占用。
- 默认无网络、无宿主文件访问。
- 可按 capability 授权网络、目录、环境变量。

关键点：

- 需要明确 WASI 版本和运行时能力。
- 安全策略应默认拒绝宿主访问。
- `command/args/env` 映射到 WASI 启动参数。

### Remote Provider

用于接入外部执行系统。

能力：

- 将本地 Pod 转换为远端任务。
- 同步远端任务状态、日志和退出码。
- 支持异构资源池。

关键点：

- 本地 `SandboxId` 需要映射远端任务 ID。
- 网络、存储和安全能力由远端声明。
- 需要处理远端 API 超时、重试、幂等和最终一致性。

## 状态与错误标准化

### SandboxStatus

建议扩展为：

```rust
pub struct SandboxStatus {
    pub id: SandboxId,
    pub class: SandboxClassRef,
    pub provider: String,
    pub state: SandboxState,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub exit_code: Option<i32>,
    pub reason: Option<String>,
    pub message: Option<String>,
    pub addresses: Vec<SandboxAddress>,
    pub resources: Option<SandboxResourceUsage>,
}
```

用户可见的 `PodStatus.phase` 由 `SandboxState` 汇总：

| SandboxState | PodPhase |
| --- | --- |
| Pending / Preparing / Created / Starting | Pending |
| Running | Running |
| Exited 且 exit_code = 0 | Succeeded |
| Failed 或 Exited 且 exit_code != 0 | Failed |
| Unknown | Unknown |

### 错误分类

Provider 错误建议标准化为：

- `UnsupportedClass`：Provider 不支持该沙箱类别。
- `UnsupportedArtifact`：制品类型不支持。
- `ArtifactFetchFailed`：镜像、模块或磁盘下载失败。
- `InvalidSpec`：用户请求无法转换为有效沙箱规格。
- `InsufficientResources`：本地资源不足。
- `PermissionDenied`：安全策略或宿主权限不足。
- `ProviderUnavailable`：Provider 未初始化、依赖缺失或不健康。
- `CreateFailed`、`StartFailed`、`StopFailed`、`RemoveFailed`：生命周期失败。
- `StatusUnknown`：无法确认沙箱状态。

节点代理将错误写入 Pod condition：

```yaml
status:
  conditions:
    - type: SandboxReady
      status: "False"
      reason: ArtifactFetchFailed
      message: failed to fetch wasm module
```

## 日志与指标

### 日志

所有 Provider 需要输出统一日志流：

- `bossctl logs pod-name` 不关心底层沙箱类型。
- 日志记录至少包含时间、流类型、内容和沙箱 ID。
- 对 VM/microvm，可通过串口、guest agent 或 sidecar shim 采集。
- 对 remote provider，可代理远端日志接口。

### 指标

建议统一最小指标集：

- CPU 使用量。
- 内存工作集。
- 磁盘读写字节。
- 网络收发字节。
- 启动耗时。
- 重启次数。
- Provider 错误计数。

指标用途：

- 节点状态展示。
- 调度评分。
- 自动恢复和故障诊断。
- 后续扩展自动伸缩。

## 安全设计

多类型沙箱的安全能力差异很大，必须用声明式能力和默认安全策略约束。

建议原则：

- 默认最小权限。
- 不同沙箱类型明确隔离等级。
- 用户请求的安全能力必须被 Provider 明确支持。
- 高风险能力需要显式字段，例如特权模式、宿主目录挂载、设备访问、host 网络。
- 调度器不把高隔离需求调度到低隔离 Provider。

示例安全字段：

```yaml
sandbox:
  security:
    isolation: hardware-virtualized
    privileged: false
    readonlyRootfs: true
    allowHostNetwork: false
    hostDevices:
      - type: gpu
        count: 1
```

## 存储与制品分发

不同沙箱类型使用不同制品：

- Process：本地可执行文件、脚本、命令。
- Container：OCI 镜像。
- WASM：WASM 模块或 OCI WASM 制品。
- MicroVM：rootfs、kernel、initrd、容器镜像转换产物。
- VM：磁盘镜像、cloud-init 配置。
- Remote：远端任务模板。

建议引入 `ArtifactResolver`：

```text
Pod Artifact URI -> ArtifactResolver -> Local Prepared Artifact -> Provider
```

职责：

- 认证和下载。
- 校验摘要和签名。
- 缓存和垃圾回收。
- 将通用 URI 转换为 Provider 可用路径。
- 记录缓存命中率供调度器评分使用。

## 配置设计

节点本地配置示例：

```toml
[node]
name = "node-a"

[providers.baremetal]
enabled = true
classes = ["process"]

[providers.wasmtime]
enabled = true
classes = ["wasm"]
max_concurrency = 100
allowed_wasi_dirs = []

[providers.firecracker]
enabled = true
classes = ["microvm"]
kernel_path = "/var/lib/boss/kernels/vmlinux"
max_concurrency = 20
network_mode = "tap"

[artifact_cache]
root = "/var/lib/boss/artifacts"
max_size = "100Gi"
```

运行时注册流程：

```rust
let runtime = RuntimeManager::new();
runtime.register("process", Arc::new(BareMetalRuntime::new()));
runtime.register("wasm", Arc::new(WasmRuntime::new(config.wasmtime))); 
runtime.register("microvm", Arc::new(FirecrackerRuntime::new(config.firecracker)));
```

长期建议让 `RuntimeManager` 支持按 Provider 名称和 class 双索引：

- `provider(name)`：按具体 Provider 选择。
- `providers_for_class(class)`：返回支持某 class 的 Provider 列表。
- `default_provider_for_class(class)`：返回默认 Provider。

## API Server 与存储影响

短期新增资源类型：

- `SandboxClass`
- `SandboxProfile`
- 可选 `Sandbox` 或 `SandboxInstance`，用于显式记录节点本地实例状态。

如果不新增 `SandboxInstance`，状态仍可挂在 `PodStatus` 下，改动较小。

推荐路径：

- 第一阶段只扩展 `PodSpec`、`NodeStatus` 和 runtime spec。
- 第二阶段新增 `SandboxClass`、`SandboxProfile`。
- 第三阶段再考虑 `SandboxInstance`，用于更强的可观测性和故障恢复。

## CLI 影响

`bossctl` 建议新增能力：

```bash
bossctl get sandboxclasses
bossctl describe node node-a
bossctl get pods -o wide
bossctl logs pod-name
bossctl exec pod-name --runtime-shell
bossctl explain sandboxclass wasm
```

`get pods -o wide` 可展示：

```text
NAME        PHASE     NODE     CLASS      PROVIDER      SANDBOX-ID
app-wasm    Running   node-a   wasm       wasmtime      wasm://abc
app-vm      Running   node-b   microvm    firecracker   microvm://def
```

## 能力落地顺序

### 统一模型增强

- 在 `boss-api` 中为 `PodSpec` 增加可选 `sandboxClass` 与 `sandbox` 字段。
- 在 `boss-runtime` 中新增 `RuntimeCapabilities`、`SandboxArtifact`、`SandboxState`。
- 保持现有 `runtimeClass` 兼容。
- 在节点启动时上报本地 Provider 能力。
- 文档和 examples 增加 process、wasm、container、microvm 示例。

交付结果：API 能表达更多沙箱类型，节点能展示自身能力。

### Provider 选择与调度匹配

- `RuntimeManager` 支持多 Provider 注册和能力查询。
- 节点代理根据 `sandboxClass` 和调度注解选择 Provider。
- 调度器增加基于 Provider 能力的 Filter。
- Pod 状态增加 `SandboxReady` condition。

交付结果：不同沙箱类型可以被调度到具备能力的节点。

### 制品准备与状态标准化

- 引入 `ArtifactResolver`。
- Provider 增加 `prepare()` 或节点代理统一准备制品。
- 统一 `SandboxState` 到 `PodPhase` 的映射。
- 增加标准错误 reason。
- 增加基本日志接口。

交付结果：不同制品类型可以被下载、缓存、运行，并输出一致状态。

### 真实 Provider 实现

- 完成 container Provider。
- 完成 wasm Provider。
- 完成 microvm Provider 的最小可运行路径。
- 保留 baremetal/process Provider 用于开发和测试。

交付结果：系统具备实际多沙箱运行能力。

### 高级能力

- `SandboxProfile` 和 `SandboxClass` 资源化。
- 引入资源预留和 Provider 并发槽位。
- 增加日志、指标、事件和垃圾回收。
- 增加安全策略校验。

交付结果：具备更稳定的生产化管理能力。

## 与当前代码的映射

当前模块可以这样演进：

| 当前模块 | 建议职责扩展 |
| --- | --- |
| `boss-api` | 增加 `SandboxClass`、`SandboxProfile`、`SandboxSpec` API 字段和 `NodeRuntimeCapabilities` |
| `boss-runtime` | 抽象 artifact、capabilities、state、logs、metrics，多 Provider 注册 |
| `bosslet` | 解析沙箱需求、选择 Provider、上报 Provider 能力、统一状态机 |
| `boss-controller-manager` | 增加 list-watch、workqueue、Deployment/ReplicaSet reconciler 和状态汇总 |
| `boss-scheduler` | 增加基于沙箱能力的 Filter/Score/Reserve |
| `boss-apiserver` | 支持新增资源的 CRUD/watch |
| `bossctl` | 展示沙箱类型、Provider、日志和节点能力 |
| `bin/boss-node` | 从配置加载并注册多个 Provider |

## 关键设计决策

1. `runtimeClass` 保留但降级为兼容字段，长期以 `sandboxClass` 为主。
2. Provider 能力必须由节点上报，调度器不硬编码节点能运行什么。
3. `SandboxArtifact` 使用枚举或 tagged union，避免通用 spec 被类型专属字段污染。
4. 状态机在节点代理内部细化，对外继续提供简洁 `PodPhase`。
5. 新增沙箱类型优先通过新增 Provider 和 `SandboxClass` 资源完成，而不是修改调度器核心逻辑。
6. 高隔离需求必须显式表达，并由调度器和节点代理共同校验。

## 风险与应对

| 风险 | 影响 | 应对 |
| --- | --- | --- |
| 不同 Provider 语义差异过大 | 状态和错误难统一 | 定义最小公共状态与错误集合，类型专属信息放入 metadata |
| 节点能力上报不准确 | 调度失败或运行失败 | Provider 初始化自检，周期健康检查，失败时快速更新 NodeStatus |
| 制品准备耗时过长 | 启动延迟高 | 引入缓存、预拉取、调度评分考虑缓存命中 |
| 强隔离运行时依赖复杂 | 部署门槛高 | Provider 可选启用，缺依赖时健康状态为 false 并给出 reason |
| 安全字段表达不足 | 高风险能力被误用 | 默认拒绝危险能力，引入 profile 和 admission 校验 |
| 资源超卖 | 节点不稳定 | 引入 Reserve 阶段和 Provider 并发槽位 |

## 推荐下一步

优先实现最小闭环：

1. 在 `boss-api` 增加 `sandboxClass`、`sandbox` 和 `runtimeCapabilities` 数据结构。
2. 在 `boss-runtime` 增加 `RuntimeCapabilities` 和 `SandboxArtifact`。
3. 修改 `RuntimeManager`，支持按 class 查询 Provider。
4. 节点启动时把 baremetal 作为 `process` Provider 上报。
5. 调度器先实现最小 Filter：只调度到支持目标 class 的 Ready 节点。
6. 增加两个示例：`examples/pod-process.yaml` 和 `examples/pod-wasm.yaml`。
