# pinbridge-mcp

`pinbridge-mcp` is a stdio MCP adapter for the shared PinBridge Hub. It does
not own an Agent connection, target lifecycle, control state, scripts, or an
activity journal. Hub supplies the caller identity (`ai` for this channel),
authorization, operation IDs, and activity records.

## Running

Configure an explicit Hub endpoint and AI credential out of band:

```text
PINBRIDGE_HUB_ENDPOINT=<endpoint> PINBRIDGE_HUB_AI_SECRET=<credential> \
  cargo run -p pinbridge-mcp
```

`--hub-endpoint ENDPOINT` is also accepted. Credentials are deliberately not
accepted as command-line arguments and are never logged or returned. The
adapter never accepts an Agent port, never launches/restarts/kills a target,
and a disconnected MCP process cannot stop a Hub or target.

If Hub is unavailable, tool calls return a clear MCP `CallToolResult` with
`isError: true` and a session/control-unavailable message. No fallback direct
Agent connection is attempted.

## Exposed AI tools

The AI channel exposes control status, read-only inspection, Hub-policy-
controlled AI writes, dynamic scripts, and Hub activity queries.
Human-only handoff, takeover, session connect, launch, and kill tools are not
exposed. AI cannot claim `actor`, self-authorize a human handoff, or switch
itself to `AiAutonomous`; trusted Tauri/Hub APIs perform those operations.

In the default Tauri deployment, the desktop process reads
`PINBRIDGE_HUB_HUMAN_SECRET`, `PINBRIDGE_HUB_AI_SECRET`, and its listener
configuration from `PINBRIDGE_HUB_PORT` or `--hub-listen`. If either secret is
missing or invalid, Tauri remains usable in Manual mode but does not start Hub
IPC and does not enable AI handoff. MCP connects using
`PINBRIDGE_HUB_ENDPOINT` or `--hub-endpoint` only after both secrets are
valid. The headless `pinbridge-hub` binary is an alternative deployment and
must not share the embedded Hub endpoint.

The adapter forwards purpose and parent operation metadata to Hub. It does not
create a second journal. Hub operation IDs are returned in `structuredContent`
when supplied. Addresses, IDs, counters, and large integers remain strings;
memory and instruction bytes use explicit hex encoding.

The stdio process is the AI adapter to the same Hub used by the desktop
operator. It does not open a second Agent connection. In the default desktop
deployment Tauri embeds and owns the Hub; `pinbridge-hub` is an alternative
headless deployment for environments that provide their own human adapter.
Do not run both deployments against the same Hub endpoint.

MCP exposes bounded synchronous inspection and control calls, dynamic script
inject/replace/remove, and structured activity queries. It intentionally does
not expose the high-frequency raw event stream; event snapshots remain an
internal desktop polling capability. Complex scripts are injected into the
already-running Agent and never restart the target.
