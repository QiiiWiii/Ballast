# Ballast Web

React、TypeScript 和 Vite 实现的中英文机构执行工作台。页面按执行工作流、数据与通道、安全与系统分组，核心对象为执行任务、不可变策略版本、交易通道和审计证据。

桌面端使用固定导航、全局环境状态栏、宽执行 blotter 和证据详情；移动端切换为底部工作流导航与无横向溢出的任务视图。视觉保持 Keel/Fog/Salt/Depth/Copper 色板，“龙骨稳定轴”只表达纸面执行残余，不伪装成账户净敞口。

创建任务必须引用后端持久化的有效策略模板版本。界面不会跨资产累加数量，也不会把未订阅的行情流显示成已连接。

```bash
npm ci
npm run dev
npm run check
```

开发服务器把 `/api` 和 `/health` 代理到 `http://localhost:8080`。界面不使用模拟业务数据；后端不可用时会显示明确错误。

默认未配置 OIDC 时保持 paper/open 模式。本地启用登录需同时设置 `VITE_BALLAST_OIDC_ISSUER`、`VITE_BALLAST_OIDC_CLIENT_ID`，并按 provider 要求设置可选的 `VITE_BALLAST_OIDC_AUDIENCE`。redirect URI 固定为 `http://localhost:5173/auth/callback`。
