# Compose 部署

`deploy/compose.yaml` **只拉取 Docker Hub 上已打包的镜像**，服务器不编译业务代码。

| 镜像 | 说明 |
|------|------|
| `sevenold/ballast-server` | Rust API / paper worker |
| `sevenold/ballast-gateway` | 公共行情 gRPC |
| `sevenold/ballast-web` | 前端 + nginx |

CI 同时发布 **`linux/amd64` 与 `linux/arm64`** 多架构 manifest，ARM 服务器直接 `pull` 即可。

## 镜像 Tag 约定

| Tag | 何时更新 | 用途 |
|-----|----------|------|
| `latest` | 每次 `main` 推送 | 方便跟踪主干（会变） |
| `main` | 每次 `main` 推送 | 与分支同名的滚动标签 |
| `sha-<7位>` | 每次构建 | **推荐生产固定**，不可变 |
| `vX.Y.Z` / `X.Y.Z` / `X.Y` | git 打 `v*` 标签时 | 正式发版 |

查看某次构建打出的 tag：GitHub Actions → Docker Publish → 对应 job summary。

生产示例：

```bash
# 固定到某次 commit（优先）
BALLAST_IMAGE_TAG=sha-1a2409b

# 或跟随主干滚动
BALLAST_IMAGE_TAG=latest
```

三个业务镜像共用同一个 `BALLAST_IMAGE_TAG`，保证 server/gateway/web 同源构建。

## 服务器

```bash
git clone --depth 1 https://github.com/QiiiWiii/Ballast.git
cd Ballast
cp .env.example .env
# 修改 POSTGRES_PASSWORD、BALLAST_PUBLIC_ORIGIN
# 可选：BALLAST_IMAGE_TAG=sha-xxxxxxx

docker compose -f deploy/compose.yaml pull
docker compose -f deploy/compose.yaml up -d
```

升级：

```bash
# 滚动 latest/main
docker compose -f deploy/compose.yaml pull
docker compose -f deploy/compose.yaml up -d

# 或改 .env 里的 BALLAST_IMAGE_TAG 后再 pull/up
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

## CI 发布

- 工作流：`.github/workflows/docker-publish.yml`
- 触发：`main` 推送、`v*` 标签、手动 `workflow_dispatch`
- Secrets：`DOCKERHUB_USERNAME`、`DOCKERHUB_TOKEN`
- 平台：`linux/amd64,linux/arm64`

## 安全说明

交易所 API Key 当前不受支持，不应放入 `.env`。  
`BALLAST_LIVE_ENABLED` 固定为 `false`。
