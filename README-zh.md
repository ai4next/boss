# boss

一个用 Rust 编写的沙箱原生（sandbox-native）编排与调度系统（edition 2024）。项目聚焦异构执行：进程、容器、虚拟机、微虚拟机、WASM 和远程沙箱共享同一套控制面，而不被强行塞入单一运行时模型。

当前已具备可工作的控制面、节点代理、能力感知调度器和进程运行时路径。容器、VM 和 WASM Provider 处于桩阶段，控制器框架和真实 Provider 实现仍在建设中。

## 工作区布局

```
crates/
  boss-common            # 错误、日志、ID、时间
  boss-api               # 纯 serde 数据模型（Object<T>, Pod, Node...）
  boss-store             # Storage trait + InMemoryRegistry + WatchBus (raft: stub)
  boss-apiserver         # axum CRUD + 换行分隔 watch + CAS
  boss-scheduler         # 能力感知的 Pod 到节点/Provider 绑定
  boss-controller-manager# reconciler trait 骨架，控制器循环待实现
  bosslet                # 节点代理：watch → sync → runtime → status → heartbeat
  boss-runtime           # Runtime trait + BareMetal（真实）+ container/vm/wasm（桩）
  bossctl                # CLI 客户端（apply / get / delete / watch）
bin/
  boss-server            # 控制面：apiserver + storage + scheduler + CM 骨架
  boss-node              # 节点：bosslet + RuntimeManager 多个 Provider
```

关键设计选择：基于扁平字符串键（`/registry/{type}/{ns}/{name}`）的通用 `Storage` trait、使用 `StorageBackend` 枚举分发实现运行时多态、通过 `metadata.resourceVersion` 在存储层强制乐观并发（CAS）、以及带历史环缓冲区的 `BoxStream` watch 支持回放。

## 设计方向

Boss 有意围绕沙箱原生调度设计，而非仅容器放置。控制面应将沙箱类别、制品类型、隔离级别、Provider 能力、启动延迟和制品本地化作为一等调度输入。

设计文档：

- `docs/index.md`：文档入口和推荐阅读顺序。
- `docs/design-philosophy.md`：设计原则和核心决策规则。
- `docs/system-architecture.md`：端到端系统架构和数据流。
- `docs/controller-architecture.md`：控制器管理器、workqueue、reconciler、属主关系和状态条件。
- `docs/multi-sandbox-design.md`：多沙箱管理详细设计。
- `docs/implementation-roadmap.md`：能力路线图、验收标准和测试场景。
- `docs/beyond-kubernetes-roadmap.md`：更锐利的异构沙箱平台战略方向。

## 开发工作流

项目包含一个小型 Makefile，常用构建、lint 和本地集群命令可直接使用：

```bash
make help
make check
make test
make clippy
make fmt
make build
```

等价的原生 cargo 命令同样可用：

```bash
cargo check --workspace --all-targets
cargo build --workspace
cargo test --workspace
```

## 运行端到端流程

三个终端（或把前两个放入后台）：

```bash
# 1. 控制面
make run-server

# 2. 节点代理（注册 node-A，上报 Provider 能力，watch 绑定到该节点的 Pod）
make run-node

# 3. 提交一个 Pod；调度器将其绑定到有能力的节点/Provider
make apply-example
make get-example        # pod phase: Running
make get-pods
make watch-pods         # 流式事件
make delete-example     # 沙箱停止，进程被杀死
```

无需修改文件即可覆盖默认值：

```bash
BOSS_BIND=127.0.0.1:18080 make run-server
BOSS_API_SERVER=http://127.0.0.1:18080 BOSS_NODE_NAME=node-B make run-node
```

`examples/pod.yaml` 通过 baremetal 运行时运行 `sleep 300`。Pod 处于 Running 状态时 `pgrep -fl "sleep 300"` 可看到宿主进程，delete 时进程被杀死。

## 能力状态

- ✅ 工作区、数据模型、内存存储、apiserver、CLI、bosslet、baremetal 运行时 — 端到端可运行。
- ✅ Pod 沙箱意图、Node 运行时能力、RuntimeManager Provider 索引、调度器绑定和 bosslet Provider 选择已有基础支持。
- ⬜ 控制器管理器 reconcile 循环、Deployment/ReplicaSet 控制器、workqueue 和状态条件待实现。
- ⬜ 制品解析器/缓存、更丰富的调度评分和真实的 container/vm/wasm Provider 待实现。
- ⬜ raft 存储、多副本控制面、领导者选举、认证/授权/RBAC 和准入策略属于未来的生产化工作。
