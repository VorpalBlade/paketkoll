use insta::assert_debug_snapshot;
use mtree2::MTree;
use std::fs::File;
use std::path::PathBuf;

macro_rules! test_snapshot {
    ($name:ident, $path:expr) => {
        #[test]
        fn $name() {
            let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join($path);
            let mtree = MTree::from_reader(File::open(path).unwrap());
            let entries: Vec<_> = mtree.collect();
            assert_debug_snapshot!(entries);
        }
    };
}
macro_rules! test_snapshot_with_empty_cwd {
    ($name:ident, $path:expr) => {
        #[test]
        fn $name() {
            let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join($path);
            let mtree = MTree::from_reader_with_empty_cwd(File::open(path).unwrap());
            let entries: Vec<_> = mtree.collect();
            assert_debug_snapshot!(entries);
        }
    };
}
test_snapshot!(test_gedit, "examples/gedit.mtree");
test_snapshot!(test_xterm, "examples/xterm.mtree");
test_snapshot_with_empty_cwd!(test_relative_paths, "examples/relative_paths.mtree");
test_snapshot_with_empty_cwd!(test_wrapped_lines, "examples/relative_paths_wrapped.mtree");
test_snapshot_with_empty_cwd!(
    test_wrapped_lines_exceeding_root,
    "examples/relative_paths_wrapped_exceeding_root.mtree"
);
test_snapshot_with_empty_cwd!(test_freebsd9_flavor, "examples/test_freebsd9.mtree");
test_snapshot_with_empty_cwd!(test_mtree_flavor, "examples/test_mtree.mtree");
test_snapshot_with_empty_cwd!(test_not_unicode, "examples/not_unicode.mtree");
#[cfg(feature = "netbsd6")]
test_snapshot_with_empty_cwd!(test_netbsd6_flavor, "examples/test_netbsd6.mtree");
#[cfg(feature = "netbsd6")]
test_snapshot_with_empty_cwd!(test_not_unicode_netbsd6_flavor, "examples/not_unicode.mtree");
