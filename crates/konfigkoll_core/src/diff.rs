//! Diff two sets of instructions
//!
//! This module implements a generic algorithm similar to comm(1)

use std::{
    iter::FusedIterator,
    os::unix::fs::{MetadataExt, PermissionsExt},
};

use camino::{Utf8Path, Utf8PathBuf};
use console::style;
use itertools::{EitherOrBoth, Itertools};
use konfigkoll_types::{FsInstruction, FsOp};
use paketkoll_utils::MODE_MASK;

/// Compare two sorted slices of items
pub fn comm<L, R>(left: L, right: R) -> impl FusedIterator<Item = EitherOrBoth<L::Item, R::Item>>
where
    L: Iterator,
    R: Iterator<Item = L::Item>,
    L::Item: Ord,
    L::Item: PartialEq,
{
    left.merge_join_by(right, Ord::cmp)
}

pub fn show_fs_instr_diff(
    instr: &FsInstruction,
    diff_command: &[String],
    pager_command: &[String],
) -> Result<(), anyhow::Error> {
    match &instr.op {
        FsOp::CreateFile(contents) => {
            show_file_diff(&instr.path, contents, diff_command, pager_command)?;
        }
        FsOp::Remove => {
            println!(
                "{}: Would apply action: {}",
                instr.path,
                style(&instr.op).red()
            );
        }
        FsOp::CreateDirectory
        | FsOp::CreateFifo
        | FsOp::CreateBlockDevice { .. }
        | FsOp::CreateCharDevice { .. } => {
            println!(
                "{}: Would apply action: {}",
                instr.path,
                style(&instr.op).green()
            );
        }
        FsOp::CreateSymlink { target } => {
            // Get old target
            let old_target = match std::fs::read_link(&instr.path) {
                Ok(target) => Utf8PathBuf::from_path_buf(target)
                    .map_err(|p| anyhow::anyhow!("Failed to convert path to UTF-8: {:?}", p))?
                    .to_string(),
                Err(error) => match error.kind() {
                    std::io::ErrorKind::NotFound => "<no prior symlink exists>".to_string(),
                    _ => return Err(error.into()),
                },
            };
            // Show diff
            println!(
                "{}: Would change symlink target: {} -> {}",
                instr.path,
                style(old_target).red(),
                style(target).green()
            );
        }
        FsOp::SetMode { mode } => {
            // Get old
            let old_mode = std::fs::symlink_metadata(&instr.path)
                .map(|m| m.permissions().mode() & MODE_MASK)
                .unwrap_or(0);
            // Show diff
            println!(
                "{}: Would change mode: {} -> {}",
                instr.path,
                style(format!("{:o}", old_mode)).red(),
                style(format!("{:o}", mode.as_raw())).green()
            );
        }
        FsOp::SetOwner { owner } => {
            // Get old UID
            let old_uid = std::fs::symlink_metadata(&instr.path)
                .map(|m| m.uid())
                .unwrap_or(0);
            // Resolve to old user
            let old_user = nix::unistd::User::from_uid(nix::unistd::Uid::from_raw(old_uid))?
                .map(|u| u.name)
                .unwrap_or_else(|| "<user missing in passwd?>".to_string());
            // Resolve new owner to new UID
            let new_uid = nix::unistd::User::from_name(owner.as_str())?
                .map(|u| u.uid.as_raw())
                .map(|uid| format!("{}", uid))
                .unwrap_or_else(|| "<uid missing in passwd?>".to_string());
            // Show diff
            println!(
                "{}: Would change owner: {} ({}) -> {} ({})",
                instr.path,
                style(old_user).red(),
                style(old_uid).red(),
                style(owner).green(),
                style(new_uid).green()
            );
        }
        FsOp::SetGroup { group } => {
            // Get old GID
            let old_gid = std::fs::symlink_metadata(&instr.path)
                .map(|m| m.gid())
                .unwrap_or(0);
            // Resolve to old group
            let old_group = nix::unistd::Group::from_gid(nix::unistd::Gid::from_raw(old_gid))?
                .map(|g| g.name)
                .unwrap_or_else(|| "<group missing in group?>".to_string());
            // Resolve new group to new GID
            let new_gid = nix::unistd::Group::from_name(group.as_str())?
                .map(|g| g.gid.as_raw())
                .map(|gid| format!("{}", gid))
                .unwrap_or_else(|| "<gid missing in group?>".to_string());
            // Show diff
            println!(
                "{}: Would change group: {} ({}) -> {} ({})",
                instr.path,
                style(old_group).red(),
                style(old_gid).red(),
                style(group).green(),
                style(new_gid).green()
            );
        }
        FsOp::Comment => (),
    };
    Ok(())
}

fn show_file_diff(
    sys_path: &Utf8Path,
    contents: &konfigkoll_types::FileContents,
    diff_command: &[String],
    pager_command: &[String],
) -> anyhow::Result<()> {
    let diff = match contents {
        konfigkoll_types::FileContents::Literal { checksum: _, data } => duct::cmd(
            &diff_command[0],
            diff_command[1..]
                .iter()
                .chain(&[sys_path.to_string(), "/dev/stdin".into()]),
        )
        .stdin_bytes(data.clone()),
        konfigkoll_types::FileContents::FromFile { checksum: _, path } => duct::cmd(
            &diff_command[0],
            diff_command[1..]
                .iter()
                .chain(&[sys_path.to_string(), path.to_string()]),
        ),
    }
    .unchecked();
    let pipeline = diff.pipe(duct::cmd(&pager_command[0], pager_command[1..].iter()));
    match pipeline.run() {
        Ok(output) => {
            if !output.status.success() {
                tracing::warn!(
                    "Diff or pager exited with non-zero status: {}",
                    output.status
                );
            }
            Ok(())
        }
        Err(err) => {
            tracing::error!(
                "Diff or pager exited with: {}, kind: {}, OS code {:?}",
                err,
                err.kind(),
                err.raw_os_error()
            );
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_comm() {
        let left = [1, 2, 3, 4, 5, 8];
        let right = [3, 4, 5, 6, 7];

        let mut comm_iter = comm(left.into_iter(), right.into_iter());
        assert_eq!(comm_iter.next(), Some(EitherOrBoth::Left(1)));
        assert_eq!(comm_iter.next(), Some(EitherOrBoth::Left(2)));
        assert_eq!(comm_iter.next(), Some(EitherOrBoth::Both(3, 3)));
        assert_eq!(comm_iter.next(), Some(EitherOrBoth::Both(4, 4)));
        assert_eq!(comm_iter.next(), Some(EitherOrBoth::Both(5, 5)));
        assert_eq!(comm_iter.next(), Some(EitherOrBoth::Right(6)));
        assert_eq!(comm_iter.next(), Some(EitherOrBoth::Right(7)));
        assert_eq!(comm_iter.next(), Some(EitherOrBoth::Left(8)));
        assert_eq!(comm_iter.next(), None);
    }
}
