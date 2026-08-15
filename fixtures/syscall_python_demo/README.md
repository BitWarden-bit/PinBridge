# Python 系统调用观察与同步拦截真实回归

该用例在真实 x64 Pin 目标中调用 `NtClose`，同时验证两条相互独立的能力：

- `pb.intercept("syscall.entry/exit", numbers=[...])` 同步改写入口参数和出口状态；
- `pb.on("syscall", ..., numbers=[...])` 与旧 `on_syscall` 观察同一原生号码过滤流。

观察事件同时写入 16384 槽独立环和兼容普通环。脚本按 `syscall_generation` 验证每个接口
内部没有重复、两套接口看到相同事件集合；运行器还要求 agent 总结中的
`observation_dropped=0`。Windows 运行库可能产生额外同号系统调用，因此测试核对原生代号
集合，不假设进程一共只调用两次 `NtClose`。Rust 单元测试另外覆盖多线程可能造成的代号
乱序，确认固定 65536 位去重窗口会接受迟到的新事件并拒绝两个通道的重复副本。

```powershell
.\build.ps1
.\run.ps1
```
