# Getting started

## Creating a new configuration directory

The first step is to create a new configuration directory. You can get a template
created using:

```bash
konfigkoll -c my_conf_dir init
```

This will create a few skeleton files in `my_conf_dir`. It is useful to look
at what these files are:

* `main.rn`: This is the main entry module to your configuration. You can of
  course (and probably should, to keep things manageable) create additional
  modules and import them here.
* `unsorted.rn`: This file will be overwritten when doing `konfigkoll save`.
  The idea is that you should look at this and move the changes you want to keep
  into your `main.rn` (or supporting files).
* `.gitignore`: This is a starting point for files to ignore when you check your
  config into git. You are going to version control it, right?
* `files/`: `save` will put files that have changed on the system here, and there
  are special commands to copy files from `files` to the system for use in your
  configuration.\
  The path in `files` should *normally* be the same as the path on the system (e.g.
  `files/etc/fstab`), but if you have host specific configs you can use a different
  scheme (e.g. `files/etc/fstab.hostname`).

The only hard requirements from `konfigkoll` is `main.rn` and `unsorted.rn`. `files`
also has special convenient support. The rest is just a suggestion. You can
structure your configuration however you like.

If you are coming from [aconfmgr] this structure should feel somewhat familiar.

## The configuration language

The configuration language in use is [Rune], which is based on Rust when it comes
to syntax. Unlike Rust, it is a dynamically typed language with reference counting,
no need to worry about borrow checking, strict types or any of the other features
that make Rust a bit of a learning curve.

