# Invoking external commands

> This assumes you have read [Managing Files](../files.md) before.
> This chapter builds directly on that.

If using `LineEditor` or custom Rune code doesn't cut it you can invoke external
commands. Be careful with this as you could easily make your config non-idempotent.

> Idempotency is a fancy way of saying "running the same thing multiple times
> gives the same result". This is important for a configuration management system
> as you want it to be *deterministic*.

In particular, you should not use external commands to write directly to the system.
Instead, you should use a temporary directory if you need filesystem operations.

## Example with `patch`

The following example shows how to use `patch` to apply a patch file:

```rune
async fn patch_zsh(cmds, package_managers) {
    // This is relative the config directory
    let patch_file = "patches/zsh-modutils.patch";
    // Package we will patch
    let pkg = "zsh";
    // The file we want to patch
    let file = "/usr/share/zsh/functions/Completion/Linux/_modutils";

    // Create a temporary directory to operate in
    let tmpdir = filesystem::TempDir::new()?;
    let tmpdir_path = tmpdir.path();
    
    // Read the original file from the package manager
    let orig = package_managers.files().original_file_contents(pkg, file)?;
    // Write out the original file to the temporary directory, and store the
    // full path to it for later use
    let orig_path = tmpdir.write("orig", orig)?;

    // We need to know the full path to the patch file, to give it to patch
    let absolute_patch_path = filesystem::config_path() + "/" + patch_path;

    // Create a command that describes how to invoke patch
    let command = process::Command::new("patch");
    command.arg(orig_path);
    command.arg(absolute_patch_path);

    // Start the command
    let child = command.spawn()?;

    // Wait for the command to complete
    child.wait().await?;

    // Load contents back after patch applied it
    let patched = tmpdir.read("orig")?;

    // Add a command to write out the changed file
    cmds.write(file, patched)?;

    Ok(())
}
```

As can be seen this is quite a bit more involved than using `LineEditor`
(but the pattern can be encapsulated, see [the cookbook](../cookbook.md#patching-using-patch)).

There are also some other things to note here:

* What's up with `async` and `await`? This will be covered in the [next section](#async-and-await).
* The use of `TempDir` to create a temporary directory. The temporary directory
  will be automatically removed once the variable goes out of scope.
* External processes are built up using a builder object `process::Command`, and
  are then invoked. You can build pipelines and handle stdin/stdout/stderr as well,
  see the API docs for details on that.

## Async and await

You might have noticed `async fn` a few times before, without it ever being
explained. It is an advanced feature and not one you really need to use much
for Konfigkoll.

However, the basic idea is that Rust and Rune have functions that can run
concurrently. These are not quite like threads, instead they can run on the same
thread (or separate ones) but can be paused and resumed at certain points. For
example when waiting for IO (or an external process to complete), you could be
doing something else.

Konfigkoll uses this internally on the Rust side to do things like scanning the
file system for changes at the same time as processing your configuration.

For talking to external processes this leaks through into the Rune code (otherwise
*you* don't really need to care about it).

Here is what you have to keep in mind:

* When you see an `async fn` in the API docs, you need to call it like so:
  
  ```rune
  let result = some_async_fn().await;
  ```
  
  This means that when `some_async_fn` is called we should wait for it's output.
* You can only use async functions from other async functions. That is, you
  can't call an async function from a non-async function. So your `phase_main`
  must also be async and so does the whole chain in between your phase_main and
  the async API function.
* Async functions don't execute *until they are awaited. That means, they do nothing
  until you `await` them. They won't magically run in the background unless you
  specifically make them do so (see below).

### Awaiting multiple things

If you want to do multiple things in parallel yourself, you don't need to
*immediately* `await` the `async fn`, the key here is that it has to be awaited
*eventually*. Using `std::future::join` you can wait for multiple async functions:

```rune
// Prepare a whole bunch of patch jobs
let patches = [];
patches.push(do_patch(cmds, package_managers, "patches/etckeeper-post-install.patch"));
patches.push(do_patch(cmds, package_managers, "patches/etckeeper-pre-install.patch"));
patches.push(do_patch(cmds, package_managers, "patches/zsh-modutils.patch"));

// Run them and wait for them all
let results = std::future::join(patches).await;
// Process the results to propagate any errors
for result in results {
    result?;
}
```
