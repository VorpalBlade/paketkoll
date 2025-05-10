use insta::assert_debug_snapshot;
use mtree2::MTree;
use std::fs::File;
use std::path::PathBuf;

macro_rules! test_snapshot {
    ($name:ident, $path:expr) => {
        #[test]
        fn $name() {
            let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join($path);
            let mtree = MTree::from_reader_with_cwd(File::open(path).unwrap(), PathBuf::from("/"));
            let entries: Vec<_> = mtree.collect();
            assert_debug_snapshot!(entries);
        }
    };
}

test_snapshot!(test_gedit, "examples/gedit.mtree");
test_snapshot!(test_xterm, "tests/data/xterm.mtree");
test_snapshot!(test_relative_paths, "tests/data/relative_paths.mtree");
test_snapshot!(
    invalid_double_filename,
    "tests/data/invalid_double_filename.mtree"
);
test_snapshot!(
    test_wrapped_lines,
    "tests/data/relative_paths_wrapped.mtree"
);
