//! Generate a stream of commands that would create the current system state

use camino::Utf8Path;
use compact_str::CompactString;
use compact_str::format_compact;
use eyre::WrapErr;
use itertools::Itertools;
use konfigkoll_types::FileContents;
use konfigkoll_types::FsInstruction;
use konfigkoll_types::PkgIdent;
use konfigkoll_types::PkgInstruction;
use paketkoll_types::intern::Interner;

/// Save file system changes
///
/// Takes a fn that is responsible for writing out the file data to a location
/// in the config directory. It should put the file in the standard location
/// (`files/input_file_path`, e.g `files/etc/fstab`)
///
/// Precondition: The instructions are sorted by default sort order (path, op)
pub fn save_fs_changes<'instruction>(
    prefix: &str,
    output: &mut dyn std::io::Write,
    mut file_data_saver: impl FnMut(&Utf8Path, &FileContents) -> eyre::Result<()>,
    instructions: impl Iterator<Item = &'instruction FsInstruction>,
    interner: &Interner,
) -> eyre::Result<()> {
    for instruction in instructions {
        let comment = match (instruction.pkg, &instruction.comment) {
            (None, None) => CompactString::default(),
            (None, Some(comment)) => {
                format_compact!(" // {comment}")
            }
            (Some(pkg), None) => {
                format_compact!(" // [{}]", pkg.as_str(interner))
            }
            (Some(pkg), Some(comment)) => {
                format_compact!(" // [{}] {comment}", pkg.as_str(interner))
            }
        };
        let prefix = format!("    {prefix}cmds");
        match instruction.op {
            konfigkoll_types::FsOp::Remove => {
                writeln!(output, "{prefix}.rm(\"{}\")?;{}", instruction.path, comment)?;
            }
            konfigkoll_types::FsOp::CreateFile(ref contents) => {
                file_data_saver(&instruction.path, contents).wrap_err_with(|| {
                    format!("Failed to save {} to config directory", instruction.path)
                })?;
                writeln!(
                    output,
                    "{prefix}.copy(\"{}\")?;{}",
                    instruction.path, comment
                )?;
            }
            konfigkoll_types::FsOp::CreateSymlink { ref target } => {
                writeln!(
                    output,
                    "{prefix}.ln(\"{}\", \"{}\")?;{}",
                    instruction.path, target, comment
                )?;
            }
            konfigkoll_types::FsOp::CreateDirectory => {
                writeln!(
                    output,
                    "{prefix}.mkdir(\"{}\")?;{}",
                    instruction.path, comment
                )?;
            }
            konfigkoll_types::FsOp::CreateFifo => {
                writeln!(
                    output,
                    "{prefix}.mkfifo(\"{}\")?;{}",
                    instruction.path, comment
                )?;
            }
            konfigkoll_types::FsOp::CreateBlockDevice { major, minor } => {
                writeln!(
                    output,
                    "{prefix}.mknod(\"{}\", \"b\", {}, {})?;{}",
                    instruction.path, major, minor, comment
                )?;
            }
            konfigkoll_types::FsOp::CreateCharDevice { major, minor } => {
                writeln!(
                    output,
                    "{prefix}.mknod(\"{}\", \"c\", {}, {})?;{}",
                    instruction.path, major, minor, comment
                )?;
            }
            konfigkoll_types::FsOp::SetMode { mode } => {
                writeln!(
                    output,
                    "{prefix}.chmod(\"{}\", 0o{:o})?;{}",
                    instruction.path,
                    mode.as_raw(),
                    comment
                )?;
            }
            konfigkoll_types::FsOp::SetOwner { ref owner } => {
                writeln!(
                    output,
                    "{prefix}.chown(\"{}\", \"{}\")?;{}",
                    instruction.path, owner, comment
                )?;
            }
            konfigkoll_types::FsOp::SetGroup { ref group } => {
                writeln!(
                    output,
                    "{prefix}.chgrp(\"{}\", \"{}\")?;{}",
                    instruction.path, group, comment
                )?;
            }
            konfigkoll_types::FsOp::Comment => {
                writeln!(output, "    // {}: {}", instruction.path, comment)?;
            }
            konfigkoll_types::FsOp::Restore => {
                writeln!(
                    output,
                    "    restore({}) // Restore this file to original package manager state{}",
                    instruction.path, comment
                )?;
            }
        }
    }
    Ok(())
}

