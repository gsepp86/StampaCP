# StampaCP native port monitor (Windows x64)

This directory contains the first native Windows print-monitor implementation.

## Current scope

- Builds a 64-bit Windows print-monitor DLL against the Windows SDK.
- Exposes `InitializePrintMonitor2` and a `MONITOR2` callback table.
- Provides the fixed port name `StampaCP:` through `EnumPorts`.
- Receives a print job between `StartDocPort` and `EndDocPort`, writing the stream to a temporary `.xps` file.
- At end of job, attempts to launch the installed `StampaCP.exe` in the active interactive console session and passes `--spool-job <file>`.
- Grants the active user's token read/delete access to that job file. The GUI removes the temporary file when it closes.

## Important limitations — not ready for end users

1. The installer does not yet register this DLL as a Windows print monitor, create the printer queue, or remove the monitor on uninstall.
2. The correct XPS-producing driver/print processor combination still needs validation on Windows 10 LTSC x64. A queue configured with an arbitrary driver may send a different spool format.
3. The current StampaCP GUI accepts the job path, but its preview is still a placeholder; XPS parsing/rendering and the XPS-to-PDF converter are not implemented.
4. Interactive launch is intentionally limited to the active local console session. Remote sessions, disconnected sessions, multiple logged-in users, service hardening, and recovery after a failed launch need additional design and testing.
5. This DLL is loaded inside the privileged Print Spooler process. Do not install it on a production machine until code review, CI, installation/uninstallation tests, and real print-job tests have passed.

## Build

Run from a Visual Studio 2022 Developer PowerShell or a Windows GitHub Actions runner:

```powershell
cmake -S native/port-monitor -B build/port-monitor -G "Visual Studio 17 2022" -A x64
cmake --build build/port-monitor --config Release
```

The resulting DLL is `build/port-monitor/Release/StampaCPMonitor.dll`.

## Windows print-monitor contract

The implementation follows the Windows `MONITOR2` contract and the StartDocPort/WritePort/EndDocPort job lifecycle. See:

- https://learn.microsoft.com/windows-hardware/drivers/print/initializing-a-print-monitor
- https://learn.microsoft.com/windows-hardware/drivers/print/port-monitor-server-dll-functions
- https://learn.microsoft.com/windows-hardware/drivers/print/printing-a-print-job
- https://learn.microsoft.com/windows-hardware/drivers/ddi/winsplp/nf-winsplp-xcvdataport
