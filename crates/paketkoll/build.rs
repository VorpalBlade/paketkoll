use clap::CommandFactory;
use clap::ValueEnum;
use clap_complete::{generate_to, Shell};
use std::env;
use std::io::Error;
use std::path::PathBuf;

include!("src/cli.rs");

fn main() -> Result<(), Error> {
    let outdir = env::var_os("OUT_DIR").ok_or(std::io::ErrorKind::NotFound)?;

    let mut cmd = Cli::command();
    for &shell in Shell::value_variants() {
        generate_to(shell, &mut cmd, "paketkoll", &outdir)?;
    }

    let man = clap_mangen::Man::new(cmd);
    let mut buffer: Vec<u8> = Default::default();
    man.render(&mut buffer)?;
    std::fs::write(PathBuf::from(outdir).join(man.get_filename()), buffer)?;

    // Outputs will be in a directory like target/release/build/paketkoll-<some-hash>/out/
    // That is unfortunate, but there doesn't seem to be a way to get a stable output directory
    // println!("cargo:warning=shell completion & man page generated in: {outdir:?}");

    Ok(())
}
