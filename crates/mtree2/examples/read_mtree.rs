use mtree2::MTree;
use std::env;
use std::error::Error;
use std::fs::File;

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = env::args().collect();
    let path = match args.get(1) {
        Some(p) => p.into(),
        None => env::current_dir()?.join("examples/gedit.mtree"),
    };
    let mtree = MTree::from_reader(File::open(path)?);
    for entry in mtree {
        println!("{}", entry?);
    }
    Ok(())
}
