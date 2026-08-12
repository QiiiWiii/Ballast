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

## CI 发布

- 工作流：`.github/workflows/docker-publish.yml`
- 触发：`main` 推送、`v*` 标签、手动 `workflow_dispatch`
- Secrets：`DOCKERHUB_USERNAME`、`DOCKERHUB_TOKEN`
- 平台：`linux/amd64,linux/arm64`

## 安全说明

交易所 API Key 当前不受支持，不应放入 `.env`。  
`BALLAST_LIVE_ENABLED` 固定为 `false`。
