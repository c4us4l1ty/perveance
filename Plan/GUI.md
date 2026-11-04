# GUI and visual direction

## Reference interpretation

The owner supplied a dark desktop application screenshot: nearly black/charcoal panels, subtle separators, compact sidebar rows, small simple outline icons, a muted light-text hierarchy and lime-green accents for selection/status. Spacing is restrained with rounded row backgrounds. It is **visual inspiration only**—do not copy another product's name, branding, content, layout pixel-for-pixel or proprietary icons. Use appropriately licensed icons or create original ones. The result must be a real art editor, not a duplicate of an automation list.

## Information architecture

- **Left rail:** project navigation, recent files, assets, capabilities and task history; collapsible to preserve canvas space.
- **Top bar:** project name and save state, undo/redo, workspace controls, export and a clear active-task indicator.
- **Central canvas:** dominant editing area with checkerboard transparency, zoom/pan, pixel-grid option, tool cursor and visible selection.
- **Tool strip:** select, move, pencil, eraser, fill, color picker; additional tools appear only if supported/installed.
- **Right inspector:** ordered editable layers/objects, properties and color/palette controls; accessible tabs or collapsible panels.
- **Agent drawer:** conversational prompt, work-in-progress timeline, capability-building progress, policy/budget status, artifacts and stop button. Can close without stopping a task unless explicitly cancelled.
- **Capabilities view:** installed/staged/failed, version, tests, permissions, history and rollback. Label a generated capability as such; never equate successful tests with a safety certification.
- **Settings:** configured model provider, key-management status (never display full key), autonomy policy, isolation backend status, limits, data sharing and diagnostics.

An empty-state onboarding flow should create/open a project and explain local vs model-backed workflows. Non-AI pixel editing remains functional without a key. When the user requests missing functionality, show discovery → build → test → activation → artwork, without interrupting routine autonomous work. Errors explain whether work was stopped by budget, failed tests, unsupported backend or missing permissions.

## Design tokens and interaction

Start with approximate charcoal backgrounds (#151515 and #1d1d1d), subtle borders (#30302a), off-white text (#e5e5e2), muted text (#a0a09a) and a restrained lime accent (#a6d74a). These are proposed, not copied brand values; validate actual contrast. Use a consistent 4/8 px spacing scale, small-radius surfaces, unobtrusive tooltips and coherent 16–20 px outline icons. Prioritize canvas space; avoid cramming a desktop editor into the reference screenshot's list layout.

Provide keyboard navigation, visible focus and non-color status indicators, scalable text, adjustable panel widths and sensible minimum window behavior. Design for mouse, trackpad and eventually pen/tablet input; pressure-sensitive drawing is future work until tested. Pixel grids, nearest-neighbor zoom and layer ordering must remain legible at different scales.

## Key states to design and test

Empty workspace; pixel editing; agent creating art; building a capability; isolation unavailable; budget exhausted; failed verification; activated feature with rollback; incompatible project; conflicting external changes; export complete. Verify GUI-to-headless round trips and accessibility with actual implementation rather than treating this plan as a screenshot specification.
