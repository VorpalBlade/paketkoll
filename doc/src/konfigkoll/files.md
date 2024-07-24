# Managing files

> This assumes you have read [Getting started](./getting_started.md) before.
> This chapter builds directly on that, specifically the
> [section about the main phase](./getting_started.md#the-main-phase) (which in
> turn builds on earlier sections of that chapter).

## Copying files

The most basic operation is to copy a file from the `files` directory in your
configuration to the system. This is what `save` will use when saving changes.

For example

```rune
pub async fn phase_main(props, cmds, package_managers) {
    cmds.copy("/etc/fstab")?;
    cmds.copy("/etc/ssh/sshd_config.d/99-local.conf")?;

    Ok(())
}
```

This config would mean that:

* The file `files/etc/fstab` in your configuration should be copied to `/etc/fstab`
* The file `files/etc/ssh/sshd_config.d/99-local.conf` in your configuration
  should be copied to `/etc/ssh/sshd_config.d/99-local.conf`
* Every other (non-ignored) file on the system should be unchanged compared
  to the package manager.

Like with [packages](./packages.md) the configuration is *total*, that is, it
should describe the system state fully.

Sometimes you might want to rename a file as you copy it. For example to have
host specific configs. `/etc/fstab` is an example of where this can be a good
solution. Then you can use `copy_from` instead of `copy`:

```rune
pub async fn phase_main(props, cmds, package_managers) {
    let sysinfo = sysinfo::SysInfo::new();
    let host_name = sysinfo.host_name();
    cmds.copy_from("/etc/fstab", `/etc/fstab.${host_name}`)?;

    Ok(())
}
```

Here we can also see another feature: In strings surrounded by backquotes you
can use `${}` to interpolate variables. This is a feature of the Rune language.

You can also check if a file exists:

```rune
let candidate = std::fmt::format!("/etc/conf.d/lm_sensors.{}", host_name);
if cmds.has_source_file(candidate) {
    cmds.copy_from("/etc/conf.d/lm_sensors", candidate)?;
}
```

This shows another way to format strings, using `std::fmt::format!`. You can use
either.

## Writing a file directly from the configuration

Sometimes you want to write a file directly from the configuration (maybe it is short,
maybe you have complex logic to generate it). This can be done with `write`:

```rune
pub async fn phase_main(props, cmds, package_managers) {
    ctx.cmds.write("/etc/NetworkManager/conf.d/dns.conf", b"[main]\ndns=dnsmasq\n");
    ctx.cmds.write("/etc/hostname",
                   std::fmt::format!("{}\n", ctx.system.host_name).as_bytes())?;
    ctx.cmds.write("/etc/sddm.conf", b"");
    Ok(())
}
```

Some notes on what we just saw:

* We see here the notion of byte strings (`b"..."`). Unlike normal strings these
  don't have to be Unicode (UTF-8) encoded, though the Rune source file itself
  still does. But you can use escape codes (`b"\001\003"`) to create non-UTF-8 data.
* `write` only take byte strings, if you want to write a UTF-8 string you need to use `.as_bytes()`
  on that string, as can be seen for `/etc/hostname`.
* The file `sddm.conf` will end up empty here.
* `write` replaces the whole file in one go, there isn't an `append`. For patching files, see the
  next section.

## Patching a file compared to the package manager state

Often times you want to use the standard config file but change one or two things about it.
This can be done by extracting the file from the package manager, patching it and then writing it.

Here is a short example appending a line to a config file

```rune
// Specifically the package manager that is responsible for general
// files (as opposed to say flatpak)
let package_manager = package_managers.files();

// Get the contents of /etc/default/grub, then convert it
// to a UTF-8 string (it is a Bytes by default)
let contents = String::from_utf8(
    package_manager.original_file_contents("grub", "/etc/default/grub")?)?;

// Push an extra line to it
contents.push_str("GRUB_FONT=\"/boot/grubfont.pf2\"\n");

// Add a command to write the file
cmds.write(file, contents.as_bytes())?;
```

