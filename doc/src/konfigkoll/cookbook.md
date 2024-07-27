# Cookbook: Examples & snippets

This contains a bunch of useful patterns and functions you can use in your own
configuration.

## Using strong types

While `props` is a generic key value store for passing info between the phases,
it is easy to make a typo (was it `enable_disk_ecryption` or `use_disk_encryption`, etc.?)

A useful pattern is to define one or a few struct that contains all your properties
and store that, then extract it at the start of each phase that needs it.

```rune
pub struct System {
    cpu_arch,
    cpu_feature_level,
    cpu_vendor,

    has_wifi,

    host_name,
    os,

    // ...
}

pub struct Tasks {
    cad_and_3dprinting,
    development,
    development_rust,
    games,
    office,
    photo_editing,
    video_editing,
    // ...
}

pub async fn phase_system_discovery(props, settings) {
    /// ...

    // This has system discovery info
    props.set("system", system);
    // This defines what tasks the system will fulfill
    // (like "video editing" and "gaming")
    props.set("tasks", tasks);
    Ok(())
}

pub async fn phase_main(props, cmds, package_managers) {
    // Extract the properties
    let system = props.get("system")?;
    let tasks = props.get("tasks")?;

    // ...

    if tasks.gaming {
        // Install steam
        package_managers.apt.install("steam")?;
    }

    // ...

    Ok(())
}
```

Now, when you access e.g. `tasks.gaming` you will get a loud error from Rune if you
typo it, unlike if you use the properties directly.

## Creating a context object

This is a continuation of the previous pattern, and most useful in the main phase:

You might end up with helper functions that need a large number of objects passed to them:

```rune
fn configure_grub(
    props,
    cmds,
    package_managers,
    system,
    tasks,
    passwd)
{
    // ...
}
```

What if you need yet another one? No, the solution here is to pass a single context object
around:

```rune
/// This is to have fewer parameters to pass around
pub struct Context {
    // properties::Properties
    props,
    // commands::Commands
    cmds,
    // package_managers::PackageManagers
    package_managers,

    // System
    system,
    // Tasks
    tasks,

    // passwd::Passwd
    passwd,
}

pub async fn phase_main(props, cmds, package_managers) {
    let system = props.get("system")?;
    let tasks = props.get("tasks")?;
    let passwd = passwd::Passwd::new(tables::USER_MAPPING, tables::GROUP_MAPPING)?;

    let ctx = Context {
        props,
        cmds,
        package_managers,
        system,
        tasks,
        passwd,
    };

    configure_grub(ctx)?;
    configure_network(ctx)?;
    configure_systemd(ctx)?;
    configure_gaming(ctx)?;
    // ...
    Ok(())
}
```

## Patching files ergonomically with LineEditor

Using `LineEditor` directly can get verbose. Consider this (using the context
object idea from above):

```rune
/// Patch a file (from the config directory)
///
/// * cmds (Commands)
/// * package_anager (PackageManager)
/// * package (string)
/// * file (string)
/// * patches (Vec<(Selector, Action)>)
pub fn patch_file_from_config(ctx, file, patches) {
    let package_manager = ctx.package_managers.files();
    let fd = filesystem::File::open_from_config("files/" + file)?;
    let orig = fd.read_all_string()?;
    let editor = LineEditor::new();
    for patch in patches {
        editor.add(patch.0, patch.1);
    }
    let contents = editor.apply(orig);
    ctx.cmds.write(file, contents.as_bytes())?;
    Ok(())
}


/// Patch a file (from a package) to a new destination
///
/// * cmds (Commands)
/// * package_anager (PackageManager)
/// * package (string)
/// * file (string)
/// * target_file (string)
/// * patches (Vec<(Selector, Action)>)
pub fn patch_file_to(ctx, package, file, target_file, patches) {
    let package_manager = ctx.package_managers.files();
    let orig = String::from_utf8(package_manager.original_file_contents(package, file)?)?;
    let editor = LineEditor::new();
    for patch in patches {
        editor.add(patch.0, patch.1);
    }
    let contents = editor.apply(orig);
    ctx.cmds.write(target_file, contents.as_bytes())?;
    Ok(())
}
```

Then you can use this as follows:

```rune
    crate::utils::patch_file(ctx, "bluez", "/etc/bluetooth/main.conf",
        [(Selector::Regex("#AutoEnable"), Action::RegexReplace("^#", "")),
         (Selector::Regex("#AutoEnable"), Action::RegexReplace("false", "true"))])?;
```

Much more compact! In general, consider creating utility functions to simplify
common patterns in your configuration. Though there needs to be a balance, so
you still understand your configuration a few months later. Don't go overboard
with the abstractions.

## Patching using patch

This builds on the example in [Processes (advanced)](./advanced/process.md):

```rune
pub async fn apply_system_patches(ctx) {
    let patches = [];
    patches.push(do_patch(ctx, "patches/etckeeper-post-install.patch"));
    patches.push(do_patch(ctx, "patches/etckeeper-pre-install.patch"));
    patches.push(do_patch(ctx, "patches/zsh-modutils.patch"));

    let results = std::future::join(patches).await;
    for result in results {
        result?;
    }
    Ok(())
}

async fn do_patch(ctx, patch_path) {
    // Load patch file
    let patch_file = filesystem::File::open_from_config(patch_path)?;
    let patch = patch_file.read_all_bytes()?;
    let patch_as_str = String::from_utf8(patch)?;

    // The first two lines says which package and file they apply to, extract them
    let lines = patch_as_str.split('\n').collect::<Vec>();
    let pkg = lines[0];
    let file = lines[1];

    // Create a temporary directory
    let tmpdir = filesystem::TempDir::new()?;
    let tmpdir_path = tmpdir.path();

    // Read the original file
    let orig = ctx.package_managers.files().original_file_contents(pkg, file)?;
    let orig_path = tmpdir.write("orig", orig)?;
    let absolute_patch_path = filesystem::config_path() + "/" + patch_path;

    // Shell out to patch command in a temporary directory
    let command = process::Command::new("patch");
    command.arg(orig_path);
    command.arg(absolute_patch_path);
    let child = command.spawn()?;
    child.wait().await?;

    // Load contents back
    let patched = tmpdir.read("orig")?;

    ctx.cmds.write(file, patched)?;

    Ok(())
}
```

Here the idea is to parse the patch file, which should contain some metadata
at the top for where it should be applied to. Patch will ignore text at the very
top of a diff file and only handle the file from the first `---`. For example:

```patch
etckeeper
/usr/share/libalpm/hooks/05-etckeeper-pre-install.hook

--- /proc/self/fd/12 2022-12-19 17:36:30.026865507 +0100
+++ /usr/share/libalpm/hooks/05-etckeeper-pre-install.hook 2022-12-19 12:43:40.751631786 +0100
@@ -4,8 +4,8 @@
 Operation = Install
 Operation = Upgrade
 Operation = Remove
-Type = Path
-Target = etc/*
+Type = Package
+Target = *

 [Action]
 Description = etckeeper: pre-transaction commit
```
