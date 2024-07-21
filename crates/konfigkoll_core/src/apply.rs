//! Apply a stream of instructions to the current system

use std::{collections::BTreeMap, fs::Permissions, os::unix::fs::PermissionsExt, sync::Arc};

use ahash::AHashMap;
use anyhow::Context;
use either::Either;
use itertools::Itertools;
use konfigkoll_types::{FsInstruction, FsOp, FsOpDiscriminants, PkgIdent, PkgInstruction, PkgOp};
use paketkoll_types::{
    backend::{Backend, Files, OriginalFileQuery, PackageBackendMap, PackageMap},
    intern::Interner,
};

use crate::{
    confirm::MultiOptionConfirm,
    diff::show_fs_instr_diff,
    utils::{IdKey, NameToNumericResolveCache},
};
use console::style;

/// Applier of system changes
///
/// Different implementors of this trait handle things like:
/// * Privilege separation
/// * Interactive confirmation
/// * Actual applying to the system
pub trait Applicator {
    /// Apply package changes
    fn apply_pkgs<'instructions>(
        &mut self,
        backend: Backend,
        install: &[&'instructions str],
        uninstall: &[&'instructions str],
    ) -> anyhow::Result<()>;

    /// Apply file changes
    fn apply_files(&mut self, instructions: &[&FsInstruction]) -> anyhow::Result<()>;
}

impl<L, R> Applicator for Either<L, R>
where
    L: Applicator,
    R: Applicator,
{
    fn apply_pkgs<'instructions>(
        &mut self,
        backend: Backend,
        install: &[&'instructions str],
        uninstall: &[&'instructions str],
    ) -> anyhow::Result<()> {
        match self {
            Either::Left(inner) => inner.apply_pkgs(backend, install, uninstall),
            Either::Right(inner) => inner.apply_pkgs(backend, install, uninstall),
        }
    }

    fn apply_files(&mut self, instructions: &[&FsInstruction]) -> anyhow::Result<()> {
        match self {
            Either::Left(inner) => inner.apply_files(instructions),
            Either::Right(inner) => inner.apply_files(instructions),
        }
    }
}

/// Apply with no privilege separation
#[derive(Debug)]
pub struct InProcessApplicator {
    package_backends: PackageBackendMap,
    file_backend: Arc<dyn Files>,
    interner: Arc<Interner>,
    package_maps: BTreeMap<Backend, Arc<PackageMap>>,
    id_resolver: NameToNumericResolveCache,
}

impl InProcessApplicator {
    pub fn new(
        package_backends: PackageBackendMap,
        interner: &Arc<Interner>,
        package_maps: &BTreeMap<Backend, Arc<PackageMap>>,
        file_backend: &Arc<dyn Files>,
    ) -> Self {
        Self {
            package_backends,
            file_backend: file_backend.clone(),
            interner: Arc::clone(interner),
            package_maps: package_maps.clone(),
            id_resolver: NameToNumericResolveCache::new(),
        }
    }
}

impl Applicator for InProcessApplicator {
    fn apply_pkgs<'instructions>(
        &mut self,
        backend: Backend,
        install: &[&'instructions str],
        uninstall: &[&'instructions str],
    ) -> anyhow::Result<()> {
        tracing::info!(
            "Proceeding with installing {:?} and uninstalling {:?} with backend {:?}",
            install,
            uninstall,
            backend
        );
        let backend = self
            .package_backends
            .get(&backend)
            .ok_or_else(|| anyhow::anyhow!("Unknown backend: {:?}", backend))?;
        backend.transact(install, uninstall, true)?;
        Ok(())
    }

    fn apply_files(&mut self, instructions: &[&FsInstruction]) -> anyhow::Result<()> {
        let pkg_map = self
            .package_maps
            .get(&self.file_backend.as_backend_enum())
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "No package map for file backend {:?}",
                    self.file_backend.as_backend_enum()
                )
            })?;
        for instr in instructions {
            tracing::info!("Applying: {}: {}", instr.path, instr.op);
            match &instr.op {
                FsOp::Remove => {
                    let existing = std::fs::symlink_metadata(&instr.path);
                    if let Ok(metadata) = existing {
                        if metadata.is_dir() {
                            std::fs::remove_dir(&instr.path)?;
                        } else {
                            std::fs::remove_file(&instr.path)?;
                        }
                    }
                }
                FsOp::CreateDirectory => {
                    std::fs::create_dir(&instr.path)?;
                }
                FsOp::CreateFile(contents) => match contents {
                    konfigkoll_types::FileContents::Literal { checksum: _, data } => {
                        std::fs::write(&instr.path, data)?;
                    }
                    konfigkoll_types::FileContents::FromFile { checksum: _, path } => {
                        std::fs::copy(path, &instr.path)?;
                    }
                },
                FsOp::CreateSymlink { target } => {
                    std::os::unix::fs::symlink(target, &instr.path)?;
                }
                FsOp::CreateFifo => {
                    // Since we split out mode in general, we don't know what to put here.
                    // Use empty, and let later instructions set it correctly.
                    nix::unistd::mkfifo(instr.path.as_std_path(), nix::sys::stat::Mode::empty())?;
                }
                FsOp::CreateBlockDevice { major, minor } => {
                    // Like with fifo, we don't know mode yet.
                    nix::sys::stat::mknod(
                        instr.path.as_std_path(),
                        nix::sys::stat::SFlag::S_IFBLK,
                        nix::sys::stat::Mode::empty(),
                        nix::sys::stat::makedev(*major, *minor),
                    )?;
                }
                FsOp::CreateCharDevice { major, minor } => {
                    // Like with fifo, we don't know mode yet.
                    nix::sys::stat::mknod(
                        instr.path.as_std_path(),
                        nix::sys::stat::SFlag::S_IFCHR,
                        nix::sys::stat::Mode::empty(),
                        nix::sys::stat::makedev(*major, *minor),
                    )?;
                }
                FsOp::SetMode { mode } => {
                    let perms = Permissions::from_mode(mode.as_raw());
                    std::fs::set_permissions(&instr.path, perms)?;
                }
                FsOp::SetOwner { owner } => {
                    let uid = nix::unistd::Uid::from_raw(
                        self.id_resolver.lookup(&IdKey::User(owner.clone()))?,
                    );
                    nix::unistd::chown(instr.path.as_std_path(), Some(uid), None)?;
                }
                FsOp::SetGroup { group } => {
                    let gid = nix::unistd::Gid::from_raw(
                        self.id_resolver.lookup(&IdKey::Group(group.clone()))?,
                    );
                    nix::unistd::chown(instr.path.as_std_path(), None, Some(gid))?;
                }
                FsOp::Restore => {
                    // Get package:
                    let owners = self
                        .file_backend
                        .owning_packages(&[instr.path.as_std_path()].into(), &self.interner)
                        .with_context(|| format!("Failed to find owner for {}", instr.path))?;
                    let package = owners
                        .get(instr.path.as_std_path())
                        .with_context(|| format!("Failed to find owner for {}", instr.path))?
                        .ok_or_else(|| anyhow::anyhow!("No owner for {}", instr.path))?;
                    let package = package.to_str(&self.interner);
                    // Get original contents:
                    let queries = [OriginalFileQuery {
                        package: package.into(),
                        path: instr.path.as_str().into(),
                    }];
                    let original_contents =
                        self.file_backend
                            .original_files(&queries, pkg_map, &self.interner)?;
                    // Apply
                    for query in queries {
                        let contents = original_contents.get(&query).ok_or_else(|| {
                            anyhow::anyhow!("No original contents for {:?}", query)
                        })?;
                        std::fs::write(&instr.path, contents)?;
                    }
                    // TODO: Permissions and modes
                    // TODO: Symlinks etc
                }
                FsOp::Comment => (),
            }
        }
        Ok(())
    }
}

