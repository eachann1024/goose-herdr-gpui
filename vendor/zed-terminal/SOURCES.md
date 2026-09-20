# Zed terminal extraction

- Upstream: https://github.com/zed-industries/zed
- Pinned revision: `69e2130295c2649963eb639fc70b4f2ee8ea1624` (the revision recorded by GPUI 0.2.2's `.cargo_vcs_info.json`).
- License: **GPL-3.0-or-later**. Full license: `LICENSE-GPL`. Upstream copyright belongs to Zed Industries, Inc. and contributors. This is not covered by GPUI's Apache license.
- Extracted 2026-09-19; modified for Goose Herdr GPUI.

## Included upstream implementation

- `crates/terminal/src/terminal.rs`: PTY/EventLoop ownership, 4 ms event coalescing, terminal snapshots, selection, search, paste, mouse handling, scrolling, focus reports and shutdown.
- `crates/terminal/src/mappings/{mod,keys,mouse,colors}.rs`: terminal input encoding.
- `crates/terminal/src/terminal_hyperlinks.rs`: URL/path recognition.
- `crates/terminal_view/src/terminal_element.rs`: `BatchedTextRun`, background-region merging, `layout_grid`, cell styling and batched painting.

## Host adaptations

Removed Zed project/workspace, persistence, task-runner, process-inspector, telemetry and global settings integration. Constructor accepts Alacritty PTY options; Herdr retains its existing command/session ownership. Adapted GPUI scroll modifiers and Alacritty 0.26 exit status. Palette is supplied by the host; APCA contrast rewriting is omitted to preserve configured ANSI colors. OSC clipboard access remains denied. Added input/resize queue error reporting, zero-size guard, bounds-checked hyperlink hit testing, hidden-cell suppression and a synchronous first-match search entry point.

macOS Command/Option line and word editing (cmd-backspace/delete/left/right, option-left/right/delete) is encoded as readline sequences in `mappings/keys.rs`.

`src/terminal.rs` in the host supplies focus/actions, font and palette preferences, selection/cursor/IME overlays and Herdr attach-conflict feedback. This is an extraction of Zed terminal core and grid rendering, **not** an unchanged embedding of its full `terminal_view` crate. Upstream editor-dependent tests are not included; a small host regression check covers batching, hidden cells, key encoding and IME/attach handling.

If distributing a binary incorporating this GPL code, include applicable notices and provide corresponding source under the applicable GPL requirements. Merely bundling this notice is not a complete distribution-compliance determination.

## PTY protocol extensions

`src/event_loop.rs` is adapted from `alacritty_terminal 0.26.0` (`94e7c8874e526b1e67b349d9ba30ddf81669119e`, `alacritty_terminal/src/event_loop.rs`), under Apache-2.0; its license is `LICENSE-APACHE-ALACRITTY`. It retains the upstream PTY polling, synchronized-update timeout, resize, exit and write queues. Changes route Kitty graphics and CSI 996/2031 through a bounded per-PTY parser, and serialize host theme notifications with other terminal replies. Unix polling tokens are matched to the pinned release.

`src/protocol.rs` and `src/images.rs` are host extensions. Graphics support is limited to bounded direct PNG/RGB/RGBA transmission, placements/cropping and deletion used by Pi and Herdr's direct-graphics bridge; file/shared-memory transports, compression and Unicode placeholders are not advertised. Preview anchors use private cell metadata, follow grid scrolling/reflow and cannot be opened as hyperlinks. Theme notifications require a per-PTY subscription; OSC color requests still use the host palette. Existing locked image/base64/polling crates are reused.
