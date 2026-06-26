# Boss 控制器架构设计

Controller Plane 负责把声明式资源持续收敛到期望状态。它不直接运行 sandbox，不做节点绑定，也不理解 Provider 内部实现；它通过 watch 资源变化、入队 reconcile key、创建或更新下游资源，让 scheduler、bosslet 和 runtime plane 完成各自职责。

## 核心目标

- 所有声明式资源都可以 watch、enqueue、reconcile。
- reconcile 必须幂等，可以安全重试。
- 控制器只写自己负责的 spec、metadata、status 子集。
- Deployment/ReplicaSet 原样传递 Pod template 中的 sandbox intent。
- Pod 是否能运行由 scheduler 和 node capability 决定，不由 controller 决定。
- status 使用稳定 Conditions，让 CLI、自动化和告警能解释收敛状态。

## 组件模型

```text
boss-store watch
      |
      v
Reflector -----> LocalCache
      |
      v
WorkQueue -----> Reconciler -----> boss-store CAS writes
      ^                |
      |                v
      +---------- requeue / retry
```

### ControllerManager

`ControllerManager` 负责启动和管理所有控制器：

- 初始化 shared client、reflector、cache 和 workqueue。
- 为每类资源注册 event handler。
- 启动 Deployment controller、ReplicaSet controller 和 Garbage Collector skeleton。
- 统一处理 graceful shutdown、worker 数量、重试策略和观测日志。

### Reflector

`Reflector` 负责 list-watch：

- 启动时先 list 当前对象，写入 `LocalCache` 并触发初始入队。
- 后续 watch store event，按 resourceVersion 增量更新 cache。
- watch 断开时从最近可用 resourceVersion 重新建立连接。
- watch 不负责业务判断，只把对象变化转换为缓存更新和 key 入队。

### LocalCache

`LocalCache` 是控制器的只读观察视图：

- 按 kind/namespace/name 索引对象。
- 支持按 label selector 查找 owned 或 matching 对象。
- 允许 reconciler 快速读取 observed state，减少全量 list。
- cache 只是优化；最终写入仍以 store CAS 为准。

### WorkQueue

`WorkQueue` 负责 reconcile 调度：

- key 格式：`<resource>/<namespace>/<name>`，集群级资源 namespace 为空。
- 同一 key 可以去重，避免事件风暴导致重复 reconcile。
- reconcile 失败时按指数退避重试。
- reconcile 返回 requeue-after 时按时间重新入队。
- 删除事件也入队，让控制器有机会清理派生资源或更新 owner 状态。

### Reconciler

`Reconciler` 是资源专属收敛逻辑：

- 读取期望对象和相关 observed 对象。
- 计算差异，执行最小 create/update/delete。
- 使用 CAS 写入，冲突时重新读取并重试。
- 更新 status 和 Conditions。
- 保持幂等，不依赖某次事件必须被精确处理。

## 控制器职责

### Deployment Controller

Deployment controller 管理 Deployment 到 ReplicaSet 的收敛：

- watch `Deployment`、owned `ReplicaSet`。
- 确保每个 Deployment 有一个当前 template 对应的 controller-owned ReplicaSet。
- 将 `spec.replicas`、`spec.selector`、`spec.template` 同步到当前 ReplicaSet。
- template 中的 `sandboxClass`、`sandbox`、containers、resources、labels 原样传递。
- 汇总 owned ReplicaSet 的 replicas、readyReplicas、availableReplicas，写入 Deployment status。
- 不直接创建 Pod，不选择 node/provider，不解释 runtime-specific failure。

滚动发布、revision history、rollback 可以建立在同一 owner/template-hash 机制上，但不属于控制器框架的最小闭环。

### ReplicaSet Controller

ReplicaSet controller 管理 ReplicaSet 到 Pod 的收敛：

