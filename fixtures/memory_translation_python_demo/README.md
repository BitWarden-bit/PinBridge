# Python 内存地址转换真实回归

Python 将 `SourceValue` 的 8 字节范围映射到 `BackingValue`，并把规则限制到
`ReadMappedSource` 内的读取操作。目标对源变量的物理写入保持原址；只有指定函数的读取
被 Pin 原生回调转换到 backing 地址。

```powershell
.\build.ps1
.\run.ps1
```
