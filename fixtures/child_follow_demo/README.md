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
随后 `bad_runtime/child_decision.py` 会成功编译、注册一个错误决定，再在 `pb_init()` 抛出
异常；新插件资源必须被撤销，旧插件恢复 running。`bad_policy/child_decision.py` 则完整执行
初始化，但其 XED 设置与独立的 `policy_guard.py` 冲突；提交校验必须拒绝它并再次恢复旧版。
最后加载 `good/child_decision.py`，要求新插件提交为公开版本并调用旧插件的 `on_unload`，
真实子进程决定由这个新版本完成。

```powershell
.\build.ps1
.\run.ps1 -Follow $false
.\run.ps1 -Follow $true
```
