# flutter-cli

Flutter app inspection CLI using Dart VM Service Protocol.

## Architecture

```
src/
  main.rs          - CLI entry point, clap argument parsing
  commands.rs      - Command implementations (snapshot, screenshot, details, etc.)
  vm_service.rs    - WebSocket JSON-RPC client for Dart VM Service Protocol
  isolate.rs       - Flutter isolate discovery (finds isolate with ext.flutter.* extensions)
  snapshot.rs      - Widget tree parsing, formatting, filtering, compact mode
  process.rs       - flutter run process lifecycle (start, connect, restart, stop)
  state.rs         - Process state persistence (/tmp/claude/flutter-cli/<hash>.json)
  config.rs        - .flutter-cli.toml project config
```

## Key patterns

- All commands go through `process::ensure_connection()` which either connects to `--url` or manages a `flutter run --machine` process
- VM Service communication uses JSON-RPC 2.0 over WebSocket (`vm_service.rs`)
- Process state is keyed by SHA-256 hash of the project directory path
- `snapshot.rs` has compact mode that skips framework-internal widgets (Padding, Center, etc.) and promotes their children

## Build and test

```bash
cargo build
cargo test
cargo clippy
```

## State files

- Process state: `/tmp/claude/flutter-cli/<hash>.json`
- Stderr logs: `/tmp/claude/flutter-cli/<hash>.stderr`
