# ADR 0001：barter-instrument 集成边界

状态：Accepted

Ballast 精确固定 `barter-instrument = 0.3.1`，只复用 `Side` 等能够无损表达的领域原语。原计划指定的 `0.11.0` 没有发布到 crates.io，因此不能形成可重复构建。锁文件同时固定兼容 Rust 1.88 的 `smol_str 0.3.2`。

Ballast 保留交易所、市场类型、合约类型、精度与可选交易限制。不会引入 `barter-execution`，也不会把 Barter 的订单状态当作五家交易所的权威状态模型。

任何版本升级必须通过方向/标的转换、十进制精度、序列化和数据库恢复兼容测试，并同步更新第三方许可证清单。
