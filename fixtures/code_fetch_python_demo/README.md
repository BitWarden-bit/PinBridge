# Python 机器码取码真实回归

目标程序先执行一次返回 `1` 的 `OriginalFunction`，并通过就绪文件完成显式握手。Python 随后读取返回 `2` 的
`ReplacementFunction` 字节，把这些字节预置到 `OriginalFunction` 的虚拟地址，并要求
原生取码策略立即生效。第二次调用必须返回 `2`，以证明 Pin 已丢弃旧翻译并从原生快照
重新取码，取码热路径没有进入 Python。

```powershell
.\build.ps1
.\run.ps1
```
