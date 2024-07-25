use std::env;
use std::fs::File;

use mtree2::MTree;

#[test]
fn run() {
    let path = env::current_dir().unwrap().join("examples/gedit.mtree");
    let mtree = MTree::from_reader(File::open(path).unwrap());
    for entry in mtree {
        entry.unwrap();
    }
}
