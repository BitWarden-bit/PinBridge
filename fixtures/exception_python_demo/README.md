# Python 异常接管真实回归

该目标在插件加载后先投递一次 Windows APC，再触发一次访问违规。Python 使用
`pb.intercept("exception.handle", ..., codes=[0xC0000005])` 取得异常发生现场和 Pin 的
目标上下文，改写 `rip/rsp` 跳到导出的恢复入口。测试要求原生 SEH 处理器不执行、目标
输出 `RECOVERED` 并以 0 退出，同时校验同步通道为 1 次决定、0 超时、0 槽位拥塞。
APC 必须让 `pb.on("context.change")` 精确收到一次 `reason_name="apc"` 事件；同一次访问
违规还必须让 `pb.on("exception")`、`pb.on("context.change")` 和旧 `on_exception` 各精确
收到一次带统一 `context_generation` 的异步观察事件。测试由此证明所有上下文原因都走
高优先级/兼容双写，且两条通道不会产生重复 Python 回调。

```powershell
.\build.ps1
.\run.ps1
```
