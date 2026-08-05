# Exchange Gateway

该服务是 Ballast 与交易所之间的基础设施边界，使用 TypeScript、ccxt 和 gRPC。

当前只实现健康检查，其余 RPC 会明确返回 `UNIMPLEMENTED`。添加交易所能力时必须通过 `ExchangeAdapter` 和 Protobuf 的类型化接口实现，不得增加通用 ccxt 方法透传。

```bash
npm install
npm run check
npm run dev
```

真实凭证只能由运行环境注入，不得写入配置文件或日志。
