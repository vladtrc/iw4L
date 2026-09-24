# Build dependencies

Rust stable via rustup (`rust-toolchain.toml` pins it), plus a C toolchain
and the Bevy system libs. IW4L enables both `x11` and `wayland`
(`Cargo.toml`), so the Wayland headers stay required. Gamepad input and
rumble (`bevy_gilrs`) link libudev, so the udev headers (`systemd-devel` /
`libudev-dev`) are required too.

```bash
# Fedora
sudo dnf install gcc-c++ pkgconf-pkg-config libX11-devel alsa-lib-devel systemd-devel wayland-devel libxkbcommon-devel

# Debian / Ubuntu
sudo apt install g++ pkg-config libx11-dev libasound2-dev libudev-dev libwayland-dev libxkbcommon-dev

# Arch / Manjaro
sudo pacman -S gcc pkgconf libx11 alsa-lib systemd wayland libxkbcommon libxcursor libxrandr libxi
```

Upstream list (other distros, GPU Vulkan drivers): Bevy
[`linux_dependencies.md`](https://github.com/bevyengine/bevy/blob/main/docs/linux_dependencies.md).

What is **not** needed for a native Linux build: `clang` / `llvm` / `lld` /
`glibc-static` — plain `gcc-c++` links it. `llvm` (the `llvm-lib` tool) is
only required for the Windows cross build:

```bash
sudo dnf install llvm        # Fedora: provides llvm-lib
make setup-windows           # rustup target + cargo-xwin, once
```

Then follow `README.md` (Build and run): copy `.env.example`, set
`IW4L_GAMES`, `make map mp_boneyard`. Portable Windows is
[`WINDOWS.md`](WINDOWS.md).
