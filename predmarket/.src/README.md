# Windows Kernel Driver

This is a kernel-mode driver for memory management and process monitoring on Windows.

## What it does

- Modifies page table entries for read/write access
- Monitors loaded modules via image load notifications
- Copy-on-write support for safe memory modification
- VMX helper functions for hypervisor stuff

## Building

You need:
- Windows Driver Kit (WDK) 10+
- Visual Studio 2019+ with C++
- Windows 10/11 x64

Run `build.bat` from an admin command prompt, or build manually:

```cmd
call "C:\Program Files (x86)\Windows Kits\10\bin\10.0.22000.0\x64\setenv.bat" fre
build -cZ
```

## Installing

```cmd
pnputil /add-driver polymarketEngine.inf /install
sc start polymarketEngine
```

## Usage

The driver watches module loads and applies memory permissions to whitelisted processes. Default whitelist:
- notepad.exe
- calc.exe
- explorer.exe

Edit `allowedModules` in `LoadImageNotifyRoutine` to change the list.

## Testing

Use DebugView or WinDbg to see output:

```
Polymarket Engine Kernel Manager Loading...
LoadImageNotify: PID=1234, Image=C:\Windows\System32\notepad.exe
Allowed module loaded, applying memory permissions
```

## Files

- `kernelmng.cpp` — main driver
- `deepkernel.h` — kernel structures
- `DBKFunc.h` — DBK functions
- `vmxhelper.h` — virtualization helpers
- `sources` — build config
- `polymarketEngine.inf` — install file
- `build.bat` — build script

## Warning

This is kernel code that modifies memory permissions. Test in a VM first, use test signing mode, and keep backups.
