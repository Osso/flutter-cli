# flutter-cli

Flutter app inspection CLI using Dart VM Service Protocol. Connects to running Flutter apps to inspect widget trees, take screenshots, trigger hot reload, and more.

## Installation

### From releases

Download the latest binary from [releases](https://github.com/Osso/flutter-cli/releases):

```bash
# Linux amd64
curl -L https://github.com/Osso/flutter-cli/releases/latest/download/flutter-cli-linux-amd64 -o flutter-cli
chmod +x flutter-cli
sudo mv flutter-cli /usr/local/bin/
```

### From source

```bash
cargo install --git https://github.com/Osso/flutter-cli
```

## Prerequisites

- Flutter SDK installed and on PATH
- A Flutter app (running or in a project directory)

## Usage

flutter-cli can either connect to an already-running VM Service (`--url`) or manage a `flutter run` process automatically.

### Widget tree inspection

```bash
flutter-cli snapshot                    # Full widget tree
flutter-cli snapshot --compact          # Skip framework-internal widgets
flutter-cli snapshot --depth 3          # Limit tree depth
flutter-cli snapshot --filter "NavBar"  # Filter by widget name (substring)
flutter-cli snapshot --filter "Comic*"  # Filter with glob pattern
```

Output includes widget type, value ID, and source location:

```
MaterialApp  [inspector-0] main.dart:12
  Scaffold  [inspector-2] home_page.dart:8
    AppBar  [inspector-4] home_page.dart:10
    Text "Hello"  [inspector-6] home_page.dart:15
```

### Widget details and layout

```bash
flutter-cli details <value-id>          # Widget properties (value ID from snapshot)
flutter-cli details <value-id> -d 4     # With deeper subtree
flutter-cli layout <value-id>           # Layout constraints, sizes, flex
```

### Screenshots

```bash
flutter-cli screenshot                  # Save to /tmp/claude/flutter-screenshot.png
flutter-cli screenshot output.png       # Custom path
flutter-cli screenshot --id <value-id>  # Screenshot specific widget
```

### Render and semantics trees

```bash
flutter-cli dump-render                 # Render tree text dump
flutter-cli dump-semantics              # Semantics tree text dump
```

### Hot reload / restart

```bash
flutter-cli reload                      # Hot reload
flutter-cli restart                     # Hot restart (requires managed process)
```

### Process management

```bash
flutter-cli status                      # Connection info and process status
flutter-cli stop                        # Kill managed flutter run process
```

### Global options

```bash
flutter-cli --url ws://127.0.0.1:PORT/ws  # Connect to specific VM Service
flutter-cli --json snapshot                # JSON output
flutter-cli --project-dir /path/to/app     # Specify project directory
```

## Configuration

Place a `.flutter-cli.toml` in your Flutter project root to configure how `flutter run` is launched:

```toml
device = "chrome"                          # Device ID (or "auto")
flavor = "development"                     # Build flavor
target = "lib/main_dev.dart"               # Entry point
dart_define_from_file = ".env"             # Dart defines file
extra_args = ["--web-port=8080"]           # Additional flutter run args
```

## How it works

1. When no `--url` is provided, flutter-cli spawns `flutter run --machine` as a background process
2. It parses the machine protocol JSON output to discover the VM Service WebSocket URI
3. Commands communicate with the app via Dart VM Service Protocol JSON-RPC over WebSocket
4. Process state (PID, URI) is persisted in `/tmp/claude/flutter-cli/` so subsequent commands reuse the same process
5. If the process dies or becomes unreachable, it's automatically restarted on the next command

## License

MIT
