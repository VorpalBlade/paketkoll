# Development: Design overview

This is aimed at people wanting to work on the Rust code of Konfigkoll & Paketkoll.

Konfigkoll builds upon Paketkoll (in fact paketkoll was first written as a stepping
stone in the konfigkoll development process).

## Paketkoll design

### Backends

Paketkoll (the library `paketkoll_core` that is, the cli is just a thin wrapper
on top) is centred around some core traits:

* `Files`: A backend for querying package manager file information.
* `Packages`: A backend for querying package manager package information.

A backend is anything that can implement one or both of these: Pacman, Apt, Flatpak,
etc.

To add support for a new package manager you would need to implement these traits
for it. Flatpak only implements `Packages` as it doesn't manage files system-wide,
so you may not need to implement both.

Along with these traits are a number of structs and enums that are used by the
trait methods.

Of note is that some strings are interned (that is, they are stored once and
referred to with a single 32-bit integer). This is done to save memory, as things
like package names gets repeated *a lot*. The `PackageRef` and `ArchitectureRef`
types are used for the interned strings.

### Operations

The other part of paketkoll are some algorithms that take data from the above
traits. These live in `file_ops.rs` and `package_ops.rs`. This includes finding
where a file comes from, checking the integrity of files, etc.

Of note is that the integrity checking generate list of `Issue` structs describing
the discrepancies found. These are then printed by the cli or used by konfigkoll.

### Crates

* `paketkoll_core`: The core library that does the heavy lifting (as described above).
* `paketkoll_cache`: Actually only used by konfigkoll, implements a disk cache for
  slow queries to the backends.
* `paketkoll_types`: Defines some core data types that are used by the other crates.
* `paketkoll_utils`: Misc utility functions.
* `paketkoll`: The command line interface.
* `mtree2`: A fork of the `mtree` crate that fixes some outstanding issues. Used by
  the pacman backend.
* `systemd_tmpfiles`: A crate that parses systemd `tmpfiles.d` files. Works fine,
  but turned out not be very useful for comparing system state. Never got integrated
  into konfigkoll. Support in paketkoll is not included by default.

## Konfigkoll design

As stated above, konfigkoll builds on the `paketkoll_core` crate for it's core
system interactions. On top of that it adds the logic to apply changes based on
a script. It is split into multiple crates:

* `konfigkoll_types`: This just defines some core data types that are used by
  the other crates.
* `konfigkoll_core`: This deals with:
  * Take a list of paketkoll `Issue` structs and convert it to a set of primitive
    konfigkoll instructions.
  * Build a stateful model based on streams of instructions.
  * Diff two such states to produce a new stream of instructions describing the
    differences between them. We need the stateful model to handle implicit
    instructions (otherwise the fact that e.g. creating a directory creates it
    as owned by root with certain modes couldn't be handled implicitly)
  * Apply a stream of instructions to the system (possibly asking interactively)
  * Save a stream of instructions to `unsorted.rn`.
* `konfigkoll_hwinfo`: Hardware info (PCI devices currently)
* `konfigkoll_script`: The rune scripting language interface and custom
  extension modules for Rune.
* `konfigkoll_utils`: Misc utility functions to decouple compiling `konfigkoll_script`
  from `konfigkoll_core` (in order to speed up incremental builds).
* `konfigkoll`: The command line interface, and a fair bit of glue and driving
  logic (unlike paketkoll there is a fair bit more here than just command line
  parsing and printing).
