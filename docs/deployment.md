# 部署说明

首个共享环境面向单台 Linux（**amd64 或 arm64**）。Compose 只公开 Web/API 端口，不公开 PostgreSQL、Prometheus 或内部 gRPC；公网域名部署应将 Web 端口绑定到回环地址，再由宿主机 Caddy 等反向代理提供 HTTPS。

`deploy/compose.yaml` 只使用 Docker Hub 预构建镜像；运行机默认 `pull`，不在服务器上编译。

## 启动

```bash
cp .env.example .env
# 修改 POSTGRES_PASSWORD、BALLAST_PUBLIC_ORIGIN
# 启用认证时同时设置 BALLAST_OIDC_ISSUER/AUDIENCE/CLIENT_ID
# 可选：BALLAST_RECONCILIATION_ALERT_WEBHOOK_URL=https://alerts.example.test/ballast
# 生产建议固定：BALLAST_IMAGE_TAG=sha-<short>
docker compose -f deploy/compose.yaml pull
docker compose -f deploy/compose.yaml up -d
```

Compose 会等待 PostgreSQL、Gateway 和 server 分别通过健康检查后再启动依赖服务；上线后应确认 `docker compose -f deploy/compose.yaml ps` 中业务容器处于 `healthy` 或正常运行状态。

访问 `${BALLAST_PUBLIC_ORIGIN}`。网关需要能够访问五家交易所的公共 REST 和 WebSocket 域名。

从当前源码本地构建（可选）：

```bash
docker compose -f deploy/compose.yaml -f deploy/compose.build.yaml up --build -d
```

OKX 只读账户快照使用独立 overlay，默认部署不挂载交易所凭证：

```bash
mkdir -p deploy/secrets
# 参照 deploy/okx-accounts.example.json 创建并 chmod 600
docker compose -f deploy/compose.yaml -f deploy/compose.private.yaml up -d
```

账户密钥必须只读、禁用提现并绑定允许 IP。当前 overlay 仅开放余额、线性永续仓位和普通未结订单的 `AccountService.GetAccountSnapshot`；反向永续、缺失稳定 `client_order_id` 的订单、条件或算法订单、下单、撤单和私有流仍失败关闭。

private overlay 同时显式启用账户快照和本地非终态订单查询 worker。默认 paper Compose 将 `BALLAST_PRIVATE_RECONCILIATION_ENABLED` 固定为 `false`，不调用任何私有 RPC。

配置只读账户后，可直接在 Gateway 容器内运行只读验收。该命令只调用 `AccountService.GetAccountSnapshot`，输出余额、仓位和未结订单数量，不输出金额或凭证：

```bash
docker compose -f deploy/compose.yaml -f deploy/compose.private.yaml exec \
  -e BALLAST_PRIVATE_SMOKE_GATEWAY=127.0.0.1:50051 \
  -e BALLAST_PRIVATE_SMOKE_ACCOUNT_ID=okx-demo-readonly \
  -e BALLAST_PRIVATE_SMOKE_ACCOUNT_ENVIRONMENT=demo \
  gateway npm run private-smoke
```

Gateway 默认不向宿主机暴露 gRPC；不得为 smoke 直接开放公网 gRPC 端口。本地源码运行 Gateway 时，也可使用相同环境变量执行 `make private-smoke`。

## 镜像与 Tag

| 镜像 | 仓库 |
|------|------|
| server | `{user}/ballast-server` |
| gateway | `{user}/ballast-gateway` |
| web | `{user}/ballast-web` |

| Tag | 含义 |
|-----|------|
| `X.Y.Z` / `X.Y` / `vX.Y.Z` | 正式发版版本号 |
| `latest` | 最新正式发版（仅 `v*` 时更新） |
| `sha-<short>` | 单次构建 ID，不可变 |
| `main` | 手动从 main 预览构建，不含 latest |

发版：

```bash
git tag v0.1.0 && git push origin v0.1.0
```

CI 为每个 tag 推送 **multi-arch manifest**（`linux/amd64` + `linux/arm64`）。三套业务镜像必须使用**同一个** `BALLAST_IMAGE_TAG`。

镜像启动后执行 paper 端到端验收：

```bash
BALLAST_SMOKE_BASE_URL="${BALLAST_PUBLIC_ORIGIN}" make paper-smoke
```

脚本会在系统中保留唯一命名的模板、任务、切片和事件，作为发布验收记录。

## 镜像发布

- 工作流：`.github/workflows/docker-publish.yml`
- 触发：`v*` 发版标签、`workflow_dispatch`
- Secrets：`DOCKERHUB_USERNAME`、`DOCKERHUB_TOKEN`

## 服务

- `web`：nginx 静态前端和 `/api` WebSocket/HTTP 反向代理。
- `server`：Rust API、调度 worker 和 `/metrics`。
- `gateway`：Node/ccxt 公共行情 gRPC。
- `postgres`：权威任务、切片和事件。
- `prometheus`：15 天指标保留，不对宿主机暴露端口。
- `backup`：默认每日 custom-format `pg_dump`，默认保留 14 天。

## 恢复演练

至少每月把最新 `.dump` 恢复到独立 PostgreSQL 实例，运行迁移状态检查并核对任务、切片、事件序号。备份成功不等于可恢复，恢复演练结果应进入运维记录。

## 生产前仍需完成

Web 镜像在容器启动时写入公开 OIDC 运行时配置，同一镜像可用于不同 issuer，无需重新构建。OIDC 客户端必须把 `${BALLAST_PUBLIC_ORIGIN}/auth/callback` 注册为 redirect URI，并把 `${BALLAST_PUBLIC_ORIGIN}/` 注册为 post-logout redirect URI。

- 生产环境启用 OIDC；未配置时仅适合纸面研究和内部验证。
- 对账差异 webhook 只接受 HTTPS；未配置时仍持久化对账事实，但不发起告警网络请求。
- 内部 RPC 身份认证/加密或等价服务网格策略。
- 扩展到 OKX 之外的单交易所私有账户适配器。
- 交易限额、kill switch 和完整 live 执行闭环；对账差异 webhook 已实现，但仍需在受控环境完成接收端演练。
