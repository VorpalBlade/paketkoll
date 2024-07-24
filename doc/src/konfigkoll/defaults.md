# Defaults

This section documents some defaults for settings in Konfigkoll.

## Default ignores

Some paths are always ignored in the file system scan:

* `**/lost+found`
* `/dev/`
* `/home/`
* `/media/`
* `/mnt/`
* `/proc/`
* `/root/`
* `/run/`
* `/sys/`
* `/tmp/`
* `/var/tmp/`

## Default early configurations

Some configurations are always applied early (before packages are installed)
in the configuration process (you can add additional with `settings.early_config`
during the system discovery phase):

* `/etc/passwd`
* `/etc/group`
* `/etc/shadow`
* `/etc/gshadow`

The reason these are applied early is to ensure consistent ID assignment when
installing packages that want to add their own IDs.

## Default sensitive configurations

Konfigkoll will not write out the following files when you use `save`, no matter
what. This is done as a security measure to prevent accidental leaks of sensitive
information:

* `/etc/shadow`
* `/etc/gshadow`

You can add additional files to this list with `settings.sensitive_file` during
the system discovery phase.
