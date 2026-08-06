# ADR 0003：双角色审批与零默认风控

状态：Accepted

paper 任务直接进入 `scheduled`；live 任务进入 `pending_approval`。operator 发起任务，另一位 admin 批准或拒绝，创建者不能自审批。

风险在创建、审批和实际提交前分别检查。所有生产限额默认零，且全局、交易所、账户三级 kill switch 任一级都可以阻止提交。审批只授予调度资格，不绕过提交时的行情新鲜度、名义金额、仓位或敞口检查。

在 OIDC issuer/audience/JWKS 验证、只读账户对账和生产开关完成前，私有 HTTP/gRPC 入口失败关闭，`BALLAST_LIVE_ENABLED=true` 会让服务拒绝启动。
