# Python 原生插桩策略真实回归

目标先调用一次 `IncludedFunction`，使 Pin 在未启用事件引擎时编译该函数。Python 随后用
`pb.instrumentation_set` 只允许该函数范围内的指令事件。agent 必须使旧代码缓存失效并按
新规则重新插桩；第二次调用应产生事件，而 `ExcludedFunction` 不得产生事件。

```powershell
.\build.ps1
.\run.ps1
```
