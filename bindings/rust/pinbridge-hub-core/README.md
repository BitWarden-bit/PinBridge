# pinbridge-hub-core

Shared domain and adapter contract for PinBridge. `HubService<A>` owns the
control gate, bounded activity journal, script bookkeeping, session state and
all Agent operations. `A` implements `AgentApi`; `AgentConnection` is the
provided request-per-connection implementation. It serializes every logical
operation, calls `Client::connect(agent_port)`, performs the request, and
drops the client before releasing the transport gate. It never launches or
restarts a target.

Adapters call `HubService::call(Caller, method, params)`. The `Caller` is
created by the trusted IPC entrance. Do not accept an actor from tool JSON.
Trusted Human can hand off/take over and change the session port; AI reads in
all modes and writes only in `AiAutonomous`; Human writes only in `Manual`.
Takeover acquires the control write gate, switches to Manual, then pauses the
Agent while the gate remains held.

The default desktop topology embeds one Hub inside Tauri: the human adapter
and its UI poller share the Hub's session, Agent transport, scripts, and
activity timeline. `pinbridge-hub` is the alternative headless Hub process;
it must not be started on the same endpoint as an embedded Hub.

The human-led flow is: attach or launch, inspect and position the target in
Manual, then explicitly hand control to AI. Takeover is always available to a
trusted human and blocks new AI writes before attempting the Agent pause. AI
can make bounded synchronous calls only in the modes allowed by the control
gate. Dynamic scripts are injected into the current target and do not restart
it. The Hub journal stores structured operation metadata and resource
references, never script source or raw memory payloads.

The MCP adapter does not receive the high-frequency raw event stream. The
Hub may use a small internal event snapshot for desktop polling, but event
callback coverage is not promised as an MCP capability.

The local IPC framing is a little-endian u32 length followed by UTF-8 JSON,
with a 2 MiB maximum frame. Hello credentials are channel-specific (`human`
and `ai`) and are compared before requests are dispatched. Secrets must come
from protected environment/inherited configuration and are never echoed or
recorded.
