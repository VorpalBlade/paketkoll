//! State representation of file system

use std::{collections::BTreeMap, sync::Arc};

use anyhow::anyhow;
use camino::{Utf8Path, Utf8PathBuf};
use compact_str::CompactString;
use konfigkoll_types::{FileContents, FsInstruction, FsOp};
use paketkoll_types::{
    backend::Files,
    files::{Mode, PathMap, Properties},
};

use crate::utils::{IdKey, NumericToNameResolveCache};

const DEFAULT_FILE_MODE: Mode = Mode::new(0o644);
const DEFAULT_DIR_MODE: Mode = Mode::new(0o755);
const ROOT: CompactString = CompactString::const_new("root");

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct FsNode {
    entry: FsEntry,
    mode: Option<Mode>,
    owner: Option<CompactString>,
    group: Option<CompactString>,
    /// Keep track of if this node was removed before being added back.
    /// Needed for handling type conflicts correctly.
    removed_before_added: bool,
    /// Optional comment for saving purposes
    comment: Option<CompactString>,
}

// This is a macro due to partial moving of self
macro_rules! fsnode_into_base_instruction {
    ($this:ident, $path:tt) => {
        match $this.entry {
            FsEntry::Removed => Some(FsInstruction {
                path: $path.into(),
                op: FsOp::Remove,
                comment: $this.comment,
            }),
            FsEntry::Unchanged => None,
            FsEntry::Directory => Some(FsInstruction {
                path: $path.into(),
                op: FsOp::CreateDirectory,
                comment: $this.comment,
            }),
            FsEntry::File(contents) => Some(FsInstruction {
                path: $path.into(),
                op: FsOp::CreateFile(contents),
                comment: $this.comment,
            }),
            FsEntry::Symlink { target } => Some(FsInstruction {
                path: $path.into(),
                op: FsOp::CreateSymlink { target },
                comment: $this.comment,
            }),
            FsEntry::Fifo => Some(FsInstruction {
                path: $path.into(),
                op: FsOp::CreateFifo,
                comment: $this.comment,
            }),
            FsEntry::BlockDevice { major, minor } => Some(FsInstruction {
                path: $path.into(),
                op: FsOp::CreateBlockDevice { major, minor },
                comment: $this.comment,
            }),
            FsEntry::CharDevice { major, minor } => Some(FsInstruction {
                path: $path.into(),
                op: FsOp::CreateCharDevice { major, minor },
                comment: $this.comment,
            }),
        }
    };
}

impl FsNode {
    fn into_instruction(self, path: &Utf8Path) -> impl Iterator<Item = FsInstruction> {
        let mut results = vec![];
        let mut do_metadata = true;
        let mut was_symlink = false;
        let default_mode = match &self.entry {
            FsEntry::Removed => None,
            FsEntry::Unchanged => None,
            FsEntry::Directory => Some(DEFAULT_DIR_MODE),
            FsEntry::File(_) => Some(DEFAULT_FILE_MODE),
            FsEntry::Symlink { .. } => None,
            FsEntry::Fifo | FsEntry::BlockDevice { .. } | FsEntry::CharDevice { .. } => {
                Some(DEFAULT_FILE_MODE)
            }
        };

        if self.removed_before_added && self.entry != FsEntry::Removed {
            results.push(FsInstruction {
                path: path.into(),
                op: FsOp::Remove,
                comment: Some("Removed (and later recreated) due to file type conflict".into()),
            });
        }
        match &self.entry {
            FsEntry::Removed => {
                do_metadata = false;
            }
            FsEntry::Symlink { .. } => {
                was_symlink = true;
            }
            _ => (),
        }
        if let Some(instr) = fsnode_into_base_instruction!(self, path) {
            results.push(instr);
        }

        if do_metadata {
            if !was_symlink && self.mode != default_mode {
                if let Some(mode) = self.mode {
                    results.push(FsInstruction {
                        path: path.into(),
                        op: FsOp::SetMode { mode },
                        comment: None,
                    });
                }
            }
            if let Some(owner) = self.owner {
                if owner != ROOT {
                    results.push(FsInstruction {
                        path: path.into(),
                        op: FsOp::SetOwner { owner },
                        comment: None,
                    });
                }
            }
            if let Some(group) = self.group {
                if group != ROOT {
                    results.push(FsInstruction {
                        path: path.into(),
                        op: FsOp::SetGroup { group },
                        comment: None,
                    });
                }
            }
        }

        results.into_iter()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, strum::EnumDiscriminants)]
