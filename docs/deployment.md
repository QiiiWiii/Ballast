# 部署说明

首个共享环境面向单台 Linux，通过私网或 VPN 访问。Compose 只公开 nginx 的 Web/API 端口，不公开 PostgreSQL、Prometheus 或内部 gRPC。

## 启动

```bash
cp .env.example .env
# 修改 POSTGRES_PASSWORD、BALLAST_PUBLIC_ORIGIN，并为股票研究配置 Alpaca market-data key
docker compose -f deploy/compose.yaml up --build -d
```

访问 `${BALLAST_PUBLIC_ORIGIN}`。网关需要能够访问五家交易所的公共 REST 和 WebSocket 域名。

## 服务

- `web`：静态前端和 `/api` WebSocket/HTTP 反向代理。
- `server`：Rust API、调度 worker 和 `/metrics`。
- `server` 的 `/var/lib/ballast/research`：按交易日保存 Alpaca/IEX 压缩历史行情，使用独立持久卷。
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
