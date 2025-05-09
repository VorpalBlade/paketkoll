use insta::assert_debug_snapshot;
use mtree2::MTree;
use std::fs::File;
use std::path::PathBuf;

macro_rules! test_snapshot {
    ($name:ident, $path:expr) => {
        #[test]
        fn $name() {
            let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join($path);
            let mtree = MTree::from_reader_with_cwd(File::open(path).unwrap(), None);
            let entries: Vec<_> = mtree.collect();
            assert_debug_snapshot!(entries);
        }
    };
}

test_snapshot!(test_gedit, "examples/gedit.mtree");
test_snapshot!(test_xterm, "examples/xterm.mtree");
test_snapshot!(test_relative_paths, "examples/relative_paths.mtree");
test_snapshot!(test_wrapped_lines, "examples/relative_paths_wrapped.mtree");
test_snapshot!(test_freebsd9_flavor, "examples/test_freebsd9.mtree");
test_snapshot!(test_mtree_flavor, "examples/test_mtree.mtree");
#[cfg(feature = "netbsd6")]
test_snapshot!(test_netbsd6_flavor, "examples/test_netbsd6.mtree");
