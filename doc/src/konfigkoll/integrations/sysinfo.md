# Getting system information

Getting information about the system (host name, distro, architecture, hardware,
etc.) is important in order to make a robust config for multiple computers.

For example, rather than listing exactly which computers should have `intel-ucode`
installed for the microcode firmware, you can look at the CPU vendor and determine
if it should have Intel or AMD microcode.

Konfigkoll exposes this via the `sysinfo` module
([API docs](https://vorpalblade.github.io/paketkoll/api/sysinfo.module.html)).

Currently, this is a bit of work in progress and the API is likely to be expanded,
in particular around detecting PCI devices (GPUs etc.).
