# Compose 部署

`server` / `gateway` / `web` 由 GitHub Actions 构建并推送到 Docker Hub。服务器默认 **pull 镜像运行**，不必在机器上编译。

镜像（Docker Hub 命名空间可改）：

- `${BALLAST_IMAGE_NAMESPACE}/ballast-server`
- `${BALLAST_IMAGE_NAMESPACE}/ballast-gateway`
- `${BALLAST_IMAGE_NAMESPACE}/ballast-web`

默认命名空间 `sevenold`，标签默认 `latest`。

## 服务器（推荐）

```bash
# 只需 deploy/ 与 .env
git clone --depth 1 https://github.com/QiiiWiii/Ballast.git
cd Ballast
cp .env.example .env
# 修改 POSTGRES_PASSWORD、BALLAST_PUBLIC_ORIGIN
# 若 Docker Hub 用户名不是 sevenold：
# BALLAST_IMAGE_NAMESPACE=your-dockerhub-user

docker compose -f deploy/compose.yaml pull
docker compose -f deploy/compose.yaml up -d
```

升级：

```bash
docker compose -f deploy/compose.yaml pull
docker compose -f deploy/compose.yaml up -d
```

## 本地从源码构建

完整仓库且需要打本地镜像时：

```bash
docker compose -f deploy/compose.yaml up --build -d
```

`--build` 使用当前工作区源码，不经过 Docker Hub。

## 环境变量

| 变量 | 默认 | 说明 |
|------|------|------|
| `BALLAST_IMAGE_NAMESPACE` | `sevenold` | Docker Hub 用户/组织名 |
| `BALLAST_IMAGE_TAG` | `latest` | 镜像标签（也可固定为 short SHA） |
| `POSTGRES_PASSWORD` | `ballast-local-only` | 共享环境必须修改 |
| `BALLAST_PUBLIC_ORIGIN` | `http://localhost:8080` | 浏览器访问源 |
| `BALLAST_WEB_PORT` | `8080` | 宿主机端口 |

## CI 发布

推送 `main` 或 `v*` 标签时，工作流 `.github/workflows/docker-publish.yml` 构建并推送三套镜像。仓库需配置 secrets：

- `DOCKERHUB_USERNAME`
- `DOCKERHUB_TOKEN`（Docker Hub Access Token）

当前发布平台为 `linux/amd64`。

## 安全说明

交易所 API Key 当前不受支持，不应放入 `.env`。  
`BALLAST_LIVE_ENABLED` 固定为 `false`；即使手工改成 `true`，服务也会在 OIDC 与私有对账完成前拒绝启动。
