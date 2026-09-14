# SuperOpti desktop experience

## Design intent

SuperOpti should be calm while the computer is healthy and immediately useful during slowness. The primary action is a short, bounded capture. A completed capture remains readable; hiding the window never implies that recording has stopped. The header and tray tooltip always distinguish idle, capturing and completed states.

The visual system uses a pale slate workspace, white trend cards, dark navy typography and a restrained teal identity. Segoe UI and Windows common controls retain familiar keyboard behavior without a browser, web renderer, chart package or animation runtime. Secondary metrics use distinct colors and explicit names; color is never the only identifier.

## Information architecture

- **Overview:** persistent capture/export controls, six comparable percentage trend cards, native top-ten process table, thread/process investigation actions and a compact system pressure strip.
- **System checks:** an explicit check action previews the machine's pagefile target, three optional reversible settings, an Undo action and separate links to Windows settings. Fix all names its exact scope and requires confirmation; storage, startup and update reviews remain manual.
- **Settings:** explicit per-user autostart, installation, local capture folder and Exit actions. Copy explains that autostart does not start sampling.
- **Process detail:** double-click or press Enter on a selected contributor. A focused page requests one detailed report, remembers process creation identity, and offers Back, thread inspection and a separately confirmed ten-second TCP measurement. It does not create an idle refresh loop.

Capture controls remain in a stable location on every page. They are disabled appropriately while capturing; Stop is disabled when idle and Export is disabled until data exists. Existing worker messages and reports retain their meaning.

The requested fixed pagefile policy uses `max(50% of installed physical RAM, 16 GiB)` with initial and maximum sizes equal. A dedicated **Set fixed pagefile** button applies that policy; **Fix all (3 settings)** includes it alongside animations and power. Confirmation explains administrator rights, the disk-space guard, restart requirements, and saved Undo state. SuperOpti does not elevate or restart Windows automatically. System checks supplies the exact target preview before the user applies a change.

## Graph semantics

CPU, physical memory, GPU, disk busy, commit and pagefile usage all use a fixed 0–100% scale and a two-minute time window. The window moves only when an actual sample arrives. Early captures show unused space for the remainder of the window rather than stretching two samples across a full history. Missing values break the line and show N/A. Before the first capture, cards explicitly say “Awaiting capture.” No sample values are synthesized or interpolated across missing observations.

Commit is committed memory relative to its limit; pagefile usage is separate from physical RAM. Aggregate disk busy and busiest-engine GPU are not interchangeable with individual process attribution. The table calls its ranking a clue rather than proof of causation. The detail report distinguishes process I/O from network traffic and exposes unavailable network measurements explicitly.

## Native interaction and rendering

- A Windows report ListView replaces the fixed-width text table. Columns remain legible at native font sizes, scroll horizontally, support row selection and native accessibility, and keep PID as row data rather than reading it from formatted display text.
- Selection is retained by PID during ranking updates. Process identity is checked using creation time when available before requesting details.
- All actions use keyboard-focusable native buttons, the sort chooser is a native combo box, and reports are selectable, read-only edit controls.
- Content uses logical layout units scaled to system DPI. Trend cards and tables expand horizontally; process and report areas use the available height. The minimum window size preserves controls instead of allowing overlap.
- GDI draws into an offscreen bitmap before one blit; the ListView uses its own double buffering. Only user/window events and actual worker samples invalidate the view. No timer or animation runs while idle.
- This release is **system-DPI aware**, not per-monitor-DPI aware. Windows may bitmap-scale the window after moving between monitors with different scaling factors. Full custom-chart screen-reader descriptions and high-contrast theme adaptation remain follow-up accessibility work; native process/report data and controls remain accessible.

## Original icon family

`assets/superopti.svg` is the editable pulse-mark source, and `assets/superopti.png` is the README image. The teal rounded square and white trace signify performance observation, not security certification. `assets/superopti.ico` includes 16, 20, 24, 32, 40, 48, 64, 128 and 256-pixel images for the executable, title bar and idle tray state. The recording icon adds a static amber indicator and uses an explicit active tooltip; no icon animation is required.

Assets are original project artwork under the repository MIT license. `python assets/generate_icons.py` reproduces raster/ICO assets using Pillow. Pillow is an optional artwork-generation tool and is not used by the app or its distribution build. The checked-in ICO files are embedded by the Windows SDK resource compiler. The resource version comes from the Cargo package version.

## Validation checklist

Compiler, formatting, lint, unit and collector smoke checks are run as part of release integration. Manual UI checks still required before claiming full visual/accessibility QA:

1. Review idle, warming up, active, stopped and unavailable-counter states at 100%, 150% and 200% scaling.
2. Resize to the minimum and a wide layout; check column scrolling, read-only report scrolling, focus visibility and tab order.
3. Select a row, allow its rank to change, and open details with both double-click and Enter; verify PID/creation identity remains correct.
4. Check both light and dark Windows taskbars at 16/20/24 pixels and verify active/idle tooltips.
5. Inspect all pages with keyboard-only navigation and Windows Narrator; record the custom-chart accessibility limitation above.
6. Confirm that closing/minimizing hides to the tray, a stopped capture leaves no sampler active, and Explorer tray recreation restores the correct icon.

A fresh desktop automation attempt during integration failed because the helper repeatedly returned a stale window ID after refresh. The native window launched, but visual/interactive validation remains outstanding; the checklist above is not marked passed.
