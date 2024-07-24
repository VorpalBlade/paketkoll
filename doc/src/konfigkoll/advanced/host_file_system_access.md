# Host file system access

> This assumes you have read [Managing Files](../files.md) before.
> This chapter builds directly on that.

Like with the previous chapter on [processes](./process.md) this is an advanced
feature that can be dangerous! In particular be careful, you could easily make
your config non-idempotent.

> Idempotency is a fancy way of saying "running the same thing multiple times
> gives the same result". This is important for a configuration management system
> as you want it to be *deterministic*.

With that said: Konfigkoll allows you read-only access to files on the host. Some
example of use cases:

* The main purpose of this is for things that *shouldn't* be stored in your git
  managed configuration, in particular for passwords and other secrets:
  * Hashed passwords from `/etc/shadow` (use the special support for
    [`passwd`](../integrations/passwd.md) instead though, it is a better option)
  * Passwords for wireless networks
  * Passwords for any services needed (such as databases)
* Another use case is to read some system information from `/sys` that isn't
  already exposed by other APIs

Now, the use case of `/etc/shadow` is better served by the built-in
[`passwd`](../integrations/passwd.md) module. But lets look at some of the
other use cases.

## Read from `/sys`

```rune
let is_uefi = filesystem::exists("/sys/firmware/efi")?;
```

This determines if `/sys/firmware/efi` exists, which indicates that this system
is using UEFI.

## Read password for a NetworkManager network

The idea here is that we still want to manage our network configurations, but
we *don't* want to store the password in our git repository. Instead, we can read
that back from the system before applying the config.

```rune
// Get the type of network (wifi or not) and the password for the network
fn parse_sys_network(network_name) {
    // Open the file (with root privileges)
    let fname = `/etc/NetworkManager/system-connections/${network_name}.nmconnection`;
    let f = filesystem::File::open_as_root(fname)?;

    // Read the contents of the file
    let old_contents = f.read_all_string()?;

    // Split it out and parse it
    let lines = old_contents.split("\n").collect::<Vec>();
    // Iterate over the lines to find the psk one
    let psk = lines.iter()
        .find(|line| line.starts_with("psk="))
        .map(|v| v.split("=")
        .collect::<Vec>()[1]);
    // Do the same, but for the network type
    let net_type = lines.iter()
        .find(|line| line.starts_with("type="))
        .map(|v| v.split("=")
        .collect::<Vec>()[1]);
    Ok((net_type, psk))
}
```

We can then use this to patch our network configs before we apply them:

```rune
pub fn nm_add_network(cmds, package_managers, hw_type, network_name) {
    // Get PSK from system
    if let (net_type, psk) = parse_sys_network(network_name)? {
        if net_type == Some("wifi") {
            let fname = `/etc/NetworkManager/system-connections/${network_name}.nmconnection`;
            let edit_actions = [
                (Selector::Regex("^psk=PLACEHOLDER"),
                Action::Replace(format!("psk={}", psk.unwrap()))),
            ];

            // Laptops should auto-connect to Wifi, desktops shouldn't,
            // they use ethernet normally
            if hw_type == SystemType::Laptop {
                edit_actions.push((Selector::Regex("^autoconnect=false"),
                                  Action::Delete));
            }

            // This is a wrapper for LineEditor, see the cook-book chapter
            patch_file_from_config(cmds, package_managers, fname, edit_actions)?;
            // The file should be root only
            cmds.chmod(fname, 0o600)?;
        }
    } else {
        return Err("Network not found")?;
    }
    Ok(())
}
```

This could then be used like this:

```rune
nm_add_network(cmds, package_managers, hw_type, "My Phone Hotspot")?;
nm_add_network(cmds, package_managers, hw_type, "My Home Wifi")?;
nm_add_network(cmds, package_managers, hw_type, "Some other wifi")?;
```