/// An applicator that asks for confirmation before applying changes
#[derive(Debug)]
pub struct InteractiveApplicator<Inner: std::fmt::Debug> {
    inner: Inner,
    pkg_confirmer: MultiOptionConfirm,
    fs_confirmer: MultiOptionConfirm,
    interactive_confirmer: MultiOptionConfirm,
    diff_command: Vec<String>,
    pager_command: Vec<String>,
}

impl<Inner: std::fmt::Debug> InteractiveApplicator<Inner> {
    pub fn new(inner: Inner, diff_command: Vec<String>, pager_command: Vec<String>) -> Self {
        let mut prompt_builder = MultiOptionConfirm::builder();
        prompt_builder
            .prompt("Do you want to apply these changes?")
            .option('y', "Yes")
            .option('n', "No")
            .option('d', "show Diff");
        let pkg_confirmer = prompt_builder.build();
        prompt_builder.option('i', "Interactive (change by change)");
        let fs_confirmer = prompt_builder.build();

        let mut prompt_builder = MultiOptionConfirm::builder();
        prompt_builder
            .prompt("Apply changes to this file?")
            .option('y', "Yes")
            .option('a', "Abort")
            .option('s', "Skip")
            .option('d', "show Diff");
        let interactive_confirmer = prompt_builder.build();

        Self {
            inner,
            pkg_confirmer,
            fs_confirmer,
            interactive_confirmer,
            diff_command,
            pager_command,
        }
    }
}

