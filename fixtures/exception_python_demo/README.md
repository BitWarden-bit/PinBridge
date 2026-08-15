# Python 异常接管真实回归

该目标在插件加载后触发一次访问违规。Python 使用
`pb.intercept("exception.handle", ..., codes=[0xC0000005])` 取得异常发生现场和 Pin 的
目标上下文，改写 `rip/rsp` 跳到导出的恢复入口。测试要求原生 SEH 处理器不执行、目标
输出 `RECOVERED` 并以 0 退出，同时校验同步通道为 1 次决定、0 超时、0 槽位拥塞。
同一次访问违规还必须让 `pb.on("exception")`、`pb.on("context.change")` 和旧
`on_exception` 各精确收到一次带 `exception_generation` 的异步观察事件，证明高优先级环
与兼容普通环的双写没有产生重复 Python 回调。

```powershell
.\build.ps1
.\run.ps1
```
