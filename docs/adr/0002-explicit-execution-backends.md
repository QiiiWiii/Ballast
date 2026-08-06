# ADR 0002：执行后端必须显式选择

状态：Accepted

策略模板版本必须选择 `managed_ioc` 或 `venue_native_algo`。任务保存不可变模板引用和完整参数快照；运行时不得自动切换后端、订单类型或交易所。

managed 模板可跨交易所共享，由 Ballast 计算 TWAP/POV 切片。native 模板必须绑定交易所、市场和强类型算法配置。只有创建、查询、取消、子订单/成交对账和价格保护全部通过时，能力矩阵才允许提交。

当前所有私有提交能力为 false。存在官方端点只会得到 `documented_not_validated`，不会得到可交易状态。
