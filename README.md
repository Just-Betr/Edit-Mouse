# Edit Mouse

Edit Mouse is a tray app for detecting external mice and remapping buttons per device. It supports a system theme mode, startup toggle, and per-device button configuration.

## Features
- Detect external mice and select a device
- Per-device button remapping (Left/Right/Middle/Button 4/Button 5)
- Theme: Light, Dark, or System
- Run at startup toggle
- Tray icon with Show/Hide/Quit

## Development

```sh
npm install
npm run dev
```

Or run the backend directly:

```sh
cargo run
```

## Notes
- macOS global remapping uses an event tap and requires Input Monitoring permission.

## License
BSD-3-Clause. See `LICENSE`.
