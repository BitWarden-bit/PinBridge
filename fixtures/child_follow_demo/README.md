# Python 子进程跟随与独立控制面回归

该用例验证 `pb.intercept("child.follow", ...)` 的两条真实 Pin 路径：

- `-Follow $false`：Python 明确拒绝，子目标正常运行但不由 Pin 跟随；
- `-Follow $true`：Python 同意后，父 agent 为子会话分配不同的回环端口并改写 Pin 子命令行。

跟随路径不以原生计数为终点。运行器从父 Python 事件取得子端口，连接子查询服务，热加载
`child_session.py`，并由子 Python 校验 `pb.control_port()` 与
`pb.parent_control_port()`。子插件完成后创建 PID 专属握手文件，目标才退出。运行器同时要求
`child_config_failures=0`，且父子日志使用不同文件。

在子进程创建前，运行器还会尝试加载 `bad/child_decision.py`。它与正在运行的决策插件同名但
故意包含语法错误；加载必须失败，并且原插件仍保持 running、随后继续完成真实跟随决定。

```powershell
.\build.ps1
.\run.ps1 -Follow $false
.\run.ps1 -Follow $true
```
