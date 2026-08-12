# 部署说明

首个共享环境面向单台 Linux，通过私网或 VPN 访问。Compose 只公开 nginx 的 Web/API 端口，不公开 PostgreSQL、Prometheus 或内部 gRPC。

业务镜像由 GitHub Actions 推送到 Docker Hub；运行机默认 pull，不在服务器上编译 Rust/Node。

## 启动

```bash
cp .env.example .env
# 修改 POSTGRES_PASSWORD、BALLAST_PUBLIC_ORIGIN
# Docker Hub 命名空间若不是默认值，设置 BALLAST_IMAGE_NAMESPACE
docker compose -f deploy/compose.yaml pull
docker compose -f deploy/compose.yaml up -d
```

访问 `${BALLAST_PUBLIC_ORIGIN}`。网关需要能够访问五家交易所的公共 REST 和 WebSocket 域名。

从当前源码本地构建（可选）：

```bash
docker compose -f deploy/compose.yaml up --build -d
```

## 镜像发布

- 工作流：`.github/workflows/docker-publish.yml`
- 触发：`main` 推送、`v*` 标签、`workflow_dispatch`
- Secrets：`DOCKERHUB_USERNAME`、`DOCKERHUB_TOKEN`
- 镜像：
  - `{user}/ballast-server`
  - `{user}/ballast-gateway`
  - `{user}/ballast-web`
- 标签：`latest`（main）、分支名、semver、短 SHA
- 平台：`linux/amd64`

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

- OIDC 和角色权限。
- 内部 RPC 身份认证/加密或等价服务网格策略。
- 按交易所拆分私有账户网关。
- 密钥通过 Compose secrets 或目标环境 secret manager 挂载。
- 交易限额、kill switch、订单对账和告警接收端。
