## Installing on macOS

Public Wezmux releases are macOS-first. Windows preview builds are published as
zip artifacts while the installer path is being adapted.

Release automation builds and packages a universal `Wezmux.app` bundle for
Apple Silicon and Intel Macs. Older macOS versions may work, but the supported
path for `v1.0.x` is the current macOS release line tested by the project.

[:simple-apple: Download for macOS :material-tray-arrow-down:]({{ macos_zip_stable }}){ .md-button }
[:simple-apple: Nightly for macOS :material-tray-arrow-down:]({{ macos_zip_nightly }}){ .md-button }

1. Download <a href="{{ macos_zip_stable }}">Release</a>.
2. Extract the zipfile and drag the `Wezmux.app` bundle to your `Applications` folder.
3. First time around, you may need to right click and select `Open` to allow launching
   the application that you've just downloaded from the internet.
3. Subsequently, a simple double-click will launch the UI.
4. The CLI binaries inside the app bundle still use the inherited upstream
   command names such as `wezterm` and `wezterm-gui`. To use them from another
   terminal, add the bundled `MacOS` directory to your `PATH`. For example, if
   `Wezmux.app` is installed in `/Applications`, add this to `~/.zshrc`:
   ```sh
   PATH="$PATH:/Applications/Wezmux.app/Contents/MacOS"
   export PATH
   ```
5. Configuration instructions can be [found here](../config/files.md).
6. Homebrew, MacPorts, Linux, BSD, and Windows installer paths are still being
   adapted from upstream WezTerm and are not part of the public Wezmux `v1.0`
   support contract yet.
