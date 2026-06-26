# Boss 文档入口

Boss 是一个 sandbox-native orchestration system。它的核心目标不是把所有 workload 塞进单一 runtime 模型，而是让 process、container、WASM、microVM、VM、remote execution 这些执行环境共享同一个控制面、调度面、控制器体系和状态模型。

## 推荐阅读顺序

1. [设计哲学](design-philosophy.md)
   - 解释 Boss 为什么以 sandbox-native、capability-driven、minimal core 为核心。
   - 适合先建立判断标准，避免后续设计发散。

2. [系统架构设计](system-architecture.md)
   - 说明 User Plane、Control Plane、Controller Plane、Scheduling Plane、Node Plane、Runtime Plane、Storage Plane 的职责。
   - 适合理解完整数据流和模块边界。

3. [控制器架构设计](controller-architecture.md)
   - 定义 ControllerManager、Reflector、WorkQueue、Reconciler、LocalCache 的协作方式。
   - 适合实现 Deployment、ReplicaSet、Garbage Collector 等声明式收敛能力时参考。

4. [多类型沙箱管理设计](multi-sandbox-design.md)
   - 深入展开 SandboxClass、SandboxProvider、WorkloadIntent、调度、节点能力、状态、日志、指标和安全模型。
   - 适合实现 API、runtime、scheduler、bosslet 相关能力时参考。

5. [实施路线](implementation-roadmap.md)
   - 将目标架构拆成能力推进顺序、验收标准和测试场景。
   - 适合直接拆任务、写 issue、分配实现顺序。

6. [Beyond Kubernetes 方向](beyond-kubernetes-roadmap.md)
   - 说明 Boss 长期要竞争的方向和差异化能力。
   - 适合做战略路线和产品定位参考。

## 文档约定

- 正文以中文为主。
- 核心对象、接口和字段保留英文，例如 `Pod`, `Node`, `SandboxClass`, `RuntimeManager`, `RuntimeCapabilities`。
- 文档采用最终目标架构叙事，不用产品版本号表达能力边界。
- 实施顺序使用“能力状态”和“验收标准”描述，不用版本阶段描述。
- 公开定位统一使用 sandbox-native orchestration。

## 实现状态

当前仓库已经具备：

- `boss-api`: `Pod`, `Node`, `Deployment`, `ReplicaSet`, `Lease` 等基础模型。
- `boss-apiserver`: `Pod`、`Node` 的 CRUD、list、watch、status update；通用 CRUD 框架可扩展到更多资源。
- `boss-store`: in-memory storage、CAS、watch bus。
- `boss-scheduler`: 能扫描未绑定 Pod，基于 Node runtime capabilities 选择 node/provider 并写入绑定信息。
- `bosslet`: 节点注册、heartbeat、Provider capability 上报、watch bound Pod、尊重 selected provider 驱动 runtime。
- `boss-runtime`: `Runtime` trait、`RuntimeManager` 双索引、baremetal runtime 和 container/vm/wasm stub。
- `bossctl`: apply、get、delete、watch，并能展示 Pod/provider/node capability 摘要。

当前主要缺口：

- `boss-controller-manager` 仍是 skeleton，缺少 list-watch、workqueue、reconciler 和具体控制器。
- `Deployment`、`ReplicaSet` API 路由和 status subresource 还未接入 apiserver。
- `DeploymentStatus`、`ReplicaSetStatus` 还缺少统一 Conditions 和 `observedGeneration`。
- Artifact 解析、拉取、校验、本地缓存和日志/指标接口还未形成完整管线。
- container/vm/wasm Provider 还没有真实执行能力。
