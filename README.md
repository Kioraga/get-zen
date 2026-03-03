# get-zen

A graphical installer for [Zen Browser](https://github.com/zen-browser/desktop) on Linux, built with GTK4 and Rust. It downloads Zen Browser and integrates it using [GearLever](https://github.com/pkgforge-dev/Gear-Lever-AppImage).

## Features

- Simple GTK4 interface
- Downloads the latest Zen Browser AppImage automatically
- Integrates it into the system via GearLever
- Shows download progress and logs in real time
- Also available as a shell script (`get_zen.sh`)

## Requirements

- Linux x86_64
- GTK 4.12+
- Rust + Cargo (to build from source)

## Usage

### AppImage (pre-built)

Download `get-zen-x86_64.AppImage` from the releases, make it executable, and run it:

```bash
chmod +x get-zen-x86_64.AppImage
./get-zen-x86_64.AppImage
```

### Build from source

```bash
cargo build --release
./target/release/get-zen
```

### Build AppImage

```bash
bash build-appimage.sh
```

### Shell script (no GUI)

```bash
bash get_zen.sh
# or skip confirmation:
bash get_zen.sh --yes
```

## License

MIT
