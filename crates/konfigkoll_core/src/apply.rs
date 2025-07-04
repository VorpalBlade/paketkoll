//! Apply a stream of instructions to the current system

use crate::confirm::Choices;
use crate::confirm::MultiOptionConfirm;
use crate::diff::show_fs_instr_diff;
use crate::utils::IdKey;
use crate::utils::NameToNumericResolveCache;
use crate::utils::original_file_contents;
use crate::utils::pkg_backend_for_files;
use ahash::AHashMap;
use color_eyre::Section;
use console::style;
use either::Either;
use eyre::WrapErr;
use konfigkoll_types::FsInstruction;
use konfigkoll_types::FsOp;
use konfigkoll_types::FsOpDiscriminants;
use konfigkoll_types::PkgIdent;
use konfigkoll_types::PkgInstruction;
use konfigkoll_types::PkgOp;
use paketkoll_types::backend::Backend;
use paketkoll_types::backend::Files;
use paketkoll_types::backend::PackageBackendMap;
use paketkoll_types::backend::PackageMap;
use paketkoll_types::backend::PackageMapMap;
use paketkoll_types::intern::Interner;
use paketkoll_types::intern::PackageRef;
use std::fs::Permissions;
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::fs::PermissionsExt;
use std::sync::Arc;

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
        mark_explicit: &[&'instructions str],
        uninstall: &[&'instructions str],
    ) -> eyre::Result<()>;

    /// Apply file changes
    fn apply_files(&mut self, instructions: &[FsInstruction]) -> eyre::Result<()>;
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
        mark_explicit: &[&'instructions str],
        uninstall: &[&'instructions str],
    ) -> eyre::Result<()> {
        match self {
            Self::Left(inner) => inner.apply_pkgs(backend, install, mark_explicit, uninstall),
            Self::Right(inner) => inner.apply_pkgs(backend, install, mark_explicit, uninstall),
        }
    }

    fn apply_files(&mut self, instructions: &[FsInstruction]) -> eyre::Result<()> {
        match self {
            Self::Left(inner) => inner.apply_files(instructions),
            Self::Right(inner) => inner.apply_files(instructions),
        }
    }
}

/// Apply with no privilege separation
#[derive(Debug)]
pub struct InProcessApplicator {
    package_backends: PackageBackendMap,
    file_backend: Arc<dyn Files>,
    interner: Arc<Interner>,
    package_maps: PackageMapMap,
    id_resolver: NameToNumericResolveCache,
}

impl InProcessApplicator {
    pub fn new(
        package_backends: PackageBackendMap,
        interner: &Arc<Interner>,
        package_maps: &PackageMapMap,
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

    fn apply_single_file(
        &mut self,
        instr: &FsInstruction,
        pkg_map: &PackageMap,
    ) -> eyre::Result<()> {
        tracing::info!("Applying: {}: {}", instr.path, instr.op);
        if instr.op != FsOp::Comment
            && instr.op != FsOp::Remove
            && let Some(parent) = instr.path.parent()
        {
            std::fs::create_dir_all(parent).wrap_err("Failed to create parent directory")?;
        }
        match &instr.op {
            FsOp::Remove => {
                let existing = std::fs::symlink_metadata(&instr.path);
                if let Ok(metadata) = existing {
                    if metadata.is_dir() {
                        match std::fs::remove_dir(&instr.path) {
                            Ok(()) => (),
                            Err(err) => match err.raw_os_error() {
                                Some(libc::ENOTEMPTY) => {
                                    Err(err)
                                        .wrap_err(
                                            "Failed to remove directory: it is not empty \
                                             (possibly it contains some ignored files)",
                                        )
                                        .suggestion(
                                            "You will have to investigate the non-empty directory \
                                             and decide if you want to remove it yourself, since \
                                             we don't want to delete things we shouldn't.",
                                        )?;
                                }
                                Some(_) | None => {
                                    Err(err).wrap_err("Failed to remove directory")?;
                                }
                            },
                        }
                    } else {
                        std::fs::remove_file(&instr.path)?;
                    }
                }
            }
            FsOp::CreateDirectory => {
                std::fs::create_dir_all(&instr.path)?;
            }
            FsOp::CreateFile(contents) => {
                match contents {
                    konfigkoll_types::FileContents::Literal { checksum: _, data } => {
                        std::fs::write(&instr.path, data).wrap_err("Failed to write file data")?;
                    }
                    konfigkoll_types::FileContents::FromFile { checksum: _, path } => {
                        // std::fs::copy copies permissions, which we don't want (we want the
                        // file to be owned by root with default permissions until an
                        // instruction says otherwise), so we can't use it.
                        let mut target_file = std::fs::OpenOptions::new()
                            .write(true)
                            .truncate(true)
                            .create(true)
                            .mode(0o644)
                            .open(&instr.path)
                            .wrap_err("Failed to open target file for writing")?;
                        let mut source_file = std::fs::File::open(path)
                            .wrap_err("Failed to open source file for reading")?;
                        std::io::copy(&mut source_file, &mut target_file)
                            .wrap_err("Failed to copy file contents")?;
                    }
                }
            }
            FsOp::CreateSymlink { target } => {
                match std::os::unix::fs::symlink(target, &instr.path) {
                    Ok(()) => Ok(()),
                    Err(err) => {
                        if err.kind() == std::io::ErrorKind::AlreadyExists {
                            // If the symlink already exists, we can just remove it and try
                            // again
                            std::fs::remove_file(&instr.path)
                                .wrap_err("Failed to remove old file before creating symlink")?;
                            std::os::unix::fs::symlink(target, &instr.path)
                        } else {
                            Err(err)
                        }
                    }
                }
                .wrap_err("Failed to create symlink")?;
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
                let contents =
                    original_file_contents(&*self.file_backend, &self.interner, instr, pkg_map)?;
                // Apply
                std::fs::write(&instr.path, contents)?;
            }
            FsOp::Comment => {
                tracing::warn!(
                    "Ignoring comment instruction, we shouldn't ever get here: {:?}",
                    instr
                );
            }
        };
        Ok(())
    }
}

impl Applicator for InProcessApplicator {
    fn apply_pkgs<'instructions>(
        &mut self,
        backend: Backend,
        install: &[&'instructions str],
        mark_explicit: &[&'instructions str],
        uninstall: &[&'instructions str],
    ) -> eyre::Result<()> {
        tracing::info!(
            "Proceeding with installing {:?} and uninstalling {:?} with backend {:?}",
            install,
            uninstall,
            backend
        );
        let backend = self
            .package_backends
            .get(&backend)
            .ok_or_else(|| eyre::eyre!("Unknown backend: {:?}", backend))?;

        tracing::info!("Installing packages...");
        backend.transact(install, &[], true)?;
        tracing::info!("Marking packages explicit...");
        backend.mark(&[], mark_explicit)?;
        tracing::info!("Attempting to mark unwanted packages as dependencies...");
        match backend.mark(uninstall, &[]) {
            Ok(()) => {
                tracing::info!("Successfully marked unwanted packages as dependencies");
                tracing::info!("Removing unused packages...");
                backend.remove_unused(true)?;
            }
            Err(paketkoll_types::backend::PackageManagerError::UnsupportedOperation(_)) => {
                tracing::info!(
                    "Marking unwanted packages as dependencies not supported, using uninstall \
                     instead"
                );
                backend.transact(&[], uninstall, true)?;
            }
            Err(e) => return Err(e.into()),
        }

        Ok(())
    }

    fn apply_files(&mut self, instructions: &[FsInstruction]) -> eyre::Result<()> {
        let pkg_map = pkg_backend_for_files(&self.package_maps, &*self.file_backend)?;
        for instr in instructions {
            self.apply_single_file(instr, &pkg_map).wrap_err_with(|| {
                format!("Failed to apply change for {}: {:?}", instr.path, instr.op)
            })?;
        }
        Ok(())
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum PkgPromptChoices {
    Yes,
    Abort,
    Skip,
}

impl Choices for PkgPromptChoices {
    fn options() -> &'static [(char, &'static str, Self)] {
        &[
            ('y', "Yes", Self::Yes),
            ('a', "Abort", Self::Abort),
            ('s', "Skip", Self::Skip),
        ]
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum FsPromptChoices {
    Yes,
    Abort,
    Skip,
    Interactive,
}

impl Choices for FsPromptChoices {
    fn options() -> &'static [(char, &'static str, Self)] {
        &[
            ('y', "Yes", Self::Yes),
            ('a', "Abort", Self::Abort),
            ('s', "Skip", Self::Skip),
            ('i', "Interactive (change by change)", Self::Interactive),
        ]
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
enum InteractivePromptChoices {
    Yes,
    Abort,
    Skip,
    ShowDiff,
}

impl Choices for InteractivePromptChoices {
    fn options() -> &'static [(char, &'static str, Self)] {
        &[
            ('y', "Yes", Self::Yes),
            ('a', "Abort", Self::Abort),
            ('s', "Skip", Self::Skip),
            ('d', "show Diff", Self::ShowDiff),
        ]
    }
}

/// An applicator that asks for confirmation before applying changes
#[derive(Debug)]
pub struct InteractiveApplicator<Inner: std::fmt::Debug> {
    inner: Inner,
    pkg_confirmer: MultiOptionConfirm<PkgPromptChoices>,
    fs_confirmer: MultiOptionConfirm<FsPromptChoices>,
    interactive_confirmer: MultiOptionConfirm<InteractivePromptChoices>,
    diff_command: Vec<String>,
    pager_command: Vec<String>,
    file_backend: Arc<dyn Files>,
    interner: Arc<Interner>,
    package_maps: PackageMapMap,
}

impl<Inner: std::fmt::Debug> InteractiveApplicator<Inner> {
    pub fn new(
        inner: Inner,
        diff_command: Vec<String>,
        pager_command: Vec<String>,
        file_backend: &Arc<dyn Files>,
        interner: &Arc<Interner>,
        package_maps: &PackageMapMap,
    ) -> Self {
        let mut prompt_builder = MultiOptionConfirm::builder();
        prompt_builder.prompt("Do you want to apply these changes?");
        let pkg_confirmer = prompt_builder.build();
        let mut prompt_builder = MultiOptionConfirm::builder();
        prompt_builder.prompt("Do you want to apply these changes?");
        let fs_confirmer = prompt_builder.build();

        let mut prompt_builder = MultiOptionConfirm::builder();
        prompt_builder.prompt("Apply changes to this file?");
        let interactive_confirmer = prompt_builder.build();

        Self {
            inner,
            pkg_confirmer,
            fs_confirmer,
            interactive_confirmer,
            diff_command,
            pager_command,
            file_backend: file_backend.clone(),
            interner: interner.clone(),
            package_maps: package_maps.clone(),
        }
    }
}

impl<Inner: Applicator + std::fmt::Debug> Applicator for InteractiveApplicator<Inner> {
    fn apply_pkgs<'instructions>(
        &mut self,
        backend: Backend,
        install: &[&'instructions str],
        mark_explicit: &[&'instructions str],
        uninstall: &[&'instructions str],
    ) -> eyre::Result<()> {
        tracing::info!(
            "Will install {:?}, mark {:?} as explicit and uninstall {:?} with backend {backend}",
            install.len(),
            mark_explicit.len(),
            uninstall.len(),
        );
        show_pkg_diff(backend, install, mark_explicit, uninstall);

        match self.pkg_confirmer.prompt()? {
            PkgPromptChoices::Yes => {
                tracing::info!("Applying changes");
                self.inner
                    .apply_pkgs(backend, install, mark_explicit, uninstall)
            }
            PkgPromptChoices::Abort => {
                tracing::error!("Aborting");
                Err(eyre::eyre!("User aborted"))
            }
            PkgPromptChoices::Skip => {
                tracing::warn!("Skipping");
                Ok(())
            }
        }
    }

    fn apply_files(&mut self, instructions: &[FsInstruction]) -> eyre::Result<()> {
        tracing::info!("Will apply {} file instructions", instructions.len());
        show_fs_diff(instructions);
        match self.fs_confirmer.prompt()? {
            FsPromptChoices::Yes => {
                tracing::info!("Applying changes");
                self.inner.apply_files(instructions)
            }
            FsPromptChoices::Abort => {
                tracing::error!("Aborting");
                Err(eyre::eyre!("User aborted"))
            }
            FsPromptChoices::Skip => {
                tracing::warn!("Skipping");
                Ok(())
            }
            FsPromptChoices::Interactive => {
                let pkg_map = pkg_backend_for_files(&self.package_maps, &*self.file_backend)?;
                for instr in instructions {
                    self.interactive_apply_single_file(instr, &pkg_map)?;
                }
                Ok(())
            }
        }
    }
}

fn show_fs_diff(instructions: &[FsInstruction]) {
    println!("Would apply file system changes:");
    for instr in instructions {
        println!(" {}: {}", style(instr.path.as_str()).blue(), instr.op);
    }
}

fn show_pkg_diff(backend: Backend, install: &[&str], mark_explicit: &[&str], uninstall: &[&str]) {
    println!("With package manager {backend}:");
    for pkg in install {
        println!(" {} {}", style("+").green(), pkg);
    }
    for pkg in mark_explicit {
        println!(" {} {} (mark explicit)", style("E").green(), pkg);
    }
    for pkg in uninstall {
        println!(" {} {}", style("-").red(), pkg);
    }
}

impl<Inner: Applicator + std::fmt::Debug> InteractiveApplicator<Inner> {
    fn interactive_apply_single_file(
        &mut self,
        instr: &FsInstruction,
        pkg_map: &PackageMap,
    ) -> eyre::Result<()> {
        println!(
            "Under consideration: {} with change {}",
            style(instr.path.as_str()).blue(),
            instr.op
        );
        loop {
            match self.interactive_confirmer.prompt()? {
                InteractivePromptChoices::Yes => {
                    tracing::info!("Applying change to {}", instr.path);
                    return self.inner.apply_files(std::slice::from_ref(instr));
                }
                InteractivePromptChoices::Abort => {
                    tracing::info!("Aborting");
                    return Err(eyre::eyre!("User aborted"));
                }
                InteractivePromptChoices::Skip => {
                    tracing::info!("Skipping {}", instr.path);
                    return Ok(());
                }
                InteractivePromptChoices::ShowDiff => {
                    show_fs_instr_diff(
                        instr,
                        self.diff_command.as_slice(),
                        self.pager_command.as_slice(),
                        &self.interner,
                        &*self.file_backend,
                        pkg_map,
                    )?;
                }
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
        mark_explicit: &[&'instructions str],
        uninstall: &[&'instructions str],
    ) -> eyre::Result<()> {
        tracing::info!(
            "Would install {:?}, mark {:?} explicit and uninstall {:?} with backend {:?}",
            install.len(),
            mark_explicit.len(),
            uninstall.len(),
            backend
        );

        for pkg in install {
            tracing::info!(" + {}", pkg);
        }
        for pkg in mark_explicit {
            tracing::info!("   {} (mark explicit)", pkg);
        }
        for pkg in uninstall {
            tracing::info!(" - {}", pkg);
        }
        Ok(())
    }

    fn apply_files(&mut self, instructions: &[FsInstruction]) -> eyre::Result<()> {
        tracing::info!("Would apply {} file instructions", instructions.len());
        for instr in instructions {
            tracing::info!(" {}: {}", instr.path, instr.op);
        }
        Ok(())
    }
}

pub fn apply_files(
    applicator: &mut dyn Applicator,
    instructions: &mut [FsInstruction],
) -> eyre::Result<()> {
    // Sort and group by type of operation, to make changes easier to review
    instructions.sort_by(|a, b| {
        FsOpDiscriminants::from(&a.op)
            .cmp(&FsOpDiscriminants::from(&b.op))
            .then_with(|| a.path.cmp(&b.path))
    });
    let chunked_instructions = instructions
        .chunk_by_mut(|a, b| FsOpDiscriminants::from(&a.op) == FsOpDiscriminants::from(&b.op));
    // Process each chunk separately
    for chunk in chunked_instructions {
        // Removing things has to be sorted reverse, so we remove contents before the
        // directory they are containers of
        if chunk[0].op == FsOp::Remove {
            chunk.reverse();
        };
        // These make no sense during apply (only for save)
        if chunk[0].op == FsOp::Comment {
            tracing::warn!("There are entries in your config that are no longer needed:");
            for instr in chunk {
                if let Some(comment) = &instr.comment {
                    tracing::warn!(" * {}: {comment}", instr.path);
                }
            }
            continue;
        }
        applicator
            .apply_files(&*chunk)
            .wrap_err("Error while applying files")?;
    }
    Ok(())
}

#[derive(Default)]
struct PackageOperations<'a> {
    install: Vec<&'a str>,
    mark_as_manual: Vec<&'a str>,
    uninstall: Vec<&'a str>,
}

/// Apply package changes
pub fn apply_packages<'instructions>(
    applicator: &mut dyn Applicator,
    instructions: impl Iterator<Item = (&'instructions PkgIdent, PkgInstruction)>,
    package_maps: &PackageMapMap,
    interner: &Interner,
) -> eyre::Result<()> {
    // Sort into backends
    let mut sorted = AHashMap::new();
    for (pkg, instr) in instructions {
        let backend = pkg.package_manager;
        let entry = sorted
            .entry(backend)
            .or_insert_with(PackageOperations::default);
        let sub_map = package_maps
            .get(&backend)
            .ok_or_else(|| eyre::eyre!("No package map for backend {:?}", backend))?;
        // Deal with the case where a package is installed as a dependency and we want
        // it explicit
        let pkg_ref = PackageRef::get_or_intern(interner, pkg.identifier.as_str());
        let has_pkg = sub_map.get(&pkg_ref).is_some();
        match (instr.op, has_pkg) {
            (PkgOp::Install, true) => entry.mark_as_manual.push(pkg.identifier.as_str()),
            (PkgOp::Install, false) => entry.install.push(pkg.identifier.as_str()),
            (PkgOp::Uninstall, _) => entry.uninstall.push(pkg.identifier.as_str()),
        }
    }

    // Apply with applicator
    for (backend, operations) in sorted {
        applicator
            .apply_pkgs(
                backend,
                &operations.install,
                &operations.mark_as_manual,
                &operations.uninstall,
            )
            .wrap_err_with(|| format!("Error while applying packages with {backend}"))?;
    }
    Ok(())
}
