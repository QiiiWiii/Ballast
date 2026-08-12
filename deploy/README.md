# Compose 部署

`deploy/compose.yaml` **只拉取 Docker Hub 上已打包的镜像**，服务器不编译业务代码。

| 镜像 | 说明 |
|------|------|
| `sevenold/ballast-server` | Rust API / paper worker |
| `sevenold/ballast-gateway` | 公共行情 gRPC |
| `sevenold/ballast-web` | 前端 + nginx |

CI 同时发布 **`linux/amd64` 与 `linux/arm64`** 多架构 manifest，ARM 服务器直接 `pull` 即可。

## 镜像 Tag 约定

镜像与 **git 发版**一起发布（推送 `v*` 标签触发 CI）。

| Tag | 何时更新 | 用途 |
|-----|----------|------|
| `X.Y.Z` / `X.Y` / `vX.Y.Z` | 打 `vX.Y.Z` 发版 | 普通版本号，固定可回滚 |
| `latest` | **仅正式发版**（`v*`）时移动 | 始终指向最新正式版 |
| `sha-<7位>` | 每次构建 | 不可变构建 ID |
| `main` | 仅 `workflow_dispatch` 从 main 手动构建 | 开发预览，**不带 latest** |

发版示例：

```bash
git tag v0.1.0
git push origin v0.1.0
# CI 推送：sevenold/ballast-*:0.1.0 与 sevenold/ballast-*:latest 等
```

服务器选 tag：

```bash
# 跟随最新正式版
BALLAST_IMAGE_TAG=latest

# 钉死某一正式版
BALLAST_IMAGE_TAG=0.1.0

# 钉死某次构建
BALLAST_IMAGE_TAG=sha-1a2409b
```

三个业务镜像共用同一个 `BALLAST_IMAGE_TAG`，保证 server/gateway/web 同源构建。

## 服务器

```bash
git clone --depth 1 https://github.com/QiiiWiii/Ballast.git
cd Ballast
cp .env.example .env
# 修改 POSTGRES_PASSWORD、BALLAST_PUBLIC_ORIGIN
# 启用认证时同时设置 BALLAST_OIDC_ISSUER/AUDIENCE/CLIENT_ID
# 可选：BALLAST_IMAGE_TAG=sha-xxxxxxx

docker compose -f deploy/compose.yaml pull
docker compose -f deploy/compose.yaml up -d
```

升级：

```bash
# 跟随最新正式版 latest，或先改 .env 里的 BALLAST_IMAGE_TAG
docker compose -f deploy/compose.yaml pull
docker compose -f deploy/compose.yaml up -d
```

## 本地从源码构建（可选）

```bash
docker compose -f deploy/compose.yaml -f deploy/compose.build.yaml up --build -d
```

## OKX 只读账户快照（可选）

默认部署不读取交易所凭证。启用 P0.2 只读账户快照时，在宿主机创建 `deploy/secrets/okx-accounts.json`，格式参考 `deploy/okx-accounts.example.json`，并确保 API Key 仅有读取权限、禁用提现、绑定允许 IP。

```bash
mkdir -p deploy/secrets
chmod 700 deploy/secrets
chmod 600 deploy/secrets/okx-accounts.json
docker compose -f deploy/compose.yaml -f deploy/compose.private.yaml up -d
```

当前仅 `AccountService.GetAccountSnapshot` 支持 OKX 余额、线性永续仓位和普通未结订单快照。反向永续因数量单位尚未进入协议而显式失败，缺失稳定 `client_order_id` 的订单同样失败关闭；账户存在条件单或算法订单时，快照也会显式失败，避免静默遗漏风险敞口。`TradingService`、私有流和原生算法提交继续显式禁用；不得给该阶段的 Key 授予交易或提现权限。

private overlay 会将 `BALLAST_ORDER_RECONCILIATION_ENABLED=true`，仅用于查询本地已知非终态订单。默认 paper Compose 固定为 `false`，不触发私有账户请求。

## 环境变量

| 变量 | 默认 | 说明 |
|------|------|------|
| `BALLAST_IMAGE_NAMESPACE` | `sevenold` | Docker Hub 用户名 |
| `BALLAST_IMAGE_TAG` | `latest` | 业务镜像统一 tag |
| `POSTGRES_PASSWORD` | `ballast-local-only` | 共享环境必须修改 |
| `BALLAST_PUBLIC_ORIGIN` | `http://localhost:8080` | 浏览器访问源 |
| `BALLAST_WEB_PORT` | `8080` | 宿主机端口 |
| `BALLAST_OIDC_ISSUER` | 空 | OIDC HTTPS issuer；为空时保持 paper/open 模式 |
| `BALLAST_OIDC_AUDIENCE` | 空 | API access token audience |
| `BALLAST_OIDC_CLIENT_ID` | 空 | Web Authorization Code + PKCE 公共客户端 ID |
| `BALLAST_ORDER_RECONCILIATION_ENABLED` | `false` | 仅 private overlay 启用本地已知订单查询 worker |

## CI 发布

- 工作流：`.github/workflows/docker-publish.yml`
- 触发：`v*` 标签、手动 `workflow_dispatch`
- Secrets：`DOCKERHUB_USERNAME`、`DOCKERHUB_TOKEN`
- 平台：`linux/amd64,linux/arm64`

## 安全说明

交易所凭证只能通过 `deploy/compose.private.yaml` 挂载的 secret 文件提供，不得放入 `.env`、日志或数据库。
`BALLAST_LIVE_ENABLED` 固定为 `false`。
