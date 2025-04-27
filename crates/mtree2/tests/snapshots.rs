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

test_snapshot!(test_gedit, "examples/gedit.mtree");
test_snapshot!(test_xterm, "examples/xterm.mtree");
