# Python 原生插桩策略真实回归

目标先调用一次 `IncludedFunction`，使 Pin 在未启用事件引擎时编译该函数。Python 随后用
`pb.instrumentation_set` 只允许该函数范围内的指令事件。agent 必须使旧代码缓存失效并按
新规则重新插桩；第二次调用应产生事件，而 `ExcludedFunction` 不得产生事件。
同一规则还验证 `routine.instrument` 的热加载函数快照，以及重新翻译时产生的
`trace.instrument`、`basic_block.instrument`；三类都必须服从同一个原生地址范围。
运行时指令同时通过命名回调和原始批处理回调校验长度与精确策略版本。

```powershell
.\build.ps1
.\run.ps1
```
