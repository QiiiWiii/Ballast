# 开源准备核对（2026-08-12）

本文件记录公开发布前的检查结果与剩余人工确认项。

## 1. 法律与内容

| 项 | 状态 | 说明 |
| --- | --- | --- |
| 著作权 / 授权 | **需你确认** | Git 作者为 `seven <sevenold@outlook.com>`。若代码归属公司或存在其他贡献者，开源前需公司法务/版权方书面批准，并统一 LICENSE 版权行。 |
| 硬编码密钥 | 通过 | 工作区与 5 条 Git 历史中未发现真实 API Key / 私钥 / 证书；仅有 `.env.example` 占位与本地 Compose 测试口令。 |
| `.env` 历史 | 通过 | 从未跟踪真实 `.env`；`.gitignore` 忽略 `.env` 且保留 `.env.example`。 |
| 内部专有信息 | 通过 | 未见客户数据、内部主机名、商业机密标注；文档中的地址均为 `localhost` 开发示例。 |
| 依赖许可证 | 基本通过 | 直接依赖以 MIT / Apache-2.0 / OFL（字体）为主，未见强制 GPL/AGPL 传染；详见 [THIRD_PARTY_LICENSES.md](../THIRD_PARTY_LICENSES.md)。发布前建议用 `cargo license` 与 npm license checker 再扫一遍传递依赖。 |

## 2. 许可证

- 项目许可证：**Apache License 2.0**（见根目录 [LICENSE](../LICENSE)）
- 选择理由：宽松使用；含专利授权，适合交易基础设施类项目；与当前依赖生态兼容。
- 若你更偏好极简宽松协议，可改为 MIT，但需同步改 `Cargo.toml` / `package.json` / README。

## 3. 文档

| 文件 | 状态 |
| --- | --- |
| README.md | 已有安装与安全边界；已补 License / Contributing 入口 |
| CONTRIBUTING.md | 已扩充 |
| CODE_OF_CONDUCT.md | 已添加（Contributor Covenant 2.1） |
| SECURITY.md | 已扩充报告与基线清单 |
| NOTICE | Apache-2.0 归属声明 |

## 4. 发布前仍建议你亲自完成

1. **版权行定稿**：将 `the Ballast authors` 替换为最终权利主体（个人或公司全称）。
2. **公司批准**：若使用公司邮箱/工时/资产开发，取得开源批准邮件或工单。
3. **GitHub 仓库设置**：启用 private vulnerability reporting；确认 description、topics、默认分支保护。
4. **最终敏感扫描**（可选加固）：
   ```bash
   git rev-list --all | xargs git grep -n -I -e 'BEGIN PRIVATE' -e 'AKIA' || true
   ```
5. **传递依赖许可证扫描**：见 `THIRD_PARTY_LICENSES.md` 末尾命令。
6. **确认无未提交本地密钥**：确保本机 `.env` 永不 `git add`。

## 5. 已知非阻塞项

- 历史提交作者邮箱含公司域名：这不等于泄露密钥，但公开后可被关联到组织；若需匿名化历史，需在**首次公开前**用 `git filter-repo` 重写（会改写 SHA，仅适合尚未广泛 fork 时）。
- `POSTGRES_PASSWORD=change-this-for-shared-environments` 与 Compose 默认 `ballast-local-only` 为本地占位，文档已提示生产必须修改。
