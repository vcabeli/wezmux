# Privacy Policy for Wezmux

No data about your device(s) or Wezmux usage leave your device.

## Data Maintained by Wezmux

Wezmux maintains some historical data, such as recent searches or action
usage, in some of its overlays such as the debug overlay and character
selector, in order to make your usage more convenient. It is used only
by the local process, and care is taken to limit access for the associated
files on disk to only your local user identity.

Wezmux tracks the output from the commands that you have executed in
a scrollback buffer.  At the time of writing, that scrollback buffer
is an in-memory structure that is not visible to other users of the machine.
In the future, if Wezmux expands to offload scrollback information to
your local disk, it will do so in such a way that other users on the
same system will not be able to inspect it.

## macOS and Data permissions

On macOS, when a GUI application that has a "bundle" launches child processes
(eg: Wezmux, running your shell, and your shell running the programs which you
direct it to run), any permissioned resource access that may be attempted by
those child processes will be reported as though Wezmux is attempting to
access those resources.

The result is that from time to time you may see a dialog about Wezmux
accessing your Contacts if run a `find` command that happens to step through
the portion of your filesystem where the contacts are stored.  Or perhaps you
are running a utility that accesses your camera; it will appear as though
Wezmux is accessing those resources, but it is not: there is no logic within
Wezmux to attempt to access your contacts, camera or any other sensitive
information.

## Update Checking

By default, once every 24 hours, Wezmux makes an HTTP request to GitHub's
release API in order to determine if a newer version is available and to
notify you if that is the case.

The content of that request is private between your machine and GitHub.

If you wish, you can disable update checking by setting
`check_for_updates = false` in your config.

## Upstream

Wezmux is a fork of [WezTerm](https://github.com/wezterm/wezterm). The privacy
properties described above are inherited from WezTerm's source code. Wezmux
binaries are built from source at https://github.com/vcabeli/wezmux/.
