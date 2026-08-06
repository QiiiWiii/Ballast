# Compose 部署

```bash
cp .env.example .env
docker compose -f deploy/compose.yaml up --build -d
docker compose -f deploy/compose.yaml down
```

默认入口为 `http://localhost:8080`。PostgreSQL、gRPC 和 Prometheus 不映射宿主机端口。

共享环境至少修改：

- `POSTGRES_PASSWORD`
- `BALLAST_PUBLIC_ORIGIN`
- `BALLAST_WEB_PORT`
- `BACKUP_RETENTION_DAYS`

交易所 API Key 当前不受支持，不应放入 `.env`。私有账户阶段必须使用 secrets 文件或外部 secret manager。

`BALLAST_LIVE_ENABLED` 固定为 `false`。即使手工改成 `true`，Rust 服务也会在 OIDC 验签、私有对账与风控实现完成前拒绝启动。生产阶段将按交易所拆分五个网关进程；当前共享环境仍只运行无密钥的公共行情网关。
