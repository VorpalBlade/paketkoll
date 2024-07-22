//! Generate a stream of commands that would create the current system state

use anyhow::Context;
use camino::Utf8Path;
use compact_str::{format_compact, CompactString};
use itertools::Itertools;
use konfigkoll_types::{FileContents, FsInstruction, PkgIdent, PkgInstruction};

/// Save file system changes
///
/// Takes a fn that is repsonsible for writing out the file data to a location in the config directory.
/// It should put the file in the standard location (`files/input_file_path`, e.g `files/etc/fstab`)
///
/// Precondition: The instructions are sorted by default sort order (path, op)
pub fn save_fs_changes<'instruction>(
    output: &mut dyn std::io::Write,
    mut file_data_saver: impl FnMut(&Utf8Path, &FileContents) -> anyhow::Result<()>,
    instructions: impl Iterator<Item = &'instruction FsInstruction>,
) -> anyhow::Result<()> {
    for instruction in instructions {
        let comment = match instruction.comment {
            Some(ref comment) => format_compact!(" // {}", comment),
            None => CompactString::default(),
        };
        match instruction.op {
            konfigkoll_types::FsOp::Remove => {
                writeln!(output, "    cmds.rm(\"{}\")?;{}", instruction.path, comment)?;
            }
            konfigkoll_types::FsOp::CreateFile(ref contents) => {
                file_data_saver(&instruction.path, contents).with_context(|| {
                    format!("Failed to save {} to config directory", instruction.path)
                })?;
                writeln!(
                    output,
                    "    cmds.copy(\"{}\")?;{}",
                    instruction.path, comment
                )?;
            }
            konfigkoll_types::FsOp::CreateSymlink { ref target } => {
                writeln!(
                    output,
                    "    cmds.symlink(\"{}\", \"{}\")?;{}",
                    instruction.path, target, comment
                )?;
            }
            konfigkoll_types::FsOp::CreateDirectory => {
                writeln!(
                    output,
                    "    cmds.mkdir(\"{}\")?;{}",
                    instruction.path, comment
                )?;
            }
            konfigkoll_types::FsOp::CreateFifo => {
                writeln!(
                    output,
                    "    cmds.mkfifo(\"{}\")?;{}",
                    instruction.path, comment
                )?;
            }
            konfigkoll_types::FsOp::CreateBlockDevice { major, minor } => {
                writeln!(
                    output,
                    "    cmds.mknod(\"{}\", \"b\", {}, {})?;{}",
                    instruction.path, major, minor, comment
                )?;
            }
            konfigkoll_types::FsOp::CreateCharDevice { major, minor } => {
                writeln!(
                    output,
                    "    cmds.mknod(\"{}\", \"c\", {}, {})?;{}",
                    instruction.path, major, minor, comment
                )?;
            }
            konfigkoll_types::FsOp::SetMode { mode } => {
                writeln!(
                    output,
                    "    cmds.chmod(\"{}\", 0o{:o})?;{}",
                    instruction.path,
                    mode.as_raw(),
                    comment
                )?;
            }
            konfigkoll_types::FsOp::SetOwner { ref owner } => {
                writeln!(
                    output,
                    "    cmds.chown(\"{}\", \"{}\")?;{}",
                    instruction.path, owner, comment
                )?;
            }
            konfigkoll_types::FsOp::SetGroup { ref group } => {
                writeln!(
                    output,
                    "    cmds.chgrp(\"{}\", \"{}\")?;{}",
                    instruction.path, group, comment
                )?;
            }
            konfigkoll_types::FsOp::Comment => {
                writeln!(output, "    // {}: {}", instruction.path, comment)?;
            }
        }
    }
    Ok(())
}

/// Save package changes
pub fn save_packages<'instructions>(
    output: &mut dyn std::io::Write,
    instructions: impl Iterator<Item = (&'instructions PkgIdent, PkgInstruction)>,
) -> anyhow::Result<()> {
    let instructions = instructions
        .into_iter()
        .sorted_unstable_by(|(ak, av), (bk, bv)| {
            av.op
                .cmp(&bv.op)
                .then_with(|| ak.package_manager.cmp(&bk.package_manager))
                .then_with(|| ak.identifier.cmp(&bk.identifier))
        });

    for (pkg_ident, pkg_instruction) in instructions.into_iter() {
        let comment = match &pkg_instruction.comment {
            Some(comment) => format_compact!(" // {}", comment),
            None => CompactString::default(),
        };
        match pkg_instruction.op {
            konfigkoll_types::PkgOp::Uninstall => {
                writeln!(
                    output,
                    "    cmds.remove_pkg(\"{}\", \"{}\")?;{}",
                    pkg_ident.package_manager, pkg_ident.identifier, comment
                )?;
            }
            konfigkoll_types::PkgOp::Install => {
                writeln!(
                    output,
                    "    cmds.add_pkg(\"{}\", \"{}\")?;{}",
                    pkg_ident.package_manager, pkg_ident.identifier, comment
                )?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use paketkoll_types::backend::Backend;
    use pretty_assertions::assert_eq;
    use std::collections::HashMap;

    use camino::{Utf8Path, Utf8PathBuf};
    use konfigkoll_types::{
        FileContents, FsInstruction, FsOp, PkgIdent, PkgInstruction, PkgInstructions, PkgOp,
    };

    use super::*;

    #[test]
    fn test_save_fs_changes() {
        let mut output = Vec::new();
        let mut file_data = HashMap::new();
        let file_data_saver = |path: &Utf8Path, contents: &FileContents| {
            file_data.insert(path.to_owned(), contents.clone());
            Ok(())
        };

        let instructions = vec![
            FsInstruction {
                op: FsOp::CreateFile(FileContents::from_literal("hello".as_bytes().into())),
                path: Utf8PathBuf::from("/hello/world"),
                comment: None,
            },
            FsInstruction {
                op: FsOp::Remove,
                path: Utf8PathBuf::from("/remove_me"),
                comment: Some("For reasons!".into()),
            },
        ];

        save_fs_changes(&mut output, file_data_saver, instructions.iter()).unwrap();

        let expected =
            "    cmds.copy(\"/hello/world\")?;\n    cmds.rm(\"/remove_me\")?; // For reasons!\n";
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
            &mut output,
            instructions.iter().map(|(a, b)| (a, b.clone())).sorted(),
        )
        .unwrap();

        let expected = "    cmds.remove_pkg(\"apt\", \"zsh\")?; // A comment\n    cmds.add_pkg(\"pacman\", \"bash\")?;\n";
        assert_eq!(String::from_utf8(output).unwrap(), expected);
    }
}
