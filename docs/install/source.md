## Installing from source

If you want to work from the repository directly, you can build Wezmux from
source. Public `v1.0` support is macOS-first, with Windows preview builds
available as zip artifacts. Other platforms should be treated as best-effort
while the inherited upstream packaging is being adapted.

* Install `rustup` to get the `rust` compiler installed on your system.
  [Install rustup](https://www.rust-lang.org/en-US/install.html).
* Rust version 1.71 or later is required
* Build in release mode: `cargo build --release`
* Run it via either `cargo run --release --bin wezterm` or `target/release/wezterm`

You will need a collection of support libraries; the repo-local [`get-deps`](https://github.com/vcabeli/wezmux/blob/main/get-deps) script will
attempt to install them for you. If it doesn't know about your system,
[please contribute instructions!](https://github.com/vcabeli/wezmux/blob/main/CONTRIBUTING.md).

Use the full git repo:

```console
$ curl https://sh.rustup.rs -sSf | sh -s
$ git clone --depth=1 --branch=main --recursive https://github.com/vcabeli/wezmux.git
$ cd wezmux
$ git submodule update --init --recursive
$ ./get-deps
$ cargo build --release
$ cargo run --release --bin wezterm -- start
```

**If you get an error about zlib then you most likely didn't initialize the submodules;
take a closer look at the instructions!**

### Building without Wayland support on Unix systems

By default, support for both X11 and Wayland is included on Unix systems.
If your distribution has X11 but not Wayland, then you can build Wezmux without
Wayland support by changing the `build` invocation:

```console
$ cargo build --release --no-default-features --features vendored-fonts
```

Building without X11 is not supported.

### Building on Windows

If you experiment with Windows builds, you must select the MSVC version of
Rust. That is the only viable toolchain for building `wezterm` there.

On Windows, instead of using `get-deps`, the only other dependency that you need is
[Strawberry Perl](https://strawberryperl.com). You must ensure that you have
your `PATH` environment set up to find that particular `perl.exe` ahead of any
other perl that you may have installed on your system. This particular version
of perl is required to build openssl on Windows.

```console
$ set PATH=c:\Strawberry\perl\bin;%PATH%
```
