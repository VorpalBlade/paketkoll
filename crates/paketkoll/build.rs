use clap::CommandFactory;
use clap::ValueEnum;
use clap_complete::{generate_to, Shell};
use std::env;
use std::io::Error;

include!("src/cli.rs");

fn main() -> Result<(), Error> {
    let outdir = match env::var_os("OUT_DIR") {
        None => return Ok(()),
        Some(outdir) => outdir,
    };

    let mut cmd = Cli::command();
    for &shell in Shell::value_variants() {
        generate_to(shell, &mut cmd, "paketkoll", &outdir)?;
    }
    // Outputs will be in a directory like target/release/build/paketkoll-<some-hash>/out/
    // That is unfortunate, but there doesn't seem to be a way to get a stable output directory
    // println!("cargo:warning=shell completion files generated in: {outdir:?}");

    Ok(())
}
