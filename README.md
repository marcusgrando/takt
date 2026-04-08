# Takt

macOS menu bar task scheduler. Automate opening files, URLs, apps, running commands, sending notifications, webhooks, and keyboard shortcuts — all on a schedule.

Built with SwiftUI (macOS 14+) and a Rust core library (`libtakt`) via UniFFI.

## Development

```bash
# Prerequisites: Rust toolchain, Xcode 16+
rustup target add aarch64-apple-darwin

# Run Rust tests
cargo test --package libtakt

# Build & run
open macos/Takt.xcodeproj   # Cmd+R in Xcode
```

## Build

```bash
xcodebuild -project macos/Takt.xcodeproj -scheme Takt -configuration Release build
```

## License

Private — all rights reserved.