The best documentation on the language itself is [the Rune book](https://rune-rs.github.io/book/),
however for a basic configuration you won't need advanced features.

The main config file is structured in four *phases* that are called in order. This
is done in order to speed up execution and allow file system and package scanning
to start early in the background.

This is the basic structure of `main.rn` (don't worry, we will go through it piece
by piece below):

```rune
/// This phase is for configuring konfigkoll itself and for system discovery.
/// You need to select which backends (pacman, apt, flatpak) to use here
///
/// Parameters:
/// - props: A persistent properties object that the script can use to store
///   data between phases
/// - settings: Settings for konfigkoll (has methods to enable backends etc)
pub async fn phase_system_discovery(props, settings) {
    // Enable backends (if you want to be generic to support multiple distros
    // you would do this based on distro in use and maybe hostname)
    settings.enable_pkg_backend("pacman")?;
    settings.enable_pkg_backend("flatpak")?;
    settings.set_file_backend("pacman")?
    Ok(())
}

/// Here you need to configure which directories to ignore when scanning the
/// file system for changes
pub async fn phase_ignores(props, cmds) {
    // Note! Some ignores are built in to konfigkoll, so you don't need to add them here:
    // These are things like /dev, /proc, /sys, /home etc. See below for the full list.

    cmds.ignore_path("/var/cache")?;
    cmds.ignore_path("/var/lib/flatpak")?;
    cmds.ignore_path("/var/lib/pacman")?;
    // ...
    Ok(())
}

/// This is for installing any packages immediately that are later needed to be
/// *executed* by your main configuration. This should very rarely be needed.
pub async fn phase_script_dependencies(props, cmds) {
    Ok(())
}

/// Main phase, this is where the bulk of your configration should go
///
/// It is recommended to use the "save" sub-command to create an initial
/// `unsorted.rn` file that you can then copy the parts you want from into here.
/// 
/// A tip is to use `konfigkoll -p dry-run save` the first few times to not
/// *actually* save all the files, this helps you figure out what ignores to add
/// above in `phase_ignores()` without copying a ton of files. Once you are happy
/// with the ignores, you can remove the `-p dry-run` part.
pub async fn phase_main(props, cmds, package_managers) {
    Ok(())
}
```

Let's look at it once piece at a time:

### System discovery

If you want to make your configuration generic to support multiple distros you
need to do some conditional logic based on things detected by the system. This
can vary in how refined it is. Let's say you just want to do this based on OD and
hostname, then something like this might be a good starting point

```rune
pub async fn phase_system_discovery(props, settings) {
    let sysinfo = sysinfo::SysInfo::new();
    let os_id = sysinfo.os_id();
    let host_name = sysinfo.host_name()?;

    println!("Configuring for host {} (distro: {})", host_name, os_id);

    // We need to enable the backends that we want to use
    match os_id {
        "arch" => {
            settings.enable_pkg_backend("pacman")?;
            settings.set_file_backend("pacman")?
        }
        "debian" => {
            settings.enable_pkg_backend("apt")?;
            settings.set_file_backend("apt")?
        }
        "ubuntu" => {
            settings.enable_pkg_backend("apt")?;
            settings.set_file_backend("apt")?
        }
        _ => return Err("Unsupported OS")?,
    }

    match host_name {
        "mydesktop" => {
            settings.enable_pkg_backend("flatpak")?;
        }
        "myserver" => {
            // This doesn't have flatpak
        }
    }

    Ok(())
}
```

Some Rune language features of interest here:

* The `match` statement. This is like a `case` or `switch` statement in many
  other languages.
* The use of `?` to propagate errors. This is a common pattern in Rust and Rune,
  and is used instead of exceptions that some other languages uses. Basically it
  means "if this is a `Result::Error`, abort the function and propagate the error to the
  caller".
* The use of `Result` is also why the function has a final `Ok(())` at the end. This
  is because the function needs to return a `Result` type, and `Ok(())` is a way to
  return a successful result with no value.
* Why `()` you might ask? Well, `()` is an empty tuple, and is used in Rust and
  Rune to represent "no value". This is a bit different from many other languages
  where `void` or `None` is used for this purpose.
* You might expect to see `return Ok(());` instead of `Ok(())`, but in Rust and Rune
  the `return` keyword is optional if it is the final expression in the function.
* `println!` is a macro that prints to stdout. It is similar to `printf` in C or
  `console.log` in JavaScript. The `!` is a special syntax for macros in Rust and Rune
  (and the reason it is a macro and not a function isn't really important here).

The other thing you might want to do in this phase is to set properties that you
can then refer back to later. For example, you might want to abstract away checks
like "install video editing software if this is one of these two computers" by
setting a property in this phase and then checking it in the main phase instead
of having checks for which specific hosts to install on everywhere. This makes it
easier should you add yet another computer (fewer places to update in).

To support this `props` can be used:

```rune
pub async fn phase_system_discovery(props, settings) {
    // ...
    props.set("tasks.videoediting", true);
    // ...
    Ok(())
}

pub async fn phase_main(props, settings) {
    // ...
    if props.get("tasks.videoediting") {
        // Install kdenlive and some other things
    }
    // ...
    Ok(())
}
```

Props is a simple key-value store that is persisted between phases. You can use
it however you want. It is basically a `HashMap<String, Value>` where `Value` can
be any type.

Even if you only have a single if statement for a particular property, it can be
*cleaner* to separate out the checking for hardware and host name from the actual
installations. This is especially true as the configuration grows.

### Ignoring files

The next phase is to ignore files that you don't want to track. This is absolutely
required, as there is a bunch of things (especially in `/var`) that aren't managed
by the package manager. In fact `/var` is awkward since there also *are* managed
things under it. As such the ignore section grows long, it can be a good idea
to put this into a separate file and include it. Let's look at how that would be
done:

Your main.rn could look like this:

```rune
mod ignores;

// System discovery goes here still

/// Ignored paths
pub async fn phase_ignores(props, cmds) {
    ignores::ignores(props, cmds)?
    Ok(())
}

// The other later phases
```

In `ignores.rn` you would then have:

```rune
pub fn ignores(props, cmds) {
    cmds.ignore_path("/var/cache")?;
    cmds.ignore_path("/var/lib/flatpak")?;
    cmds.ignore_path("/var/lib/pacman")?;
    // ...
    Ok(())
}
```

The key here is the use of the `mod` keyword to declare another module in the
same directory. This is similar to how you would do it in Rust, and is a way to
split up your configuration into multiple files.

You can also create nested submodules, which is covered in a later section of
the manual.

### Script dependencies

You probably won't need this phase, but it is there if you do. If you need to call
out *from your configuration* to a program that isn't installed by default on a
clean system, you should put it here. For example:

```rune
pub fn phase_script_dependencies(props, cmds) {
    // We use patch in the main phase to apply some diff files to a package
    cmds.add_pkg("pacman", "patch")?;
    Ok(())
}
```

We can see here how to add a package, but this will be covered in more details
in the documentation of the main phase.

### The main phase

This is the bread and butter of your configuration. This is where you will do
most of your work. This is where you will install packages, copy files, patch
configurations, etc.

Let's look at the signature again:

```rune
pub async fn phase_main(props, cmds, package_managers) {

    Ok(())
}
```

This takes three parameters:

* `props` we already know, it is the key value store introduced in
  [the system discovery phase](#system-discovery).
* `cmds` we have seen (for how to add ignores for example) but it hasn't been
  covered in detail, we will get to that now.
* `package_managers` is new, and is your interface to query for what the
  *original* contents of a file is. That is, before you changed it. This can be
  used to apply small changes such as "I want the stock `/etc/nanorc`, but
  uncomment just this one line".

In fact, let's dwell a bit more on that last bullet point. That (apart from
wholesale copying and replacing configuration files) is the main approach to
configuration management in `konfigkoll`.

This means you don't have to merge `.pacnew` or `.dpkg-dist` files anymore, just
reapply your config: it will apply the same change to the new version of the config.
Of course, it is possible the config has changed *drastically*, in which case you
still have to intervene manually, but almost always that isn't the case.

Now lets look at the `cmds` parameter. This is where you describe your configuration.
It builds up a list of *actions* internally that will then be compared to the system
at the end by konfigkoll. That comparison is then used to either apply changes to the
system or save missing actions to `unsorted.rn`.

The brunt of how this works in covered in the next two chapters (to prevent this
section getting far too long):

* [Managing package](packages.md)
* [Managing files](files.md)

There are also some speciality topics that are covered in a later chapter:

* [Systemd (and other integrations)](integrations/README.md)
* There are examples of how to solve specific things in the [cookbook](cookbook.md) chapter.

There are also plans to publish a complete (but sanitised from sensitive info)
example configuration in the future, this is not yet done.

[aconfmgr]: https://github.com/CyberShadow/aconfmgr
[Rune]: https://rune-rs.github.io/
