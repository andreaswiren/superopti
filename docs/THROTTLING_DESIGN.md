# Throttling evidence and pinned monitoring view

Status: implementation plan for the 0.3 redesign. Mockup values are illustrative;
they are not measurements from the user's machine. No live GPU reason collector
or native graph event overlay has passed validation yet.

## Throttling evidence

Graph events need device identity, observation time, reason, telemetry source,
coverage, and duration when the source actually provides it. Clicking an event
selects its time range and corresponding structured evidence row. Preserve
multiple simultaneous reasons. Sampled reasons describe observations at the
capture interval; they do not establish exact transition times between samples.

System health shows detected CPU/GPU limiting, the capture/time window, source,
and a drilldown. Separate thermal protection, power/current limits, firmware
limits and normal idle/application clock settings. Low frequency or high
temperature alone does not establish throttling. Unsupported/denied telemetry
is Unknown, never a healthy result. Protective limits are not disabled by Fix all.

Initial sources:

- CPU: Windows Kernel-Processor-Power Event 37 reports firmware-limited processor
  operation. The on-demand system check reads up to 250 reports from the last
  24 hours. This is historical evidence, not a precise start/end event or a
  thermal diagnosis. No matching reports do not rule out throttling.
- NVIDIA: optional installed-driver NVML clock event reasons distinguish thermal,
  power cap, power brake, idle and configured-clock constraints. Query supported
  reasons first. Generic hardware slowdown is ambiguous and must remain so.
- Intel: IGCL frequency state exposes throttle reason flags, and throttle-time
  counters support interval deltas on supported hardware. Availability must be
  probed per adapter/domain.
- AMD: validate the actual supported driver telemetry API before claiming reason
  coverage. ADLX temperature and clocks alone cannot prove thermal throttling.

Vendor integrations must load trusted installed libraries, remain read-only,
sample only during an explicit capture, and release resources at Stop/deadline.
No driver installation or idle polling is part of this design.

Primary references:

- https://learn.microsoft.com/en-us/troubleshoot/windows-server/setup-upgrade-and-drivers/event-id-37-windows-kernel-processor-power
- https://docs.nvidia.com/deploy/nvml-api/group__nvmlClocksEventReasons.html
- https://intel.github.io/drivers.gpu.control-library/Control/api.html
- https://gpuopen.com/manuals/adlx/adlx-sdk-references/adlx-interfaces/performance-monitoring/

## Always on top, compact view and opacity

Expose a keyboard-accessible pin toggle in the main window and compact window.
Apply HWND_TOPMOST/HWND_NOTOPMOST only to the app's own window. This preference
does not start a capture or require elevation.

Compact mode preserves the active capture, presents CPU/RAM/GPU/disk metrics,
small histories and the leading contributors, and retains Stop, capture time
remaining and Expand. Save the full window's position and size for restoration.
Support resizing and per-monitor DPI in both modes.

Opacity is adjustable only while pinned, using a conservative 50–100% range
(100% by default). Unpinning restores full opacity. Keep input enabled: this is
not a click-through overlay. High contrast should force opacity to 100% and
explain the disabled control. Opacity changes must not add an animation timer.

Validate pin/unpin, Alt-Tab, minimized/tray restore, maximized/full geometry,
compact capture/Stop/Expand, theme changes, DPI transitions, and opacity recovery
before release. The user selected the Signal dashboard composition.

## Process and open-file folder actions

Process details expose Open containing folder beside the full executable path.
The open-files table exposes the same action per selected filesystem file, with
keyboard and context-menu access. Open Explorer and select the item; never run
the executable or file. Use the Windows shell item API rather than composing a
shell command from a filename. Keep long paths copyable and show the exact path
before navigation. Process identity must still match the inspected capture.

Disable navigation with a reason for unavailable paths, named pipes, sockets and
other non-filesystem handles. Report missing/deleted items and access failures as
structured action results. Network locations can require credentials and must
not be accessed merely by rendering the file list. Navigation is an explicit
user action and must not start monitoring or elevate the entire application.

