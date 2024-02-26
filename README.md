# Paketkoll

[ [lib.rs] ] [ [crates.io] ] [ [AUR] ]

This is a Rust replacement for `debsums` (on Debian/Ubuntu/...) and `paccheck`
(on Arch Linux and derivatives). It is much faster than those thanks to using
all your CPU cores in parallel.

What it does is compare installed files to what the package manager installed and
report any discrepancies.

* On Arch Linux it will report changed mode, owner, group, mtimes, symlink target,
  file content (sha256) or missing files.
* On Debian it will only report if file content differs for regular files. That
  is the only information available on Debian unfortunately (the md5sum).

Additional features:

* There is a flag to include or not include "config files" (those marked as such
  by the package manager, which is not all files in `/etc` as one might think).
* On Arch Linux you can pass `--trust-mtime` to not check the contents of files
  where the mtime matches. This makes the check ultra-fast.
* Doesn't depend on any distro specific libraries for interacting with the package
  database. We do our own parsing. This makes it possible to be way faster
  (parallelism!) and also to make a cross platform binary that will run on either
  distro without any dependencies apart from libc.

Caveats:

* This is not a drop-in replacement for either debsums nor paccheck, since
  command line flags and output format differs. Additionally, debsums have some
  extra features that this doesn't, such as filtering out files removed by localepurge.
* This uses much more memory than `paccheck` (3x). This is largely unavoidable due
  to memory-speed tradeoffs, though there is room for *some* improvements still.
* paketkoll will not report quite the same errors as `paccheck`. For example, if
  it finds that the size differs, it will not bother computing the checksums,
  since they can never match.

## Benchmarks

Note: CPU time is actually comparable to the original tools (slightly better in
general). But due to parallelism the wall time is *way* better, especially
without `--trust-mtime` (where the runtime is quite small to begin with).

* All of the runs were performed on warm disk cache.
* Distro-installed versions of paccheck and debsums were used.
* Musl builds built using cross was used across the board for best portability.
* The same build flags as used for binary releases in this were used (opt level 2, fat LTO)

### Arch Linux (x64-64 AMD desktop)

* CPU: AMD Ryzen 5 5600X 6-Core Processor (6 cores, 12 threads)
* RAM: 32 GB, 2 DIMMs DDR4, 3600 MHz
* Disk: NVME Gen4 (WD Black SN850 1TB)
* Kernel: 6.7.5-arch1-1
* `pacman -Q | wc -l` indicates 2211 packages installed

When only checking file properties and trusting mtime (these should be the most similar options):

```console
$ hyperfine -i -N --warmup 1 "paketkoll --trust-mtime" "paccheck --file-properties --quiet"
Benchmark 1: paketkoll --trust-mtime
  Time (mean ± σ):     249.4 ms ±   4.8 ms    [User: 1194.5 ms, System: 1216.2 ms]
  Range (min … max):   242.1 ms … 259.7 ms    12 runs
 
Benchmark 2: paccheck --file-properties --quiet
  Time (mean ± σ):      2.561 s ±  0.020 s    [User: 1.504 s, System: 1.053 s]
  Range (min … max):    2.527 s …  2.598 s    10 runs
 
  Warning: Ignoring non-zero exit code.
 
Summary
  paketkoll --trust-mtime ran
   10.27 ± 0.21 times faster than paccheck --file-properties --quiet
```

The speedup isn't quite as impressive when checking the checksums also, but it is still large:

```console
$ hyperfine -i -N --warmup 1 "paketkoll" "paccheck --sha256sum --quiet"
Benchmark 1: paketkoll
  Time (mean ± σ):      9.986 s ±  1.329 s    [User: 17.368 s, System: 19.087 s]
  Range (min … max):    8.196 s … 11.872 s    10 runs
 
Benchmark 2: paccheck --sha256sum --quiet
  Time (mean ± σ):     68.976 s ±  0.339 s    [User: 16.661 s, System: 17.816 s]
  Range (min … max):   68.413 s … 69.604 s    10 runs
 
  Warning: Ignoring non-zero exit code.
 
Summary
  paketkoll ran
    6.91 ± 0.92 times faster than paccheck --sha256sum --quiet
```

* Many and large packages installed
* 6 cores, 12 thread means a decent speed up from multi-threading is possible.
* I don't know what paccheck was doing such that it took 68 seconds but didn't use very much CPU. Presumably waiting for IO?

### Debian (ARM64 Raspberry Pi)

* Raspberry Pi 5 (8 GB RAM)
* CPU: Cortex-A76 (4 cores, 4 threads)
* Disk: USB boot from SATA SSD in USB 3.0 enclosure: Samsung SSD 850 PRO 512GB
* Kernel: 6.1.0-rpi8-rpi-2712
* `dpkg-query -l | grep ii | wc -l` indicates 749 packages installed

```console
$ hyperfine -i -N --warmup 1 "paketkoll" "debsums -c"
Benchmark 1: paketkoll
  Time (mean ± σ):      2.664 s ±  0.102 s    [User: 3.937 s, System: 1.116 s]
  Range (min … max):    2.543 s …  2.813 s    10 runs
 
Benchmark 2: debsums -c
  Time (mean ± σ):      8.893 s ±  0.222 s    [User: 5.453 s, System: 1.350 s]
  Range (min … max):    8.637 s …  9.199 s    10 runs
 
  Warning: Ignoring non-zero exit code.
 
Summary
  'paketkoll' ran
    3.34 ± 0.15 times faster than 'debsums -c'
```

* There aren't a ton of packages installed on this system (it is acting as a headless server). This means that neither command is terribly slow.
* A Pi only has 4 cores also, which limits the maximum possible speedup.

### Ubuntu 22.04 (x86-64 Intel laptop)

* CPU: 12th Gen Intel(R) Core(TM) i9-12950HX (8 P-cores with 16 threads + 8 E-cores with 8 threads)
* RAM: 64 GB, 2 DIMMs DDR4, 3600 MHz
* Disk: NVME Gen4 (WD SN810 2 TB)
* Kernel: 6.5.0-17-generic (HWE kernel)
* `dpkg-query -l | grep ii | wc -l` indicates 4012 packages installed

```console
$ hyperfine -i -N --warmup 1 "paketkoll" "debsums -c"
Benchmark 1: paketkoll
  Time (mean ± σ):      5.341 s ±  0.174 s    [User: 42.553 s, System: 33.049 s]
  Range (min … max):    5.082 s …  5.586 s    10 runs
 
Benchmark 2: debsums -c
  Time (mean ± σ):     92.839 s ±  7.332 s    [User: 47.664 s, System: 15.697 s]
  Range (min … max):   82.872 s … 103.710 s    10 runs
 
  Warning: Ignoring non-zero exit code.
 
Summary
  paketkoll ran
   17.38 ± 1.49 times faster than debsums -c
```

## Future improvements

Most future improvements will happen in the [`paketkoll_core`](crates/paketkoll_core)
crate, to make it suitable for another project idea I have (basically that project
needs this as a library).

I consider the program itself mostly feature complete. The main changes would be
bug fixes and possibly supporting additional Linux distributions and package managers.

## What does the name mean?

paketkoll is Swedish for "package check", though the translation to English isn't perfect ("ha koll på" means "keep an eye on" for example).

[crates.io]: https://crates.io/crates/paketkoll
[lib.rs]: https://lib.rs/crates/paketkoll
[AUR]: https://aur.archlinux.org/packages/paketkoll