/// Save package changes
pub fn save_packages<'instructions>(
    prefix: &str,
    output: &mut dyn std::io::Write,
    instructions: impl Iterator<Item = (&'instructions PkgIdent, PkgInstruction)>,
) -> eyre::Result<()> {
    let prefix = format!("    {prefix}cmds");
    let instructions = instructions
        .into_iter()
        .sorted_unstable_by(|(ak, av), (bk, bv)| {
            av.op
                .cmp(&bv.op)
                .then_with(|| ak.package_manager.cmp(&bk.package_manager))
                .then_with(|| ak.identifier.cmp(&bk.identifier))
        });

    for (pkg_ident, pkg_instruction) in instructions {
        let comment = match &pkg_instruction.comment {
            Some(comment) => format_compact!(" // {}", comment),
            None => CompactString::default(),
        };
        match pkg_instruction.op {
            konfigkoll_types::PkgOp::Uninstall => {
                writeln!(
                    output,
                    "{prefix}.remove_pkg(\"{}\", \"{}\")?;{}",
                    pkg_ident.package_manager, pkg_ident.identifier, comment
                )?;
            }
            konfigkoll_types::PkgOp::Install => {
                writeln!(
                    output,
                    "{prefix}.add_pkg(\"{}\", \"{}\")?;{}",
                    pkg_ident.package_manager, pkg_ident.identifier, comment
                )?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ahash::AHashMap;
    use camino::Utf8Path;
    use camino::Utf8PathBuf;
    use konfigkoll_types::FileContents;
    use konfigkoll_types::FsInstruction;
    use konfigkoll_types::FsOp;
    use konfigkoll_types::PkgIdent;
    use konfigkoll_types::PkgInstruction;
    use konfigkoll_types::PkgInstructions;
    use konfigkoll_types::PkgOp;
    use paketkoll_types::backend::Backend;
    use paketkoll_types::intern::PackageRef;
    use pretty_assertions::assert_eq;

    #[test]
    fn test_save_fs_changes() {
        let mut output = Vec::new();
        let mut file_data = AHashMap::new();
        let file_data_saver = |path: &Utf8Path, contents: &FileContents| {
            file_data.insert(path.to_owned(), contents.clone());
            Ok(())
        };

        let interner = Interner::default();

        let instructions = [
            FsInstruction {
                op: FsOp::CreateFile(FileContents::from_literal("hello".as_bytes().into())),
                path: Utf8PathBuf::from("/hello/world"),
                comment: None,
                pkg: None,
            },
            FsInstruction {
                op: FsOp::Remove,
                path: Utf8PathBuf::from("/remove_me"),
                comment: Some("For reasons!".into()),
                pkg: Some(PackageRef::get_or_intern(&interner, "package_name")),
            },
        ];

        save_fs_changes(
            "A",
            &mut output,
            file_data_saver,
            instructions.iter(),
            &interner,
        )
        .unwrap();

        let expected = "    Acmds.copy(\"/hello/world\")?;\n    Acmds.rm(\"/remove_me\")?; // \
                        [package_name] For reasons!\n";
        assert_eq!(String::from_utf8(output).unwrap(), expected);
        assert_eq!(
            file_data.get(Utf8Path::new("/hello/world")).unwrap(),
            &FileContents::from_literal("hello".as_bytes().into())
        );
    }

    #[test]
    fn test_save_packages() {
        let mut output = Vec::new();
        let mut instructions = PkgInstructions::default();
        instructions.insert(
            PkgIdent {
                package_manager: Backend::Pacman,
                identifier: "bash".into(),
            },
            PkgInstruction {
                op: PkgOp::Install,
                comment: None,
            },
        );
        instructions.insert(
            PkgIdent {
                package_manager: Backend::Apt,
                identifier: "zsh".into(),
            },
            PkgInstruction {
                op: PkgOp::Uninstall,
                comment: Some("A comment".into()),
            },
        );

        save_packages(
            "B",
            &mut output,
            instructions.iter().map(|(a, b)| (a, b.clone())).sorted(),
        )
        .unwrap();

        let expected = "    Bcmds.remove_pkg(\"apt\", \"zsh\")?; // A comment\n    \
                        Bcmds.add_pkg(\"pacman\", \"bash\")?;\n";
        assert_eq!(String::from_utf8(output).unwrap(), expected);
    }
}
