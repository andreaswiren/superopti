# CPU placement and pagefile measurements

Use **Observe cores (5s)** in Overview, CPU details or Process Explorer. Approve
Windows UAC when requested. The main app remains unelevated; a restricted helper
collects five seconds of kernel context-switch events. The recorder stops at its
deadline and has an independent cleanup watchdog. Nothing samples while idle.

**Observed cores** lists logical CPU indexes on which the process's incoming
threads were scheduled during that observation. It does not represent CPU
affinity, a physical-core number, current placement, or time spent on each CPU.
Processes can migrate between CPUs. Ranges such as `0–3, 8` are inclusive.
Process Explorer includes the completion timestamp. Capture exports contain a
separate `core_observation` report; its interval is distinct from the performance
capture. Repeat the action to refresh it.

Attribution is limited to up to 1,000 readable processes present both before and after the
observation with matching creation times. New or exited processes are not assigned
to surviving/reused PIDs. Thread lifecycle events invalidate previous owners.
Unknown event versions are ignored, not guessed. `Not observed` means no mapped
incoming context switch was seen, not that the process could never run there.
Lost ETW events/buffers invalidate the observation and produce an error.

The Memory card shows system commit and pagefile utilization percentages. The
process table retains measured RAM and private commit. It does not display a
fabricated swap value: Windows `PROCESS_MEMORY_COUNTERS_EX.PagefileUsage` is
commit charge, not actual swapped-out bytes. Subtracting working set from private
commit is not a valid measurement of pagefile residency. Process details explain
this limitation. Missing system counters remain N/A rather than zero.

References:
- [Microsoft: CSwitch events](https://learn.microsoft.com/en-us/windows/win32/etw/cswitch)
- [Microsoft: PROCESS_MEMORY_COUNTERS_EX](https://learn.microsoft.com/en-us/windows/win32/api/psapi/ns-psapi-process_memory_counters_ex)
