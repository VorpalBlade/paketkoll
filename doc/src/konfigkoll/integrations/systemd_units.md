# Systemd units

Konfigkoll has special support for enabling and masking systemd units. This
simplifies what would otherwise be a bunch of `cmds.ln()` calls. In particular,
it will handle `Alias` and `WantedBy` correctly

## Enabling units from packages

The basic form is:

```rune
systemd::Unit::from_pkg("gpm",
                        "gpm.service",
                        package_managers.files())
    .enable(cmds)?;
```

This will load the unit file from the package manager and figure out what symlinks
needs to be created to enable the unit.

Some units are parameterised, this can be handled by using the `name` method:

```rune
systemd::Unit::from_pkg("systemd",
                        "getty@.service",
                        package_managers.files())
    .name("getty@tty1.service")
    .enable(cmds)?;
```

User units can also be enabled. This enables user units globally (`/etc/systemd/user`),
not per-user:

```rune
systemd::Unit::from_pkg("xdg-user-dirs",
                        "xdg-user-dirs-update.service",
                        package_managers.files())
    .user()
    .enable(cmds)?;
```

You can skip automatically installing `WantedBy` symlinks by using:

```rune
systemd::Unit::from_pkg("avahi",
                        "avahi-daemon.service",
                        package_managers.files())
    .skip_wanted_by()
    .enable(cmds)?;
```

A similar option is also available for `Alias`.

## Enabling custom units

If you have a unit you install yourself that doesn't come from a package you
can do this:

```rune
cmds.copy("/etc/systemd/system/kdump.service")?;
systemd::Unit::from_file("/etc/systemd/system/kdump.service", cmds)?
    .enable(cmds)?;
```

All the other options described in the previous section are also available for
these types of units.

## Caveats

While `WantedBy` and `Alias` are handled correctly, `Also` is not processed,
if you want such units you have to add them manually. The reason is that these
could come from a different package, and we don't know which one.

We could find out for installed packages, but what if it is from a package that
isn't yet installed? This can happen since we build the configuration first, then
install packages.
