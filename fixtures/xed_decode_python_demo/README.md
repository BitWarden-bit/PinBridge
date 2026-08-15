# Python XED 解码策略真实 Pin 回归

该夹具同时验证两条独立链路：

- `pb.xed_decode_set(cldemote=True)` 在 Pin 解码前由原生回调应用；
- `pb.instrumentation_set(kinds=["instruction.decode"], ...)` 在解码完成后按地址过滤，
  `pb.on("instruction.decode", ...)` 与 `on_event_batch` 都能收到复制后的静态元数据。

借用的 INS/XED 句柄不会进入 Python。测试目标包含官方 Pin 测试所用的 `0F 1C 00`
CLDEMOTE 编码；即使物理 CPU 不支持该指令，异常也由夹具捕获，不影响解码验证。

```powershell
.\build.ps1
.\run.ps1
```
