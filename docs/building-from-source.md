## Building from source

Cargo handles the build process. Install Rust with [`rustup.rs`](https://rustup.rs/), then install the system dependencies required by your operating system.

### Linux dependencies

#### Ubuntu

```sh
sudo apt install clang libxkbcommon-x11-dev pkg-config libvulkan-dev libwayland-dev xorg-dev libxcb-shape0-dev libxcb-xfixes0-dev
```

#### Fedora

```sh
sudo dnf install clang libxkbcommon-x11-devel libxcb-devel vulkan-loader-devel wayland-devel openssl-devel pkgconf
```

#### Void Linux

```sh
sudo xbps-install -S base-devel clang libxkbcommon-devel vulkan-loader wayland-devel
```

### Build Lapce

Clone this repository and enter its directory:

```sh
git clone https://github.com/lapce/lapce.git ~/lapce
cd ~/lapce
```

Build and install the application:

```sh
cargo install --path . --bin lapce --profile release-lto --locked
```

On Windows, the same Cargo command builds the application locally. The automated release workflow packages a portable ZIP for the main `lapce` executable.

Once Lapce is compiled, the executable will be available in `$HOME/.cargo/bin/lapce` and should be available in `PATH` automatically.
