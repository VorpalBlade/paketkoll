# Managing /etc/passwd, /etc/group and shadow files

Konfigkoll has special support for managing `/etc/passwd`, `/etc/group` and
`/etc/shadow`. This is because these files contain contents from multiple
sources (various packages add their own users) and it is difficult to manage
these otherwise.

The interface to this is the `::passwd::Passwd` type
([API docs](https://vorpalblade.github.io/paketkoll/api/passwd.module.html)).

Typically, you would:

* Create an instance of `::passwd::Passwd` early in the main phase
* Add things to it as needed (next to the associated packages)
* Apply it at the end of the main phase

A rough example (we will break it into chunks down below):

```rune
// Mappings for the IDs that systemd auto-assigns inconsistently from computer to computer
const USER_MAPPING = [("systemd-journald", 900), /* ... */]
const GROUP_MAPPING = [("systemd-journald", 900), /* ... */]

pub async fn phase_main(props, cmds, package_managers) {
    let passwd = passwd::Passwd::new(USER_MAPPING, GROUP_MAPPING);

    let files = package_managers.files();
    // These two files MUST come first as other files later on refer to them,
    // and we are not order independent (unlike the real sysusers.d).
    passwd.add_from_sysusers(files, "systemd", "/usr/lib/sysusers.d/basic.conf")?;
    passwd.add_from_sysusers(files, "filesystem", "/usr/lib/sysusers.d/arch.conf")?;

    // Various other packages and other changes ...
    passwd.add_from_sysusers(files, "dbus", "/usr/lib/sysusers.d/dbus.conf")?;
    // ...

    // Give root a login shell, we don't want the default /usr/bin/nologin!
    passwd.update_user("root", |user| {
        user.shell = "/bin/zsh";
        user
    });

    // Add human user
    let me = passwd::User::new(1000, "me", "me", "");
    me.shell = "/bin/zsh";
    me.home = "/home/me";
    passwd.add_user_with_group(me);
    passwd.add_user_to_groups("me", ["wheel", "optical", "uucp", "users"]);


    // Don't store passwords in your git repo, load them from the system instead
    passwd.passwd_from_system(["me", "root"]);

    // Deal with the IDs not matching (because the mappings were created
    // before konfigkoll was in use for example)
    passwd.align_ids_with_system()?;

    // Apply changes
    passwd.apply(cmds)?;
}
```

## `USER_MAPPING` and `GROUP_MAPPING`

First up, there is special support for systemd's `/usr/lib/sysusers.d/` files.
These often don't declare the specific user/group IDs, but instead auto-assign them.

This creates a bit of chaos between computers and there is no auto-assign logic
in Konfigkoll (yet?). To solve both of these issues we need to declare which
IDs we want for the auto-assigned IDs if we are to use `sysusers.d`-integration.

That is what the `USER_MAPPING` and `GROUP_MAPPING` constants are for.

## General workflow

The idea is (as stated above) to create *one* instance of `Passwd`, update it
as you go along, and then write out the result at the end:

```rune
let passwd = passwd::Passwd::new(USER_MAPPING, GROUP_MAPPING);

// Do stuff

passwd.apply(cmds)?;
```

Now, what about the "stuff" you can "do"?

### Adding a system user / group

The easiest option (when available) is `passwd.add_from_sysusers`. Arch Linux
uses this for (almost?) all users created by packages. Debian however doesn't.

If there *isn't* a corresponding sysusers file to add you need to create the user
yourself. This will be pretty much like the example of adding a human user below.

### Patching a user or group

Sometimes you need to make changes to a user or group created by sysusers. This
can be done by passing a function to `passwd.update_user` or `passwd.update_group`.

```rune
// Give root a login shell, we don't want the default /usr/bin/nologin!
passwd.update_user("root", |user| {
    user.shell = "/bin/zsh";
    user
});
```

The `|...| { code }` syntax is a *closure*, a way to declare an inline function
that you can pass to another function. The bits between the `|` are the parameters
that the function takes.

### Adding a human user

There isn't *too* much code needed for this (and remember, you could always create a utility
function if you need this a lot):

```rune
// Add human user
let me = passwd::User::new(1000, "me", "me", "");
me.shell = "/bin/zsh";
me.home = "/home/me";

// Add them to the passwd database (and automatically create a corresponding group)
passwd.add_user_with_group(me);

// Add the user to some extra groups as well
passwd.add_user_to_groups("me", ["wheel", "optical", "uucp", "users"]);
```

### Passwords

What about setting the password? Well, it isn't good practise to store those passwords
in your git repository. Instead, you can read them from the system:

```rune
passwd.passwd_from_system(["me", "root"]);
```

This will make `me` and `root` have whatever password hashes they current already
have on the system.

### IDs not matching

If you already have several computers before starting with konfigkoll, chances
are the user and group IDs don't match up. This can be fixed with `passwd.align_ids_with_system`.
This will copy the IDs *from* the system so they match up.

Of course the assignment of IDs on now your computers won't match, but the users
and groups will match whatever IDs are on the local file system.