impl<Inner: Applicator + std::fmt::Debug> Applicator for InteractiveApplicator<Inner> {
    fn apply_pkgs<'instructions>(
        &mut self,
        backend: Backend,
        install: &[&'instructions str],
        uninstall: &[&'instructions str],
    ) -> anyhow::Result<()> {
        tracing::info!(
            "Will install {:?} and uninstall {:?} with backend {backend}",
            install.len(),
            uninstall.len(),
        );

        loop {
            match self.pkg_confirmer.prompt()? {
                'y' => {
                    tracing::info!("Applying changes");
                    return self.inner.apply_pkgs(backend, install, uninstall);
                }
                'n' => {
                    tracing::info!("Aborting");
                    return Err(anyhow::anyhow!("User aborted"));
                }
                'd' => {
                    println!("With package manager {backend}:");
                    for pkg in install {
                        println!(" {} {}", style("+").green(), pkg);
                    }
                    for pkg in uninstall {
                        println!(" {} {}", style("-").red(), pkg);
                    }
                }
                _ => return Err(anyhow::anyhow!("Unexpected branch (internal error)")),
            }
        }
    }

    fn apply_files(&mut self, instructions: &[&FsInstruction]) -> anyhow::Result<()> {
        tracing::info!("Will apply {} file instructions", instructions.len());
        loop {
            match self.fs_confirmer.prompt()? {
                'y' => {
                    tracing::info!("Applying changes");
                    return self.inner.apply_files(instructions);
                }
                'n' => {
                    tracing::info!("Aborting");
                    return Err(anyhow::anyhow!("User aborted"));
                }
                'd' => {
                    println!("With file system:");
                    for instr in instructions {
                        println!(" {}: {}", style(instr.path.as_str()).blue(), instr.op);
                    }
                }
                'i' => {
                    for instr in instructions {
                        self.interactive_apply_single_file(instr)?;
                    }
                    return Ok(());
                }
                _ => return Err(anyhow::anyhow!("Unexpected branch (internal error)")),
            }
        }
    }
}

impl<Inner: Applicator + std::fmt::Debug> InteractiveApplicator<Inner> {
    fn interactive_apply_single_file(
        &mut self,
        instr: &&FsInstruction,
    ) -> Result<(), anyhow::Error> {
        println!(
            "Under consideration: {} with change {}",
            style(instr.path.as_str()).blue(),
            instr.op
        );
        loop {
            match self.interactive_confirmer.prompt()? {
                'y' => {
                    tracing::info!("Applying change to {}", instr.path);
                    return self.inner.apply_files(&[instr]);
                }
                'a' => {
                    tracing::info!("Aborting");
                    return Err(anyhow::anyhow!("User aborted"));
                }
                's' => {
                    tracing::info!("Skipping {}", instr.path);
                    return Ok(());
                }
                'd' => {
                    show_fs_instr_diff(
                        instr,
                        self.diff_command.as_slice(),
                        self.pager_command.as_slice(),
                    )?;
                }
                _ => return Err(anyhow::anyhow!("Unexpected branch (internal error)")),
            };
        }
    }
}

/// Just print, don't actually apply.
#[derive(Debug, Default)]
pub struct NoopApplicator {}

impl Applicator for NoopApplicator {
    fn apply_pkgs<'instructions>(
        &mut self,
        backend: Backend,
        install: &[&'instructions str],
        uninstall: &[&'instructions str],
    ) -> anyhow::Result<()> {
        tracing::info!(
            "Would install {:?} and uninstall {:?} with backend {:?}",
            install.len(),
            uninstall.len(),
            backend
        );
        for pkg in install {
            tracing::info!(" + {}", pkg);
        }
        for pkg in uninstall {
            tracing::info!(" - {}", pkg);
        }
        Ok(())
    }

    fn apply_files(&mut self, instructions: &[&FsInstruction]) -> anyhow::Result<()> {
        tracing::info!("Would apply {} file instructions", instructions.len());
        for instr in instructions {
            tracing::info!(" {}: {}", instr.path, instr.op);
        }
        Ok(())
    }
}

pub fn apply_files<'instructions>(
    applicator: &mut dyn Applicator,
    instructions: impl Iterator<Item = &'instructions FsInstruction>,
) -> anyhow::Result<()> {
    // Sort and group by type of operation, to make changes easier to review
    let instructions = instructions
        .sorted_by(|a, b| a.op.cmp(&b.op).then_with(|| a.path.cmp(&b.path)))
        .collect_vec();
    let chunked_instructions = instructions
        .iter()
        .chunk_by(|e| FsOpDiscriminants::from(&e.op));
    // Process each chunk separately
    for (_discr, chunk) in chunked_instructions.into_iter() {
        let chunk = chunk.cloned().collect_vec();
        applicator.apply_files(chunk.as_slice())?;
    }
    Ok(())
}

/// Apply package changes
pub fn apply_packages<'instructions>(
    applicator: &mut dyn Applicator,
    instructions: impl Iterator<Item = (&'instructions PkgIdent, &'instructions PkgInstruction)>,
) -> anyhow::Result<()> {
    // Sort into backends
    let mut sorted = AHashMap::new();
    for (pkg, instr) in instructions {
        let backend = pkg.package_manager;
        let entry = sorted
            .entry(backend)
            .or_insert_with(|| (Vec::new(), Vec::new()));
        match instr.op {
            PkgOp::Install => entry.0.push(pkg.identifier.as_str()),
            PkgOp::Uninstall => entry.1.push(pkg.identifier.as_str()),
        }
    }

    // Apply with applicator
    for (backend, (install, uninstall)) in sorted {
        applicator.apply_pkgs(backend, &install, &uninstall)?;
    }
    Ok(())
}