This is a bit cumbersome, but abstractions can be built on top of this general pattern.
In fact, a few such abstractions are already provided by Konfigkoll.

## Patching a file with LineEditor

If you are at all familar with sed, `::patch::LineEditor` is basically a Rune/Rust variant of that.
The syntax is different though (not a terse one-liner but a bit more verbose).

Lets look at patching the grub config again:

```rune
use patch::LineEditor;
use patch::Action;
use patch::Selector;

pub fn patch_grub(cmds, package_managers) {
    let package_manager = package_managers.files();
    let orig = String::from_utf8(package_manager.original_file_contents(package, file)?)?;

    let editor = LineEditor::new();

    // Replace the GRUB_CMDLINE_LINUX line with a new one
    editor.add(Selector::Regex("GRUB_CMDLINE_LINUX="),
               Action::RegexReplace("=\"(.*)\"$", "=\"loglevel=3 security=apparmor\""));

    // Uncomment the GRUB_DISABLE_OS_PROBER line
    editor.add(Selector::Regex("^#GRUB_DISABLE_OS_PROBER"),
               Action::RegexReplace("^#", ""));

    // Add a line at the end of the file (EOF)
    editor.add(Selector::Eof,
               Action::InsertAfter("GRUB_FONT=\"/boot/grubfont.pf2\""));

    // Apply the commands to the file contents and get the new file contents
    let contents = editor.apply(orig);

    // Write it back
    cmds.write(target_file, contents.as_bytes())?;
}
```

Here we can see the use of `LineEditor` to:

* Replace a line matching a regex (and the replacement itself is a regex matching part of that line)
* Uncomment a line
* Add a line at the end of the file

The above also seems a bit cumbersome, but see
[the cookbook](./cookbook.md#patching-files-ergonomically-with-lineeditor)
for a utility function that encapsulates this pattern.

`LineEditor` has many more features, see the
[API documentation](https://vorpalblade.github.io/paketkoll/api/patch.module.html)
for more details. However, the general idea if that you have a `Selector` that
selects *what* lines a given rule should affect, and an `Action` that describes
*how* those lines should be changed.

Most powerfully a selector or an action can be a function that you write, so
arbitrary complex manipulations are possible. Nested programs are also possible
to operate on multiple consecutive lines:

```rune
// Uncomment two consecutive lines when we encounter [multilib]
// This is equivalent to /\[multilib\]/ { s/^#// ; n ; s/^#// } in sed
let sub_prog = LineEditor::new();
sub_prog.add(Selector::All, Action::RegexReplace("^#", ""));
sub_prog.add(Selector::All, Action::NextLine);
sub_prog.add(Selector::All, Action::RegexReplace("^#", ""));

editor.add(Selector::Regex("\\[multilib\\]"), Action::sub_program(sub_prog));
```

## Patching a file via invoking an external command

Sometimes sed line expressions don't cut it, and you don't want to write the
code in Rune, you just want to reuse an existing command. This can be done with
the `process` module to invoke an external command. This will be
[covered in the advanced section](./advanced/process.md).

## Other file operations (permissions, mkdir, symlinks etc)

Writing files is not all you can do, you can also:

* Change permissions (owner, group, mode)
* Create symlinks
* Create directories

These are all covered in the
[API documentation](https://vorpalblade.github.io/paketkoll/api/command/Commands.struct.html),
but they are relatively simple operations compared to all the variations of
writing file contents, so there will only be a short example:

```rune
// Create a directory and make it root only access
cmds.mkdir("/etc/cni")?;
cmds.chmod("/etc/cni", 0o700)?;
// You could also write either of these and they would mean the same thing:
cmds.chmod("/etc/cni", "u=rwx")?;
cmds.chmod("/etc/cni", "u=rwx,g=,o=")?;

// Create a directory owned by colord:colord
cmds.mkdir("/etc/colord")?;
cmds.chown("/etc/colord", "colord")?;
cmds.chgrp("/etc/colord", "colord")?;

// Create a symlink
cmds.ln("/etc/localtime", "/usr/share/zoneinfo/Europe/Stockholm")?;
```
