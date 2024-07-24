# Managing packages

> This assumes you have read [Getting started](./getting_started.md) before.
> This chapter builds directly on that, specifically the
> [section about the main phase](./getting_started.md#the-main-phase) (which in
> turn builds on earlier sections of that chapter).

## Commands: Installing packages

As noted in the previous chapter, the key type for describing the system configuration
is `Commands`. This includes installing packages. Let's look at a short example:

```rune
pub async fn phase_main(props, cmds, package_managers) {
    cmds.add_pkg("pacman", "linux")?;
    cmds.add_pkg("pacman", "linux-firmware")?;

    Ok(())
}
```

This says that the packages `linux` and `linux-firmware` should be installed
*if* the package manager `pacman` is enabled.

There are two things of note here:

* Konfigkoll ignores instructions to install packages for non-enabled package
  managers. This allows sharing a config across distros more easily.
* The above example actually says that *only* `linux` and `linux-firmware`
  should be installed. Any package that isn't explicitly mentioned (or a
  dependency of an explicitly mentioned package) will be removed. As such, you
  need to list all packages you want to keep.

There is also a `cmds.remove_pkg`. You probably don't want to use it (since
all unmentioned packages are removed), the main purpose of it as a marker in
`unsorted.rn` to tell you that a package is removed on the system compared to
your configuration.

## Optional dependencies

Since Konfigkoll wants you to list all packages you want to keep (except for
their dependencies which are automatically included), what about optional dependencies?

The answer is that you need to list them too, Konfigkoll (like aconfmgr) doesn't
consider optional dependencies for the purpose of keeping packages installed.

> **Note**: This is true for Arch Linux. For Debian the situation is *currently*
> different, but likely to change in the future to match that of Arch Linux.
> Debian support is currently highly experimental.

## Note about early packages

As mentioned in the [previous chapter](./getting_started.md#script-dependencies)
you can use `phase_script_dependencies` to install packages that are needed by
the script itself during the main phase. The syntax (`cmds.add_pkg`) is identical
to the main phase.

## Package manager specific notes

Not all package managers are created equal, and konfigkoll tries to abstract
over them. Sometimes details leak through though. Here are some notes on
those leaks.

### Flatpak

Flatpak doesn't really have the notion of manual installed packages vs dependencies.
Instead, it has the notion of "applications" and "runtimes". That means you cannot
yourself set a package as explicit/implicit installed. Konfigkoll maps "runtimes"
to dependency and "applications" to explicit packages.
