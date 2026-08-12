# Exchange Gateway

该服务是 Ballast 与交易所之间的基础设施边界，使用 TypeScript、ccxt 和 gRPC。

当前实现五家公共行情与历史数据、OKX 只读账户快照，以及按 `client_order_id` 的 OKX 普通订单查询。下单、取消、私有订单流和原生算法提交保持禁用。添加交易所能力时必须通过明确适配器和 Protobuf 类型化接口实现，不得增加通用 ccxt 方法透传。

```bash
npm install
npm run check
npm run dev
```

真实凭证只能由运行环境注入，不得写入配置文件或日志。
