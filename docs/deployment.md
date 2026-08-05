# 部署原则

本地开发使用 [deploy/compose.yaml](../deploy/compose.yaml)。生产部署方案需要在真实交易阶段前结合目标运行环境单独设计。

## 生产最低要求

- PostgreSQL 使用托管或具备备份、PITR 和恢复演练的独立实例。
- Rust 服务和网关不共用 root 用户。
- 内部 gRPC 开启身份认证和加密。
- 每家交易所可以独立发布、限频、熔断和回滚。
- API Key 从 secret manager 注入，不经过数据库和日志。
- 只读和交易凭证分离；交易凭证禁用提现权限。
- 真实下单前具备全局停止、账户限额、价格偏离和裸露时间告警。

## 发布顺序

数据库迁移必须先于依赖新结构的 Rust 服务。Node 网关和 Rust 客户端的 Protobuf 版本必须兼容；发生语义破坏时发布新的 protobuf package 版本。