enum FsEntry {
    /// Negative entry: This has been removed
    Removed,
    /// Unchanged, we only got a mode/owner/group change
    Unchanged,
    /// A directory
    Directory,
    /// A file
    File(FileContents),
    /// A symlink
    Symlink { target: camino::Utf8PathBuf },
    /// Create a FIFO
    Fifo,
    /// Create a block device
    BlockDevice { major: u64, minor: u64 },
    /// Create a character device
    CharDevice { major: u64, minor: u64 },
}

#[derive(Debug, Default, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FsEntries {
    fs: BTreeMap<Utf8PathBuf, FsNode>,
}

impl FsEntries {
    /// Apply a stream of instructions to this `FsEntries`
    pub fn apply_instructions(
        &mut self,
        instructions: impl Iterator<Item = FsInstruction>,
        warn_redundant: bool,
    ) {
        for instr in instructions {
            match instr.op {
                FsOp::Remove => {
                    self.fs.insert(
                        instr.path,
                        FsNode {
                            entry: FsEntry::Removed,
                            mode: Some(DEFAULT_FILE_MODE),
                            owner: Some(ROOT),
                            group: Some(ROOT),
                            removed_before_added: true,
                            comment: instr.comment,
                        },
                    );
                }
                FsOp::CreateDirectory => {
                    self.replace_node(
                        instr.path,
                        FsNode {
                            entry: FsEntry::Directory,
                            mode: Some(DEFAULT_DIR_MODE),
                            owner: Some(ROOT),
                            group: Some(ROOT),
                            removed_before_added: false,
                            comment: instr.comment,
                        },
                    );
                }
                FsOp::CreateFile(contents) => {
                    self.replace_node(
                        instr.path,
                        FsNode {
                            entry: FsEntry::File(contents),
                            mode: Some(DEFAULT_FILE_MODE),
                            owner: Some(ROOT),
                            group: Some(ROOT),
                            removed_before_added: false,
                            comment: instr.comment,
                        },
                    );
                }
                FsOp::CreateSymlink { target } => {
                    self.replace_node(
                        instr.path,
                        FsNode {
                            entry: FsEntry::Symlink { target },
                            mode: Some(DEFAULT_FILE_MODE),
                            owner: Some(ROOT),
                            group: Some(ROOT),
                            removed_before_added: false,
                            comment: instr.comment,
                        },
                    );
                }
                FsOp::CreateFifo => {
                    self.replace_node(
                        instr.path,
                        FsNode {
                            entry: FsEntry::Fifo,
                            mode: Some(DEFAULT_FILE_MODE),
                            owner: Some(ROOT),
                            group: Some(ROOT),
                            removed_before_added: false,
                            comment: instr.comment,
                        },
                    );
                }
                FsOp::CreateBlockDevice { major, minor } => {
                    self.replace_node(
                        instr.path,
                        FsNode {
                            entry: FsEntry::BlockDevice { major, minor },
                            mode: Some(DEFAULT_FILE_MODE),
                            owner: Some(ROOT),
                            group: Some(ROOT),
                            removed_before_added: false,
                            comment: instr.comment,
                        },
                    );
                }
                FsOp::CreateCharDevice { major, minor } => {
                    self.replace_node(
                        instr.path,
                        FsNode {
                            entry: FsEntry::CharDevice { major, minor },
                            mode: Some(DEFAULT_FILE_MODE),
                            owner: Some(ROOT),
                            group: Some(ROOT),
                            removed_before_added: false,
                            comment: instr.comment,
                        },
                    );
                }
                FsOp::SetMode { mode } => {
                    self.fs
                        .entry(instr.path.clone())
                        .and_modify(|entry| {
                            if warn_redundant && entry.mode == Some(mode) {
                                tracing::warn!("Redundant mode set for: {:?}", &instr.path);
                            }
                            entry.mode = Some(mode);
                        })
                        .or_insert_with(|| FsNode {
                            entry: FsEntry::Unchanged,
                            mode: Some(mode),
                            owner: None,
                            group: None,
                            removed_before_added: false,
                            comment: instr.comment,
                        });
                }
                FsOp::SetOwner { ref owner } => {
                    self.fs
                        .entry(instr.path.clone())
                        .and_modify(|entry| {
                            if warn_redundant && entry.owner.as_ref() == Some(owner) {
                                tracing::warn!("Redundant owner set for: {:?}", &instr.path);
                            }
                            entry.owner = Some(owner.clone());
                        })
                        .or_insert_with(|| FsNode {
                            entry: FsEntry::Unchanged,
                            mode: None,
                            owner: Some(owner.clone()),
                            group: None,
                            removed_before_added: false,
                            comment: instr.comment,
                        });
                }
                FsOp::SetGroup { ref group } => {
                    self.fs
                        .entry(instr.path.clone())
                        .and_modify(|entry| {
                            if warn_redundant && entry.group.as_ref() == Some(group) {
                                tracing::warn!("Redundant group set for: {:?}", &instr.path);
                            }
                            entry.group = Some(group.clone());
                        })
                        .or_insert_with(|| FsNode {
                            entry: FsEntry::Unchanged,
                            mode: None,
                            owner: None,
                            group: Some(group.clone()),
                            removed_before_added: false,
                            comment: instr.comment,
                        });
                }
                FsOp::Comment => (),
                FsOp::Restore { .. } => {
                    tracing::error!(
                        "Restore operation not supported as *input* to state::apply_instructions"
                    );
                }
            }
        }
    }

