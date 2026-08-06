# Ballast Web

React、TypeScript 和 Vite 实现的中英文“压舱控制舱”。页面按工作流组织为控制舱、执行任务、策略中心、执行分析、市场与通道、系统状态和后续能力。

创建任务必须引用后端持久化的有效策略模板版本。界面不会跨资产累加数量，也不会把未订阅的行情流显示成已连接。

```bash
npm ci
npm run dev
npm run check
```

开发服务器把 `/api` 和 `/health` 代理到 `http://localhost:8080`。界面不使用模拟业务数据；后端不可用时会显示明确错误。
