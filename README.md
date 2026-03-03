# FanControl Windows

Tauri + Rust implementation of the original FanControl app experience for Windows.

## Features

- Fan picker with single-fan and all-fans control
- Mode actions: Auto, Min, Max, Preset
- Manual control retry loop and progress feedback
- Temperature averages (CPU, GPU, Battery) and expanded sensor list
- Error banner with copy and dismiss controls
- Settings for unit, precision, launch behavior, and debug sensor export
- Launch-at-login toggle (Windows registry `Run` key)

## Hardware Provider

The app uses **LibreHardwareMonitor WMI** on Windows when available.

If WMI data is unavailable, the app starts in simulation mode so the UI and control logic remain usable.

To use real hardware data:

1. Install and run LibreHardwareMonitor
2. Enable WMI support
3. Keep LibreHardwareMonitor running while FanControl is open

## Development

```bash
npm install
npm run dev
```

## Build

```bash
npm run build:windows
```

Generated installers are created under `src-tauri/target/release/bundle`.

## CI

Windows CI is configured in:

- `.github/workflows/build-windows.yml`
