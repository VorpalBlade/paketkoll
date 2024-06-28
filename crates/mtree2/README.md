# mtree2

This is a fork of [mtree-rs](https://github.com/derekdreery/mtree-rs) fixing some issues and improving performance. Unfortunately the upstream
has been dead apart from one comment, so a fork was neccesary.

The original README is reproduced below:

---

A library for iterating through entries of an mtree.

> *mtree* is a data format used for describing a sequence of files. Their location is record, along with optional extra values like checksums, size, permissions etc.

 For details on the spec see [mtree(5)].

## Examples

```rust
use mtree2::MTree;
use std::time::SystemTime;

// We're going to load data from a string so this example with pass doctest,
// but there's no reason you can't use a file, or any other data source.
let raw_data = "
/set type=file uid=0 gid=0 mode=644
./.BUILDINFO time=1523250074.300237174 size=8602 md5digest=13c0a46c2fb9f18a1a237d4904b6916e \
     sha256digest=db1941d00645bfaab04dd3898ee8b8484874f4880bf03f717adf43a9f30d9b8c
./.PKGINFO time=1523250074.276237110 size=682 md5digest=fdb9ac9040f2e78f3561f27e5b31c815 \
     sha256digest=5d41b48b74d490b7912bdcef6cf7344322c52024c0a06975b64c3ca0b4c452d1
/set mode=755
./usr time=1523250049.905171912 type=dir
./usr/bin time=1523250065.373213293 type=dir
";
let entries = MTree::from_reader(raw_data.as_bytes());
for entry in entries {
    // Normally you'd want to handle any errors
    let entry = entry.unwrap();
    // We can print out a human-readable copy of the entry
    println!("{}", entry);
    // Let's check that if there is a modification time, it's in the past
    if let Some(time) = entry.time() {
        assert!(time < SystemTime::now());
    }
    // We might also want to take a checksum of the file, and compare it to the digests
    // supplied by mtree, but this example doesn't have access to a filesystem.
}
```

[mtree(5)]: https://www.freebsd.org/cgi/man.cgi?mtree(5)
