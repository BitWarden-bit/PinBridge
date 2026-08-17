# pinbridge-hub

Headless Hub process (an alternative deployment to the default Tauri-embedded
Hub). It connects to an already-running Agent and exposes only a loopback TCP
IPC listener; it does not launch or kill a target. The Hub owns the shared
session, control policy, journal and dynamic script service for a human
adapter and the MCP AI adapter. Do not run it on the same endpoint as an
embedded Tauri Hub.

Example:

```text
PINBRIDGE_HUB_HUMAN_SECRET=<protected-secret> \
PINBRIDGE_HUB_AI_SECRET=<different-protected-secret> \
cargo run -p pinbridge-hub -- --agent-port 9001 --listen 9444
```

The listener binds `127.0.0.1` only. Human and AI credentials are distinct;
the client cannot override its actor in a request. MCP disconnects do not
affect the Agent or target process. A trusted human should first locate and
pause/inspect the target in Manual, then explicitly hand off to AI. Takeover
immediately blocks new AI writes before attempting the Agent pause.

On Windows, Ctrl-C and Ctrl-Break request a graceful shutdown: the console
handler only signals the main thread, which stops the IPC server before exit.
Other platforms use the same testable shutdown wait/notification abstraction;
use the process supervisor to request termination there.

The Hub supports bounded synchronous MCP calls, dynamic script injection and a
structured activity timeline. Script injection never restarts the target, and
raw high-frequency event streams are not exposed to MCP.
