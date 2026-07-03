# Boss 实施路线

本文档把 Boss 的目标架构拆成可执行能力清单。它不使用产品版本号表达边界，而是记录能力状态、关键改动、验收标准和测试场景。

## 能力状态

| 能力 | 状态 | 说明 |
| --- | --- | --- |
| 基础对象模型 | 已具备 | `Pod`, `Node`, `Deployment`, `ReplicaSet`, `Lease` 已建模 |
| Storage 与 watch | 已具备 | in-memory storage、CAS、watch bus 已可用 |
| Pod sandbox intent | 已具备 | `sandboxClass`、`sandbox`、artifact/network/security 字段已存在 |
| Node runtime capabilities | 已具备 | `runtimeCapabilities.providers` 已建模并由 bosslet 上报 |
| RuntimeManager 双索引 | 已具备 | 支持按 provider name 和 sandbox class 查询 |
| 能力感知调度 | 已具备基础能力 | scheduler 能按 sandbox class、artifact type、network mode、isolation level 绑定 node/provider，后续补充评分和资源约束 |
| Bosslet provider selection | 已具备基础能力 | bosslet 尊重 selected-provider，并在启动前重新校验 Provider 能力 |
| CLI 可观测输出 | 已具备基础能力 | Pod 和 Node 输出包含 sandbox/provider 摘要 |
| Controller framework | 已具备基础能力 | 已有 watch 驱动 workqueue、reconciler、retry/requeue 和 status conditions |
| Deployment/ReplicaSet 收敛 | 已具备基础能力 | 已有 API 路由、Deployment 到 ReplicaSet、ReplicaSet 到 Pod 的最小闭环 |
| Artifact handling | 已具备基础能力 | 支持本地路径、容器镜像和 Provider 原生拉取；生产级缓存/校验待扩展 |
| 真实 Provider | 已具备基础能力 | process/container/WASM/VM 已可运行；非 process Provider 依赖本地 CLI |

## 近期重点：控制器体系

目标：让声明式资源能被持续 reconcile，并把 Deployment/ReplicaSet/Pod 的 owner chain 打通。

关键改动：

- 在 `boss-controller-manager` 中实现通用 reconcile loop。
- 增加 `Reflector`、`LocalCache`、`WorkQueue`、retry 和 requeue-after。
- 实现 Deployment controller：Deployment 收敛到 owned ReplicaSet。
- 实现 ReplicaSet controller：ReplicaSet 收敛到 owned Pod。
- 增加基础 Garbage Collector：处理 controller owner 删除后的级联清理。
- 为 Deployment/ReplicaSet 增加 apiserver CRUD/list/watch/status 路由。
- 为 DeploymentStatus/ReplicaSetStatus 增加 `observedGeneration` 和 Conditions。

验收标准：

- 创建 Deployment 后能自动创建 ReplicaSet 和 Pod。
- Pod template 中的 `sandboxClass` 与 `sandbox` 原样传递。
- ReplicaSet 创建的 Pod 不带 `nodeName`，由 scheduler 继续绑定。
- 删除 Deployment 后 owned ReplicaSet/Pod 被清理。
- Reconcile 失败可重试，不会重复创建同名子资源。
- `bossctl get deployment` 能看到 replicas、readyReplicas 和核心 condition reason。

## 调度与节点能力完善

目标：让调度器和节点代理在统一 capability contract 下做更可靠的放置与拒绝。

关键改动：

- Scheduler filter 已增加 artifact type、network mode、isolation level 校验。
- Scheduler score 增加 provider health、节点名稳定排序、artifact cache hint。
- Bosslet 在启动前校验 selected provider 是否仍然支持目标 sandbox intent。
- Provider unhealthy 时快速刷新 NodeStatus，不把整个 Node 强制标记为 NotReady。
- Pod status 保持稳定 reason：`UnsupportedClass`、`ProviderUnavailable`、`InvalidSpec`、`CreateFailed`、`StartFailed`。

验收标准：

- 不支持的 sandbox class 不会绑定到错误节点。
- Provider 下线后，新 Pod 不再被调度到该 provider。
- 已绑定 Pod 如果 provider 缺失，bosslet 写入稳定失败 reason。
- CLI 能解释 Pod 等待、运行或失败的主要原因。

## Production Artifact Pipeline

目标：把用户描述的 artifact URI 转换为 provider 可消费的本地制品，并把准备成本纳入调度和状态。当前本地最终版已支持 Provider 直接消费本地路径或镜像引用；本节描述生产级缓存和校验扩展。

关键改动：

- 引入 `ArtifactResolver` 接口。
- 支持 executable、container image、wasm module、rootfs、disk image、remote template 等 artifact type。
- 增加摘要校验、认证失败、格式不支持、拉取超时等标准错误。
- 增加节点本地 artifact cache metadata。
- Provider 接收 prepared artifact，而不是自行理解所有 URI 细节。
- Scheduler 可以基于 cache hit、artifact size、provider locality 做评分。

验收标准：

- 相同 artifact 在节点上可以复用缓存。
- 摘要不匹配会阻止启动并写入 `ArtifactFetchFailed`。
- Provider 不需要重复实现通用拉取和校验逻辑。
- Pod status 能区分 artifact 准备失败与 runtime 启动失败。

## Provider 实现

目标：同一 API 能驱动多种真实 sandbox execution backend。

关键改动：

- 完善 process Provider 的生命周期、退出码、日志和清理语义。
- 增加最小 wasm Provider，支持本地 wasm module 启动和状态查询。
- 增加最小 container Provider，支持镜像拉取后启动一个容器 sandbox。
- 增加最小 microVM Provider，作为可选能力启用。
- Provider capability 来自启动自检和周期健康检查。

验收标准：

- process、wasm、container 至少三类 sandbox 能端到端运行。
- 所有 Provider 使用统一状态、错误 reason 和日志入口。
- 缺少本地依赖时 Provider 上报 `healthy=false` 和明确 reason。
- Provider 内部差异不泄漏到 scheduler 核心逻辑。

## 测试计划

Unit tests：

- Pod sandbox serde 与兼容映射。
- Node runtime capabilities serde。
- RuntimeManager name/class lookup。
- Scheduler capability filter。
- Bosslet provider selection。
- WorkQueue 去重、retry、requeue-after。
- Deployment/ReplicaSet condition merge。

Integration tests：

- unbound process Pod 自动调度并运行。
- unsupported sandbox class 保持 Pending 或写入稳定失败 reason。
- Deployment 创建后自动生成 ReplicaSet 和 Pod。
- ReplicaSet 扩缩容能创建和删除 owned Pod。
- Deployment/ReplicaSet status 随 Pod 状态变化更新。
- Pod delete 调用 provider stop/remove。

Manual tests：

```bash
make check
make test
make run-server
make run-node
bossctl apply -f examples/pod-process.yaml
bossctl get pods -o wide
bossctl get nodes
```

## 明确非目标

当前实施路线不要求一次完成：

- 多副本控制面和强一致持久化存储。
- 完整 RBAC、admission policy 和租户隔离。
- 复杂抢占、Reserve/Permit、资源预留和多调度器 profile。
- 完整 rolling update、rollback、revision history。
- 所有 Provider 的生产级安全加固。

这些能力应建立在当前对象模型、控制器框架、capability scheduling 和 Provider contract 稳定之后。