    /// Replace a node, taking into account if it was removed before being added back.
    fn replace_node(&mut self, path: Utf8PathBuf, new_node: FsNode) {
        self.add_missing_parents(&path);
        let entry = self.fs.entry(path).or_insert(FsNode {
            entry: FsEntry::Removed,
            mode: Some(Mode::new(0)),
            owner: Some(ROOT),
            group: Some(ROOT),
            removed_before_added: false,
            comment: None,
        });
        entry.entry = new_node.entry;
        entry.mode = new_node.mode;
        entry.owner = new_node.owner;
        entry.group = new_node.group;
        entry.comment = new_node.comment;
    }

    /// Add missing directory parents for a given node
    fn add_missing_parents(&mut self, path: &Utf8Path) {
        for parent in path.ancestors() {
            // TODO: Avoid allocation here?
            self.fs.entry(parent.into()).or_insert_with(|| FsNode {
                entry: FsEntry::Directory,
                mode: Some(DEFAULT_DIR_MODE),
                owner: Some(ROOT),
                group: Some(ROOT),
                removed_before_added: false,
                comment: None,
            });
        }
    }
}

/// Describe the goal of the diff: is it for saving or for application/diff
///
/// This will affect the exact instructions that gets generated
#[derive(Debug, Clone, strum::EnumDiscriminants)]
pub enum DiffGoal<'map, 'files> {
    Apply(Arc<dyn Files>, &'map PathMap<'files>),
    Save,
}

impl PartialEq for DiffGoal<'_, '_> {
    fn eq(&self, other: &Self) -> bool {
        #[allow(clippy::match_like_matches_macro)]
        match (self, other) {
            (DiffGoal::Apply(_, _), DiffGoal::Apply(_, _)) => true,
            (DiffGoal::Save, DiffGoal::Save) => true,
            _ => false,
        }
    }
}

