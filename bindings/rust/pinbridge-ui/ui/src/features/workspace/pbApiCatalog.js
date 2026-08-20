const api = (group, name, parameters, returns, documentation, snippet = "") => ({
  group,
  name,
  parameters,
  returns,
  documentation,
  signature: `pb.${name}(${parameters})${returns ? ` -> ${returns}` : ""}`,
  snippet: snippet || `${name}()`,
});

// Keep this catalogue aligned with pinbridge-agent/src/scripting/api.rs.
// Monaco uses it for completion, hover, signature help, and the searchable
// in-editor API reference. Every function exported by create_pb_module is
// represented here; nothing in this list is a simulated UI-only capability.
export const PB_API_CATALOG = [
  api("日志", "print", "msg: str", "None", "向当前插件的有界输出流写入一行。", "print(${1:message})"),
  api("日志", "log", "msg: str", "None", "print 的同义接口，输出会携带插件名称。", "log(${1:message})"),

  api("内存与寄存器", "read_mem", "addr: int, length: int", "bytes | None", "读取目标内存；读取失败返回 None。", "read_mem(${1:address}, ${2:length})"),
  api("内存与寄存器", "write_mem", "addr: int, data: bytes", "int", "目标停止时写入内存，返回实际写入字节数。", "write_mem(${1:address}, ${2:data})"),
  api("内存与寄存器", "get_reg", "tid: int, name: str", "int | None", "读取指定线程寄存器。", "get_reg(${1:thread_id}, \"${2:rax}\")"),
  api("内存与寄存器", "set_reg", "tid: int, name: str, value: int", "bool", "写入指定线程寄存器。", "set_reg(${1:thread_id}, \"${2:rax}\", ${3:value})"),
  api("内存与寄存器", "memory_region", "address: int", "tuple | None", "查询地址所在内存区域的基址、大小、保护和类型。", "memory_region(${1:address})"),

  api("断点与执行", "bp_set", "addr: int", "int | None", "创建传统原生断点，不绑定 Python 回调。", "bp_set(${1:address})"),
  api("断点与执行", "bp_remove", "id: int", "bool", "删除传统原生断点。", "bp_remove(${1:breakpoint_id})"),
  api("断点与执行", "breakpoint", "address: int, callback: Callable, *, description: str, once: bool = False, thread_id: int | None = None", "int", "绑定精确断点回调。description 必填；回调可返回 stay、resume、step_into、step_over 或含 action 的字典。", "breakpoint(\n\t${1:address},\n\t${2:on_hit},\n\tdescription=\"${3:说明为什么设置以及命中后做什么}\",\n\tonce=${4:False},\n)"),
  api("断点与执行", "breakpoint_remove", "id: int", "bool", "只移除当前插件拥有的断点回调绑定。", "breakpoint_remove(${1:breakpoint_id})"),
  api("断点与执行", "execution_trap", "start: int, end: int, *, once: bool = True, thread_id: int | None = None", "int", "注册半开地址区间执行监控，在匹配指令执行前停止。", "execution_trap(${1:start}, ${2:end}, once=${3:True})"),
  api("断点与执行", "execution_trap_remove", "id: int", "bool", "移除当前插件的执行监控。", "execution_trap_remove(${1:trap_id})"),
  api("断点与执行", "execution_traps", "", "list[tuple]", "列出当前插件的执行监控。"),
  api("断点与执行", "hit", "", "tuple[int | None, int]", "返回最近断点命中的线程和地址。"),
  api("断点与执行", "is_stopped", "", "bool", "目标当前是否停止。"),
  api("断点与执行", "stop", "", "bool", "请求停止目标。"),
  api("断点与执行", "resume", "", "bool", "继续运行目标。"),
  api("断点与执行", "step", "tid: int, over: bool", "bool", "指定线程单步；over=True 表示单步越过。", "step(${1:thread_id}, over=${2:False})"),
  api("断点与执行", "wait_stop", "timeout_ms: int", "bool", "等待目标停止，直到超时。", "wait_stop(${1:5000})"),
  api("断点与执行", "sleep", "ms: int", "None", "让脚本线程休眠指定毫秒。", "sleep(${1:100})"),

  api("会话与解析", "pin_state", "", "tuple[str, int]", "读取 Pin attach/detach 状态和注册状态。"),
  api("会话与解析", "pin_attach_supported", "", "bool", "当前平台与模式是否支持进程内重新附加。"),
  api("会话与解析", "pin_detach", "", "bool", "异步请求 Pin 分离。"),
  api("会话与解析", "pin_attach", "", "bool", "从分离状态请求重新附加。"),
  api("会话与解析", "resolve", "addr: int", "str | None", "把地址解析为 module!symbol+offset。", "resolve(${1:address})"),
  api("会话与解析", "resolve_name", "spec: str", "int | None", "把 module!export 解析为地址。", "resolve_name(\"${1:module!symbol}\")"),
  api("会话与解析", "disasm", "addr: int, count: int", "list[tuple] | None", "进程内反汇编指定地址；同步接管回调可用。", "disasm(${1:address}, ${2:16})"),
  api("会话与解析", "modules", "", "list[tuple]", "列出已加载模块。"),
  api("会话与解析", "threads", "", "list[int]", "列出应用线程 ID。"),
  api("会话与解析", "counters", "", "tuple | None", "读取事件总量、丢失量、容量和分类计数。"),
  api("会话与解析", "control_port", "", "int", "当前 Agent 查询/控制端口。"),
  api("会话与解析", "parent_control_port", "", "int | None", "跟随子会话的父控制端口。"),
  api("会话与解析", "exports", "module: str", "list[tuple[int, str]]", "列出模块命名导出。", "exports(\"${1:module.dll}\")"),

  api("Hook 与异常", "hook_set", "addr: int", "bool", "挂载原生 Hook 点。", "hook_set(${1:address})"),
  api("Hook 与异常", "hook_events_query", "*, limit: int = 1024, before: int = 0, after: int = 0, order: str = 'desc', hook_types: list[str] | None = None, phases: list[str] | None = None, modules: list[str] | None = None, symbols: list[str] | None = None, thread_ids: list[int] | None = None, addresses: list[int] | None = None", "dict", "查询独立 Hook 事件通道；支持按类型、阶段、模块、符号、线程、地址和序列过滤。同步拦截回调中应直接使用当前事件。", "hook_events_query(limit=${1:256}, modules=[\"${2:*ntdll*}\"] )"),
  api("Hook 与异常", "hook_rule", "addr: int, set_reg: str, set_value: int, match_reg: str | None = None, match_mask: int = 0, match_value: int = 0, thread_id: int | None = None", "bool", "为已挂载 Hook 添加同步原生寄存器规则。", "hook_rule(${1:address}, \"${2:rax}\", ${3:value})"),
  api("Hook 与异常", "hook_rules_clear", "", "None", "清除当前同步 Hook 规则。"),
  api("Hook 与异常", "hook_remove", "addr: int", "bool", "移除一个 Hook 点。", "hook_remove(${1:address})"),
  api("Hook 与异常", "hook_clear", "", "bool", "清除全部 Hook 点。"),
  api("Hook 与异常", "exc_policy", "enabled: bool, code: int = 0", "bool", "设置异常策略及可选异常码。", "exc_policy(${1:True}, code=${2:0})"),

  api("Trace", "trace_start", "path: str, kinds: list[str] | None = None, range: tuple[int, int] | None = None", "bool", "启动文件 Trace。", "trace_start(\"${1:trace.pb}\", kinds=${2:None}, range=${3:None})"),
  api("Trace", "trace_start_spec", "path: str, kinds: list[str] | None = None, ranges: list[tuple] | None = None, threads: list[int] | None = None", "bool", "按多个范围和线程启动 Trace。", "trace_start_spec(\"${1:trace.pb}\", kinds=${2:None}, ranges=${3:None}, threads=${4:None})"),
  api("Trace", "trace_extend", "ranges: list[tuple[int, int]]", "bool", "扩展活动 Trace 的地址范围。", "trace_extend(${1:ranges})"),
  api("Trace", "trace_stop", "", "tuple[int, int] | None", "停止 Trace 并返回统计。"),
  api("Trace", "trace_status", "", "tuple | None", "读取 Trace 简要状态。"),
  api("Trace", "trace_status_detail", "", "tuple | None", "读取 Trace 路径、状态和计数。"),

  api("事件与决策", "on", "event: str, callback: Callable, *, once: bool = False, address: int | None = None, numbers: list[int] | None = None", "int", "订阅异步命名事件；断点必须使用 pb.breakpoint。", "on(\"${1:event}\", ${2:callback}, once=${3:False})"),
  api("事件与决策", "off", "subscription_id: int", "bool", "移除当前插件拥有的命名事件订阅。", "off(${1:subscription_id})"),
  api("事件与决策", "event_names", "", "list[str]", "列出 pb.on 支持的规范事件名。"),
  api("事件与决策", "intercept", "event: str, callback: Callable, *, description: str = \"\", once: bool = False, address: int | None = None, thread_id: int | None = None, numbers: list[int] | None = None, codes: list[int] | None = None", "int", "注册有返回值的同步拦截器；Hook 接管必须提供固定说明。", "intercept(\"${1:hook.entry}\", ${2:callback}, description=\"${3:说明筛选条件与允许的修改}\", once=${4:False})"),
  api("事件与决策", "unintercept", "id: int", "bool", "移除同步拦截器。", "unintercept(${1:interceptor_id})"),
  api("事件与决策", "decision_names", "", "list[str]", "列出同步拦截器支持的事件名。"),
  api("事件与决策", "on_exception", "codes: list[int] | None = None", "None", "启用旧式异常观察过滤。", "on_exception(codes=${1:None})"),
  api("事件与决策", "on_syscall", "numbers: list[int] | None = None", "None", "启用旧式系统调用观察过滤。", "on_syscall(numbers=${1:None})"),
  api("事件与决策", "on_bp", "", "None", "启用旧式断点通知。"),
  api("事件与决策", "on_modules", "", "None", "启用旧式模块通知。"),

  api("原生策略", "instrumentation_set", "*, kinds: list[str] | None = None, ranges: list[tuple] | None = None, threads: list[int] | None = None", "int", "编译当前插件的高频原生采集规则。", "instrumentation_set(kinds=${1:None}, ranges=${2:None}, threads=${3:None})"),
  api("原生策略", "instrumentation_clear", "", "bool", "清除当前插件的原生采集规则。"),
  api("原生策略", "instrumentation_policy", "", "tuple | None", "读取当前插件的原生采集策略。"),
  api("原生策略", "memory_translation_set", "mappings: list[tuple], *, threads: list[int] | None = None, instruction_ranges: list[tuple] | None = None, operations: list[str] | None = None, include_pin: bool = False", "int", "设置内存地址转换映射。", "memory_translation_set(${1:mappings}, threads=${2:None}, instruction_ranges=${3:None}, operations=${4:None})"),
  api("原生策略", "memory_translation_clear", "", "bool", "清除当前插件的地址转换映射。"),
  api("原生策略", "memory_translation_policy", "", "object | None", "读取当前插件的地址转换策略。"),
  api("原生策略", "code_fetch_set", "segments: list[tuple[int, bytes]]", "int", "配置原生代码抓取覆盖段。", "code_fetch_set(${1:segments})"),
  api("原生策略", "code_fetch_clear", "", "bool", "清除代码抓取覆盖段。"),
  api("原生策略", "code_fetch_policy", "", "list[tuple] | None", "读取代码抓取覆盖策略。"),
  api("原生策略", "xed_decode_set", "*, cet: bool | None = None, cldemote: bool | None = None, mpx: bool | None = None", "int", "设置进程级 XED 预解码特性。", "xed_decode_set(cet=${1:None}, cldemote=${2:None}, mpx=${3:None})"),
  api("原生策略", "xed_decode_clear", "", "bool", "清除当前插件的 XED 解码策略。"),
  api("原生策略", "xed_decode_policy", "", "tuple | None", "读取当前插件的 XED 解码策略。"),

  api("兼容订阅", "watch", "kinds: list[str], range: tuple[int, int] | None = None, batch: int | None = None", "None", "设置插件事件观察范围和批量大小。", "watch(${1:kinds}, range=${2:None}, batch=${3:None})"),
  api("兼容订阅", "unsubscribe", "", "None", "清除兼容订阅。"),
  api("兼容订阅", "subscribe", "kinds: list[str], range: tuple[int, int] | None = None, batch: int | None = None", "None", "watch 的兼容别名。", "subscribe(${1:kinds}, range=${2:None}, batch=${3:None})"),
];

export const PB_API_BY_NAME = new Map(PB_API_CATALOG.map((item) => [item.name, item]));
