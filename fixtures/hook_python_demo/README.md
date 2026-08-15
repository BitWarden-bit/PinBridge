# Python Hook 观察与同步拦截真实回归

该用例在真实 x64 Pin 目标上同时验证同一批原生 Hook 点的两种 Python 用法：

- `pb.on("hook.entry/return", ..., address=...)` 自动挂载指定地址，异步接收快照；
- `pb.intercept("hook.entry/return", ..., address=...)` 同步跳过函数或改写返回值。

异步订阅和同步拦截共用地址所有权计数；任何一个一次性处理函数完成时都不会提前拆除
另一个处理函数仍在使用的原生点。异步事件写入独立观察环，普通遥测环只保留 CLI/UI
兼容副本，因此命名 Python 回调不会收到双份事件。入口地址用“异步一次性 + 同步常驻”
连续命中两次，返回地址反过来用“异步常驻 + 同步一次性”连续命中两次，从两个方向证明
一方释放不会破坏另一方。测试还要求三次同步决定生效并且 `observation_dropped=0`。

```powershell
.\build.ps1
.\run.ps1
```
