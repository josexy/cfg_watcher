# cfg_watcher

A Rust library for watching configuration files and reloading them automatically when changes are detected. Built with Tokio+config-rs+notify-rs.

## Features

- Asynchronous configuration file change watching
- Handles edge cases like K8S ConfigMap replacements(refer Go viper)

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
cfg_watcher = "0.1.0"
```

## Usage

See [examples](./examples/watcher/main.rs)

## License

MIT