- watch `ReplicaSet`、matching/owned `Pod`。
- 通过 `ownerReferences.controller=true` 识别自己控制的 Pod。
- 当 owned Pod 少于 `spec.replicas` 时创建 Pod。
- 当 owned Pod 多于 `spec.replicas` 时删除多余 Pod。
- 创建 Pod 时保留 template 中的 sandbox intent，保持 `spec.nodeName` 为空。
- 汇总 owned Pod 的 phase/conditions，写入 ReplicaSet status。
- 不做调度、不调用 runtime、不修改 selected-provider annotation。

### Garbage Collector Skeleton

Garbage Collector 负责所有权链清理：

- watch 带 `ownerReferences` 的对象。
- 当 controller owner 不存在或正在删除时，按策略删除 dependent 对象。
- 先支持后台级联删除；finalizer 和前台删除策略作为扩展能力。
- 不参与 Deployment/ReplicaSet 的正常扩缩容决策。

## Ownership 与 Selector 规则

- `ownerReferences.controller=true` 表示唯一控制者。
- 一个对象最多只能有一个 controller owner。
- Deployment 控制 ReplicaSet，ReplicaSet 控制 Pod。
- selector 用于发现对象，ownerReference 用于确认控制权。
- 控制器不能接管已有 controller owner 的对象。
- selector 与 template labels 不匹配时，控制器应设置 `Degraded=True` 并拒绝创建不匹配子资源。

## Status 与 Conditions

DeploymentStatus 和 ReplicaSetStatus 保留计数字段，并增加统一 Conditions：

```yaml
status:
  observedGeneration: 3
  replicas: 3
  readyReplicas: 2
  availableReplicas: 2
  conditions:
    - type: Reconciling
      status: "False"
      reason: ReconcileComplete
      message: desired state is observed
      observedGeneration: 3
      lastTransitionTime: "2026-06-26T10:00:00Z"
    - type: Available
      status: "True"
      reason: MinimumAvailable
      message: enough replicas are available
      observedGeneration: 3
      lastTransitionTime: "2026-06-26T10:00:00Z"
```

标准 condition types：

- `Reconciling`: 控制器是否仍在处理当前 generation。
- `Progressing`: 下游资源是否朝期望状态推进。
- `Available`: 是否已有足够可用副本。
- `Degraded`: 是否存在阻塞收敛的稳定错误。

标准 reason 示例：

- `ReconcileComplete`
- `ReplicaSetCreated`
- `ReplicaSetUpdateFailed`
- `PodCreateFailed`
- `SelectorMismatch`
- `OwnerConflict`
- `ObservedGenerationLagging`

## API 与路由要求

控制器体系要求 apiserver 暴露以下能力：

- `Deployment` CRUD/list/watch/status。
- `ReplicaSet` CRUD/list/watch/status。
- `Pod` CRUD/list/watch/status。
- 基于 resourceVersion 的 watch replay。
- CAS update，用于 spec、metadata 和 status 更新冲突检测。

控制器写 status 时必须使用 status subresource；写子资源 spec/metadata 时使用普通 update/create/delete。

## Failure Semantics

- transient error：重新入队并退避重试。
- CAS conflict：立即重新读取对象并重试有限次数。
- invalid spec：设置 `Degraded=True`，不创建危险或不匹配子资源。
- missing owner：dependent 对象进入垃圾回收路径。
- scheduler 或 bosslet failure：通过 Pod status 向上汇总，不由 controller 直接修复 provider 问题。

## 验收标准

- 创建 Deployment 后自动创建 owned ReplicaSet。
- ReplicaSet 根据 replicas 自动创建 Pod，Pod 保持未绑定并等待 scheduler。
- template 中的 sandbox intent 能完整传递到 Pod。
- 删除 Deployment 后 owned ReplicaSet/Pod 最终被清理。
- 失败 reconcile 会重试，且不会重复创建同名子资源。
- Deployment/ReplicaSet status 能反映 observedGeneration、replicas 和 Conditions。
