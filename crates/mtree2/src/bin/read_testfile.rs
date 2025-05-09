use mtree2::MTree;
use std::fs;
fn main() {
    //   let content = fs::read_to_string("test_freebsd9.mtree").unwrap();
    let content = fs::read_to_string("relative_paths_wrapped_exceeding_root.mtree").expect("");
    // let content = fs::read_to_string("relative_paths.mtree").unwrap();
    //    let content = fs::read_to_string("test_mtree.mtree").unwrap();
    let entries = MTree::from_reader_with_cwd(content.as_bytes(), None);
    for entry in entries {
        // Normally you'd want to handle any errors
        let entry = entry.expect("");
        // We can print out a human-readable copy of the entry
        println!("{entry}");
    }
}
