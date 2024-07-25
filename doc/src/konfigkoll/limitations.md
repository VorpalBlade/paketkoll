# Limitations

This chapter documents some known limitations of Konfigkoll.

Also consider checking the
[issue tracker on GitHub](https://github.com/VorpalBlade/paketkoll/issues)
for more potential limitations.

## Limitations due to underlying distro

### Debian

On Debian, `apt`/`dpkg` doesn't provide a lot of information about the files installed by a package.
In fact, it only provides the MD5 sum of regular files and the list of non-regular files (without info
about what *type* of non-regular file they are). This means that unlike Arch Linux:

* We won't be able to tell if the mode/owner/group is wrong on a file.
* `--trust-mtime` doesn't work (we have to checksum every file).

I have plans how to work around some of these limitations in the future.

Debian is, unlike Arch Linux, not yet fully systemd-ified. This means that some
of the integrations (like enabling systemd services) are less useful. Debian
support is *currently work in progress* and solution for this will be designed
at later point in time.

## Limitations due to not yet being implemented

* Certain errors can be delayed from when they happen to they are reported.
  This happens because of the async runtime in use (tokio) and how it handles
  (or rather not handles) cancelling synchronous background tasks.
* Some of the exposed API is work in progress:
  * Sysinfo PCI devices is the most notable example.
  * The process API is also not fully fleshed out (no way to provide stdin
    to child processes).
  * The regex API is rather limited, and will have to be fully redesigned using
    a lower level Rust crate at some point.
* There are plans to do privilege separation like aconfmgr does. This is not yet
  implemented.
* There is not yet support for creating FIFOs, device nodes etc. Or rather, there is,
  it just isn't hooked up to the scripting language yet (nor tested).

## Things that won't get implemented (probably)

This is primarily a comparison with aconfmgr, as that is the closest thing to
Konfigkoll that exists.

* Aconfmgr has special support for *some* AUR helpers. Konfigkoll doesn't.
  * For a start, I use [aurutils] which works differently than the helpers
    aconfmgr supports in that it uses a custom repository. The main purpose
    of the aconfmgr integration is to work around the lack of such custom
    repositories.
  * It would be very Arch Linux specific, and it would be hard to abstract
    over this in a way that would be useful for other distros. The reason
    Konfigkoll exists is to let me manage my Debian systems in the same way
    as my Arch Linux systems, so this is not a priority. That Konfigkoll is
    also much faster is a nice bonus.

[aurutils]: https://github.com/aurutils/aurutils
