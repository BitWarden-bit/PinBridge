# UI Workspace Handoff

Status: interactive design prototype; backend integration is intentionally incomplete.

## Product decisions confirmed

- The application is one shared human/AI workspace, not separate human and AI pages.
- The main workspace is permanently split in half:
  - Left: the traditional debugger and raw evidence plane.
  - Right: code-driven automation and AI operations.
- The left side remains visible while the right side changes views.
- The left debugger reuses the original `Toolbar`, `DisasmView`, `Registers`, `StatsPanel`, and `BottomTabs` components.
- The left side owns raw/manual debugger operations: disassembly, native breakpoints, registers, memory, stack, modules, and event evidence.
- The right side owns:
  - breakpoint strategies;
  - exception rules;
  - hooks;
  - trace strategies;
  - dynamic Python script groups;
  - structured AI/MCP activity.
- A native breakpoint and a breakpoint strategy are different assets:
  - Native breakpoint: temporary address-level debugger primitive used manually.
  - Breakpoint strategy: conditions, thread filters, Python callback, action, ownership, and audit history.
- Complex unpacking work is represented as a dynamic script group. There is no separate “analysis plan” page.
- The visual direction is the original flat PinBridge black/gray/white palette. Glass effects, animated backgrounds, and the top view-switching menu were rejected.
- Detail views return to the right-side dashboard through the main top bar. The debugger on the left does not disappear.

## Current prototype behavior

- The UI currently uses clearly simulated VMP/OEP data for visual review.
- The left debugger renders the original components with preview rows/registers.
- The right dashboard shows runtime-shaped previews for breakpoint strategies, exceptions, hooks, trace, script groups, and AI/MCP calls.
- Clicking a right-side tile replaces only the right-side content.
- Python source remains visible and editable in the right-side detail view.
- The currently launched release process at handoff was built from these sources; process state itself is not part of the checkpoint.

## Important implementation files

- `bindings/rust/pinbridge-ui/ui/src/App.jsx`
- `bindings/rust/pinbridge-ui/ui/src/features/workspace/UnifiedWorkbench.jsx`
- `bindings/rust/pinbridge-ui/ui/src/style.css`

The previous application remains preserved as `LegacyDebuggerApp` in `App.jsx`. Original debugger component files were not deleted.

## Verification completed

- `npm run build`
- `cargo build --release -p pinbridge-ui`

## Next work

1. Replace simulated values with the real session/store/MCP data.
2. Connect the persistent left debugger to the active target instead of preview rows.
3. Add explicit left-to-right actions, for example “create strategy from selected address” and “send current context to AI”.
4. Implement breakpoint-strategy/native-breakpoint ownership and conflict reporting.
5. Connect exception, hook, trace, and script detail forms to the real Python registration APIs.
6. Consolidate the prototype CSS after the product layout is accepted; the current stylesheet intentionally preserves iterative visual checkpoints.

