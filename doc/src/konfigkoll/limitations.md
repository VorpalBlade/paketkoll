# Limitations

This chapter documents some known limitations of Konfigkoll.

Also consider checking the
[issue tracker on GitHub](https://github.com/VorpalBlade/paketkoll/issues)
for more potential limitations.

## Limitations due to underlying distro

### Debian

#### Metadata

On Debian, `apt`/`dpkg` doesn't provide a lot of information about the files
installed by a package. In fact, it only provides the MD5 sum of regular files
and the list of non-regular files (without info about what *type* of non-regular
file they are). As a workaround we instead pull the data from the cached downloaded
`.deb` files. There are a number of implications of this:

* We need the apt package cache to be populated. We will download missing packages
  as needed to the cache.
* Reading the compressed packages is slow (very slow) so we cache the summary
  data in a disk cache, (typically 50-250 MB depending on the number of
  installed files). That disk cache will be located in `~/.cache/konfigkoll` by
  default. If you run with `sudo` it will be root's home directory that contains
  this.

#### Services

Debian is, unlike Arch Linux, not yet fully systemd-ified. This means that some
of the integrations (systemd services, systemd-sysusers) are less useful. Debian
support is *currently work in progress* and solution for this will be designed
at later point in time.

#### Configuration files

Unlike Arch Linux, Debian has multiple ways to handle configuration files:

* As part of package, installed with apt/dpkg: This will work like on Arch Linux,
  where you can patch files. Crucially if you run `dpkg-query -S /etc/some/file`
  and it returns a package name, it is this case.
* UCF, where post install actions copy/merge the file from somewhere in
  `/usr/share` to `/etc`. You will have to emulate this with a copy from the
  same location in your configuration. Try grepping for the config file of
  interest in `/var/lib/dpkg/info/*.postinst` to find out what is going on.
* Like UCF but free form: Basically the same but with ad-hoc logic instead of
  the `ucf` commands. Same solution (but slightly more annoying to figure out
  as it isn't standardised).
* Like the above case but with no source file: Sometimes the post install script
  just checks if the config file exists on the system, and if not echos some
  embedded text into the config file. There is nothing to copy from here,
  original file queries will not help you. You will simply have to maintain your
  own copy of the file. There is no sane solution for this case, unfortunately.

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