// Generate a stream of instructions to go from state before to state after
pub fn diff(
    goal: &DiffGoal<'_, '_>,
    before: FsEntries,
    after: FsEntries,
) -> anyhow::Result<impl Iterator<Item = FsInstruction>> {
    let diff_iter = itertools::merge_join_by(before.fs, after.fs, |(k1, _), (k2, _)| k1.cmp(k2));

    let mut results = vec![];

    let mut id_resolver = NumericToNameResolveCache::new();

    for entry in diff_iter {
        match entry {
            itertools::EitherOrBoth::Both(before, after) if before.1 == after.1 => {}
            itertools::EitherOrBoth::Both(before, after) => {
                // Compare the structs and generate a stream of instructions
                let path = before.0;
                let before = before.1;
                let after = after.1;

                if before.entry != after.entry {
                    let before_discr = FsEntryDiscriminants::from(&before.entry);
                    let after_discr = FsEntryDiscriminants::from(&after.entry);

                    if before.removed_before_added || before_discr != after_discr {
                        // The entry was removed before being added back, generate a removal
                        results.push(FsInstruction {
                            path: path.clone(),
                            op: FsOp::Remove,
                            comment: Some(
                                "Removed (and later recreated) due to file type conflict".into(),
                            ),
                        });
                    }
                    // Just the properties of it has changed
                    let path = path.as_path();
                    if let Some(instr) = fsnode_into_base_instruction!(after, path) {
                        results.push(instr);
                    }
                }

                match (before.mode, after.mode) {
                    (None, None) => (),
                    (Some(_), None) => {
                        results.push(FsInstruction {
                            path: path.clone(),
                            op: FsOp::Comment,
                            comment: Some("Mode change unneeded".into()),
                        });
                    }
                    (Some(v1), Some(v2)) if v1 == v2 => (),
                    (None, Some(v)) | (Some(_), Some(v)) => {
                        results.push(FsInstruction {
                            path: path.clone(),
                            op: FsOp::SetMode { mode: v },
                            comment: None,
                        });
                    }
                }
                match (before.owner, after.owner) {
                    (None, None) => (),
                    (Some(_), None) => {
                        results.push(FsInstruction {
                            path: path.clone(),
                            op: FsOp::Comment,
                            comment: Some("Owner change unneeded".into()),
                        });
                    }
                    (Some(v1), Some(v2)) if v1 == v2 => (),
                    (None, Some(v)) | (Some(_), Some(v)) => {
                        results.push(FsInstruction {
                            path: path.clone(),
                            op: FsOp::SetOwner { owner: v },
                            comment: None,
                        });
                    }
                }
                match (before.group, after.group) {
                    (None, None) => (),
                    (Some(_), None) => {
                        results.push(FsInstruction {
                            path: path.clone(),
                            op: FsOp::Comment,
                            comment: Some("Group change unneeded".into()),
                        });
                    }
                    (Some(v1), Some(v2)) if v1 == v2 => (),
                    (None, Some(v)) | (Some(_), Some(v)) => {
                        results.push(FsInstruction {
                            path: path.clone(),
                            op: FsOp::SetGroup { group: v },
                            comment: None,
                        });
                    }
                }
            }
            itertools::EitherOrBoth::Left(before) => {
                match goal {
                    DiffGoal::Apply(ref _backend_impl, path_map) => {
                        // Figure out what the previous state of this file was:
                        match path_map.get(before.0.as_std_path()) {
                            Some(entry) => {
                                match entry.properties {
                                    Properties::RegularFileBasic(_)
                                    | Properties::RegularFileSystemd(_)
                                    | Properties::RegularFile(_) => {
                                        results.push(FsInstruction {
                                            path: before.0.clone(),
                                            op: FsOp::Restore,
                                            comment: before.1.comment,
                                        });
                                    }
                                    Properties::Symlink(ref v) => {
                                        results.push(FsInstruction {
                                            path: before.0.clone(),
                                            op: FsOp::CreateSymlink {
                                                target: Utf8Path::from_path(&v.target)
                                                    .ok_or_else(|| anyhow!("Invalid UTF-8"))?
                                                    .into(),
                                            },
                                            comment: before.1.comment,
                                        });
                                    }
                                    Properties::Directory(_) => {
                                        results.push(FsInstruction {
                                            path: before.0.clone(),
                                            op: FsOp::CreateDirectory,
                                            comment: before.1.comment,
                                        });
                                    }
                                    Properties::Fifo(_)
                                    | Properties::DeviceNode(_)
                                    | Properties::Permissions(_)
                                    | Properties::Special
                                    | Properties::Removed => {
                                        anyhow::bail!("{:?} needs to be restored to package manager state, but how do to that is not yet implemented", entry.path)
                                    }
                                    Properties::Unknown => {
                                        anyhow::bail!("{:?} needs to be restored to package manager state, but how do to that is unknown", entry.path)
                                    }
                                }
                                match (entry.properties.mode(), before.1.mode) {
                                    (None, None) | (None, Some(_)) | (Some(_), None) => (),
                                    (Some(v1), Some(v2)) if v1 == v2 => (),
                                    (Some(v1), Some(_)) => {
                                        results.push(FsInstruction {
                                            path: before.0.clone(),
                                            op: FsOp::SetMode { mode: v1 },
                                            comment: None,
                                        });
                                    }
                                }
                                let fs_owner = entry
                                    .properties
                                    .owner()
                                    .map(|v| id_resolver.lookup(&IdKey::User(v)))
                                    .transpose()?;
                                match (fs_owner, before.1.owner) {
                                    (None, None) | (None, Some(_)) | (Some(_), None) => (),
                                    (Some(v1), Some(v2)) if v1 == v2 => (),
                                    (Some(v1), Some(_)) => {
                                        results.push(FsInstruction {
                                            path: before.0.clone(),
                                            op: FsOp::SetOwner { owner: v1 },
                                            comment: None,
                                        });
                                    }
                                }
                                let fs_group = entry
                                    .properties
                                    .group()
                                    .map(|v| id_resolver.lookup(&IdKey::Group(v)))
                                    .transpose()?;
                                match (fs_group, before.1.group) {
                                    (None, None) | (None, Some(_)) | (Some(_), None) => (),
                                    (Some(v1), Some(v2)) if v1 == v2 => (),
                                    (Some(v1), Some(_)) => {
                                        results.push(FsInstruction {
                                            path: before.0.clone(),
                                            op: FsOp::SetGroup { group: v1 },
                                            comment: None,
                                        });
                                    }
                                }
                            }
                            None => {
                                results.push(FsInstruction {
                                    path: before.0,
                                    op: FsOp::Remove,
                                    comment: before.1.comment,
                                });
                            }
                        }
                    }
                    DiffGoal::Save => {
                        // Generate instructions to remove the entry
                        results.push(FsInstruction {
                            path: before.0,
                            op: FsOp::Remove,
                            comment: before.1.comment,
                        });
                        // TODO: Do something special when the before instruction is a removal one?I
                    }
                }
            }
            itertools::EitherOrBoth::Right(after) => {
                results.extend(after.1.into_instruction(&after.0));
            }
        }
    }

    Ok(results.into_iter())
}

