# 开源准备核对

本文件记录公开发布前的检查结果。

## 1. 法律与内容

| 项 | 状态 | 说明 |
| --- | --- | --- |
| 著作权 / 授权 | 个人 | 版权人为 `seven <seven.params@gmail.com>`。 |
| 硬编码密钥 | 通过 | 仅有 `.env.example` 占位与本地 Compose 测试口令。 |
| `.env` 历史 | 通过 | 从未跟踪真实 `.env`。 |
| 内部专有信息 | 通过 | 未见客户数据、内部主机名、商业机密标注。 |
| 依赖许可证 | 基本通过 | 直接依赖以 MIT / Apache-2.0 / OFL（字体）为主；详见 [THIRD_PARTY_LICENSES.md](../THIRD_PARTY_LICENSES.md)。 |

## 2. 许可证

- 项目许可证：**Apache License 2.0**（见 [LICENSE](../LICENSE)）
- 版权声明：`Copyright 2026 seven`

## 3. 文档

| 文件 | 状态 |
| --- | --- |
| README.md | 已有安装、安全边界与 License 入口 |
| CONTRIBUTING.md | 已提供 |
| CODE_OF_CONDUCT.md | 已提供 |
| SECURITY.md | 已提供 |
| NOTICE | 已提供 |

## 4. 发布前建议

1. GitHub 启用 private vulnerability reporting。
2. 公开前确认本机 `.env` 不会被提交。
3. 可选：用 `cargo license` 与 npm license checker 复核传递依赖。
