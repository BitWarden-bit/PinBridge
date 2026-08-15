# Python 控制 Pin 分离/重新附加回归

脚本注册 `pin.detach`/`pin.attach`，调用 `pb.pin_detach()`，在分离完成事件里调用
`pb.pin_attach()`。重新附加后脚本写入目标导出标志，目标才创建新线程并执行此前从未执行的
`AfterAttachTarget`。测试必须重新收到线程和指令事件，从而证明不是只产生一个假的 attach
通知，而是实际恢复了 Pin 会话回调与原生插桩策略。

Intel Pin 3.31 的 Windows JIT 运行时会在调用 `PIN_Attach` 时直接终止目标并报告 NYI。
因此 Windows 回归验证 `pb.pin_attach_supported() == False` 且安全跳过分离；支持 JIT
重附加的平台才执行完整往返。Probe 模式仍使用 Pin 提供的 `PIN_AttachProbed`。

```powershell
.\build.ps1
.\run.ps1
```
