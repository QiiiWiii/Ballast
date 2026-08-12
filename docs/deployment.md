# 部署说明

首个共享环境面向单台 Linux（**amd64 或 arm64**），通过私网或 VPN 访问。Compose 只公开 nginx 的 Web/API 端口，不公开 PostgreSQL、Prometheus 或内部 gRPC。

`deploy/compose.yaml` 只使用 Docker Hub 预构建镜像；运行机默认 `pull`，不在服务器上编译。

## 启动

```bash
cp .env.example .env
# 修改 POSTGRES_PASSWORD、BALLAST_PUBLIC_ORIGIN
# 启用认证时同时设置 BALLAST_OIDC_ISSUER/AUDIENCE/CLIENT_ID
# 生产建议固定：BALLAST_IMAGE_TAG=sha-<short>
docker compose -f deploy/compose.yaml pull
docker compose -f deploy/compose.yaml up -d
```

访问 `${BALLAST_PUBLIC_ORIGIN}`。网关需要能够访问五家交易所的公共 REST 和 WebSocket 域名。

从当前源码本地构建（可选）：

```bash
docker compose -f deploy/compose.yaml -f deploy/compose.build.yaml up --build -d
```

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

## 镜像发布

- 工作流：`.github/workflows/docker-publish.yml`
- 触发：`v*` 发版标签、`workflow_dispatch`
- Secrets：`DOCKERHUB_USERNAME`、`DOCKERHUB_TOKEN`

## 服务

- `web`：静态前端和 `/api` WebSocket/HTTP 反向代理。
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
- 内部 RPC 身份认证/加密或等价服务网格策略。
- 按交易所拆分私有账户网关。
- 密钥通过 Compose secrets 或目标环境 secret manager 挂载。
- 交易限额、kill switch、订单对账和告警接收端。
