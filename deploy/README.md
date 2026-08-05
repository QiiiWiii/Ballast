# Deployment

`compose.yaml` 只用于本地开发和早期集成验证，不是生产部署方案。

```bash
docker compose -f deploy/compose.yaml up --build
docker compose -f deploy/compose.yaml down
```

本地 PostgreSQL 使用公开的开发凭证 `ballast/ballast`。任何共享或生产环境都必须替换凭证，并通过 secret manager 注入交易所 API Key。

生产部署在具备真实下单能力前还需要：

- 独立数据库备份与恢复演练；
- Rust 服务与各交易所网关的单独部署单元；
- mTLS 或等价的内部 RPC 身份验证；
- 出站网络白名单；
- 审计日志、告警和人工熔断入口；
- API Key 权限分级与轮换。