#[cfg(test)]
mod tests {
    use FsOp;

    use super::*;

    #[test]
    fn test_apply_instructions() {
        let mut entries = FsEntries::default();
        let instrs = vec![
            FsInstruction {
                path: "/hello/symlink".into(),
                op: FsOp::CreateSymlink {
                    target: "/hello/target".into(),
                },
                comment: None,
            },
            FsInstruction {
                path: "/hello/file".into(),
                op: FsOp::CreateFile(FileContents::from_literal(
                    b"hello".to_vec().into_boxed_slice(),
                )),
                comment: Some("A comment".into()),
            },
            FsInstruction {
                path: "/hello/file".into(),
                op: FsOp::SetMode {
                    mode: Mode::new(0o600),
                },
                comment: None,
            },
        ];
        entries.apply_instructions(instrs.into_iter(), false);

        assert_eq!(
            entries.fs.get(Utf8Path::new("/hello/symlink")),
            Some(&FsNode {
                entry: FsEntry::Symlink {
                    target: "/hello/target".into()
                },
                mode: Some(DEFAULT_FILE_MODE),
                owner: Some(ROOT),
                group: Some(ROOT),
                removed_before_added: false,
                comment: None,
            })
        );
        assert_eq!(
            entries.fs.get(Utf8Path::new("/hello/file")),
            Some(&FsNode {
                entry: FsEntry::File(FileContents::from_literal(
                    b"hello".to_vec().into_boxed_slice()
                )),
                mode: Some(Mode::new(0o600)),
                owner: Some(ROOT),
                group: Some(ROOT),
                removed_before_added: false,
                comment: Some("A comment".into()),
            })
        );
        assert_eq!(
            entries.fs.get(Utf8Path::new("/hello")),
            Some(&FsNode {
                entry: FsEntry::Directory,
                mode: Some(DEFAULT_DIR_MODE),
                owner: Some(ROOT),
                group: Some(ROOT),
                removed_before_added: false,
                comment: None,
            })
        );
        assert_eq!(
            entries.fs.get(Utf8Path::new("/")),
            Some(&FsNode {
                entry: FsEntry::Directory,
                mode: Some(DEFAULT_DIR_MODE),
                owner: Some(ROOT),
                group: Some(ROOT),
                removed_before_added: false,
                comment: None,
            })
        );
    }
}