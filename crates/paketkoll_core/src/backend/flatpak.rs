//! Package backend for flatpak

use std::process::{Command, Stdio};

use crate::utils::package_manager_transaction;
use anyhow::Context;
use paketkoll_types::backend::{Name, PackageManagerError, Packages};
use paketkoll_types::package::InstallReason;
use paketkoll_types::{
    intern::{ArchitectureRef, PackageRef},
    package::{Package, PackageInstallStatus, PackageInterned},
};
use smallvec::SmallVec;

/// Flatpak backend
#[derive(Debug)]
pub(crate) struct Flatpak {}

#[derive(Debug, Default)]
pub(crate) struct FlatpakBuilder {}

impl FlatpakBuilder {
    pub fn build(self) -> Flatpak {
        Flatpak {}
    }
}

impl Name for Flatpak {
    fn name(&self) -> &'static str {
        "Flatpak"
    }

    fn as_backend_enum(&self) -> paketkoll_types::backend::Backend {
        paketkoll_types::backend::Backend::Flatpak
    }
}

impl Packages for Flatpak {
    fn packages(
        &self,
        interner: &paketkoll_types::intern::Interner,
    ) -> anyhow::Result<Vec<PackageInterned>> {
        let cmd = Command::new("flatpak")
            .arg("list")
            .arg("--system")
            .arg("--columns=ref,origin,name,version,description,options")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .context("Failed to spawn \"flatpak list\" (is flatpak installed and in PATH?)")?;
        let output = cmd
            .wait_with_output()
            .context("Failed to wait for flatpak list")?;
        if !output.status.success() {
            anyhow::bail!(
                "Failed to run flatpak list: {}",
                String::from_utf8(output.stderr).context("Failed to parse stderr")?
            );
        }
        let output =
            String::from_utf8(output.stdout).context("Failed to parse flatpak list as UTF-8")?;

        parse_flatpak_output(&output, interner)
    }

    /// Flatpak uses the package ref (or partial ref, i.e. application ID) for installation
    fn transact(
        &self,
        install: &[&str],
        uninstall: &[&str],
        ask_confirmation: bool,
    ) -> Result<(), PackageManagerError> {
        if !install.is_empty() {
            package_manager_transaction(
                "flatpak",
                &["install"],
                install,
                (!ask_confirmation).then_some("--noninteractive"),
            )
            .context("Failed to install with flatpak")?;
        }
        if !uninstall.is_empty() {
            package_manager_transaction(
                "flatpak",
                &["uninstall"],
                uninstall,
                (!ask_confirmation).then_some("--noninteractive"),
            )
            .context("Failed to uninstall with flatpak")?;
        }
        Ok(())
    }

    fn mark(&self, _dependencies: &[&str], _manual: &[&str]) -> Result<(), PackageManagerError> {
        Err(PackageManagerError::UnsupportedOperation(
            "Marking packages as dependencies or manually installed is not supported by flatpak",
        ))
    }

    fn remove_unused(&self, ask_confirmation: bool) -> Result<(), PackageManagerError> {
        package_manager_transaction(
            "flatpak",
            &["uninstall", "--unused"],
            &[],
            (!ask_confirmation).then_some("--noninteractive"),
        )
        .context("Failed to remove unused packages with flatpak")?;
        Ok(())
    }
}

fn parse_flatpak_output(
    output: &str,
    interner: &paketkoll_types::intern::Interner,
) -> Result<Vec<PackageInterned>, anyhow::Error> {
    let mut packages = Vec::new();

    for line in output.lines() {
        let parts: SmallVec<[&str; 6]> = line.split('\t').collect();
        if parts.len() != 6 {
            anyhow::bail!("Unexpected number of columns in flatpak list: {}", line);
        }
        // Parse ref
        let (app_id, arch) = {
            let ref_parts: Vec<&str> = parts[0].split('/').collect();
            if ref_parts.len() != 3 {
                anyhow::bail!("Unexpected number of parts in flatpak ref: {}", parts[0]);
            }
            (ref_parts[0], ref_parts[1])
        };

        let version = parts[3];
        let desc = if parts[4].is_empty() {
            None
        } else {
            Some(parts[4].into())
        };

        let options = parts[5];
        let is_runtime = options.contains("runtime");

        // Build package struct
        let package = Package {
            name: PackageRef::get_or_intern(interner, parts[2]),
            version: version.into(),
            desc,
            architecture: Some(ArchitectureRef::get_or_intern(interner, arch)),
            depends: vec![],
            provides: vec![],
            reason: if is_runtime {
                // This is an approximation, flatpak doesn't appear to track
                // dependency vs explicit installs.
                Some(InstallReason::Dependency)
            } else {
                None
            },
            status: PackageInstallStatus::Installed,
            ids: smallvec::smallvec![
                // TODO: What other subsets of the ref is valid?
                PackageRef::get_or_intern(interner, app_id),
                PackageRef::get_or_intern(interner, parts[0])
            ],
        };
        packages.push(package);
    }
    Ok(packages)
}

#[cfg(test)]
mod tests {
    use Package;

    use super::*;

    use pretty_assertions::assert_eq;

    #[test]
    fn test_parse_flatpak_output() {
        let interner = paketkoll_types::intern::Interner::default();
        let output = indoc::indoc! {
            "com.github.tchx84.Flatseal/x86_64/stable	flathub	Flatseal	2.2.0	Manage Flatpak permissions	current
            org.fedoraproject.MediaWriter/x86_64/stable	flathub	Fedora Media Writer	5.1.1	A tool to create a live USB drive with an edition of Fedora	current
            org.freedesktop.Platform/x86_64/23.08	flathub	Freedesktop Platform	23.08.19	Runtime platform for applications	runtime
            org.freedesktop.Platform.GL.default/x86_64/23.08	flathub	Mesa	24.0.7	Mesa - The 3D Graphics Library	runtime
            org.freedesktop.Platform.GL.default/x86_64/23.08-extra	flathub	Mesa (Extra)	24.0.7	Mesa - The 3D Graphics Library	runtime
            org.freedesktop.Platform.GL.nvidia-550-78/x86_64/1.4	flathub	nvidia-550-78			runtime
            org.freedesktop.Platform.VAAPI.Intel/x86_64/23.08	flathub	Intel			runtime
            org.freedesktop.Platform.openh264/x86_64/2.2.0	flathub	openh264	2.1.0	OpenH264 Video Codec provided by Cisco Systems, Inc.	runtime
            org.freedesktop.Platform.openh264/x86_64/2.4.1	flathub	openh264	2.4.1	OpenH264 Video Codec provided by Cisco Systems, Inc.	runtime
            org.gnome.Platform/x86_64/46	flathub	GNOME Application Platform version 46		Shared libraries used by GNOME applications	runtime
            org.gtk.Gtk3theme.Adwaita-dark/x86_64/3.22	flathub	Adwaita dark GTK theme		Dark variant of the Adwaita GTK theme	runtime
            org.gtk.Gtk3theme.Breeze/x86_64/3.22	flathub	Breeze GTK theme	6.0.5	Breeze GTK theme matching the KDE Breeze theme	runtime
            org.kde.KStyle.Adwaita/x86_64/6.6	flathub	Adwaita theme		Adwaita widget theme matching the GNOME adwaita theme	runtime
            org.kde.Platform/x86_64/6.6	flathub	KDE Application Platform		Shared libraries used by KDE applications	runtime"
        };
        let packages = parse_flatpak_output(output, &interner).unwrap();
        assert_eq!(packages.len(), 14);
        assert_eq!(
            packages,
            vec![
                Package {
                    name: PackageRef::get_or_intern(&interner, "Flatseal"),
                    version: "2.2.0".into(),
                    desc: Some("Manage Flatpak permissions".into()),
                    architecture: Some(ArchitectureRef::get_or_intern(&interner, "x86_64")),
                    depends: vec![],
                    provides: vec![],
                    reason: None,
                    status: PackageInstallStatus::Installed,
                    ids: smallvec::smallvec![
                        PackageRef::get_or_intern(&interner, "com.github.tchx84.Flatseal"),
                        PackageRef::get_or_intern(
                            &interner,
                            "com.github.tchx84.Flatseal/x86_64/stable"
                        )
                    ],
                },
                Package {
                    name: PackageRef::get_or_intern(&interner, "Fedora Media Writer"),
                    version: "5.1.1".into(),
                    desc: Some(
                        "A tool to create a live USB drive with an edition of Fedora".into()
                    ),
                    architecture: Some(ArchitectureRef::get_or_intern(&interner, "x86_64")),
                    depends: vec![],
                    provides: vec![],
                    reason: None,
                    status: PackageInstallStatus::Installed,
                    ids: smallvec::smallvec![
                        PackageRef::get_or_intern(&interner, "org.fedoraproject.MediaWriter"),
                        PackageRef::get_or_intern(
                            &interner,
                            "org.fedoraproject.MediaWriter/x86_64/stable"
                        )
                    ],
                },
                Package {
                    name: PackageRef::get_or_intern(&interner, "Freedesktop Platform"),
                    version: "23.08.19".into(),
                    desc: Some("Runtime platform for applications".into()),
                    architecture: Some(ArchitectureRef::get_or_intern(&interner, "x86_64")),
                    depends: vec![],
                    provides: vec![],
                    reason: Some(InstallReason::Dependency),
                    status: PackageInstallStatus::Installed,
                    ids: smallvec::smallvec![
                        PackageRef::get_or_intern(&interner, "org.freedesktop.Platform"),
                        PackageRef::get_or_intern(
                            &interner,
                            "org.freedesktop.Platform/x86_64/23.08"
                        )
                    ],
                },
                Package {
                    name: PackageRef::get_or_intern(&interner, "Mesa"),
                    version: "24.0.7".into(),
                    desc: Some("Mesa - The 3D Graphics Library".into()),
                    architecture: Some(ArchitectureRef::get_or_intern(&interner, "x86_64")),
                    depends: vec![],
                    provides: vec![],
                    reason: Some(InstallReason::Dependency),
                    status: PackageInstallStatus::Installed,
                    ids: smallvec::smallvec![
                        PackageRef::get_or_intern(&interner, "org.freedesktop.Platform.GL.default"),
                        PackageRef::get_or_intern(
                            &interner,
                            "org.freedesktop.Platform.GL.default/x86_64/23.08"
                        )
                    ],
                },
                Package {
                    name: PackageRef::get_or_intern(&interner, "Mesa (Extra)"),
                    version: "24.0.7".into(),
                    desc: Some("Mesa - The 3D Graphics Library".into()),
                    architecture: Some(ArchitectureRef::get_or_intern(&interner, "x86_64")),
                    depends: vec![],
                    provides: vec![],
                    reason: Some(InstallReason::Dependency),
                    status: PackageInstallStatus::Installed,
                    ids: smallvec::smallvec![
                        PackageRef::get_or_intern(&interner, "org.freedesktop.Platform.GL.default"),
                        PackageRef::get_or_intern(
                            &interner,
                            "org.freedesktop.Platform.GL.default/x86_64/23.08-extra"
                        )
                    ],
                },
                Package {
                    name: PackageRef::get_or_intern(&interner, "nvidia-550-78"),
                    version: "".into(),
                    desc: None,
                    architecture: Some(ArchitectureRef::get_or_intern(&interner, "x86_64")),
                    depends: vec![],
                    provides: vec![],
                    reason: Some(InstallReason::Dependency),
                    status: PackageInstallStatus::Installed,
                    ids: smallvec::smallvec![
                        PackageRef::get_or_intern(
                            &interner,
                            "org.freedesktop.Platform.GL.nvidia-550-78"
                        ),
                        PackageRef::get_or_intern(
                            &interner,
                            "org.freedesktop.Platform.GL.nvidia-550-78/x86_64/1.4"
                        )
                    ],
                },
                Package {
                    name: PackageRef::get_or_intern(&interner, "Intel"),
                    version: "".into(),
                    desc: None,
                    architecture: Some(ArchitectureRef::get_or_intern(&interner, "x86_64")),
                    depends: vec![],
                    provides: vec![],
                    reason: Some(InstallReason::Dependency),
                    status: PackageInstallStatus::Installed,
                    ids: smallvec::smallvec![
                        PackageRef::get_or_intern(
                            &interner,
                            "org.freedesktop.Platform.VAAPI.Intel"
                        ),
                        PackageRef::get_or_intern(
                            &interner,
                            "org.freedesktop.Platform.VAAPI.Intel/x86_64/23.08"
                        )
                    ],
                },
                Package {
                    name: PackageRef::get_or_intern(&interner, "openh264"),
                    version: "2.1.0".into(),
                    desc: Some("OpenH264 Video Codec provided by Cisco Systems, Inc.".into()),
                    architecture: Some(ArchitectureRef::get_or_intern(&interner, "x86_64")),
                    depends: vec![],
                    provides: vec![],
                    reason: Some(InstallReason::Dependency),
                    status: PackageInstallStatus::Installed,
                    ids: smallvec::smallvec![
                        PackageRef::get_or_intern(&interner, "org.freedesktop.Platform.openh264"),
                        PackageRef::get_or_intern(
                            &interner,
                            "org.freedesktop.Platform.openh264/x86_64/2.2.0"
                        )
                    ],
                },
                Package {
                    name: PackageRef::get_or_intern(&interner, "openh264"),
                    version: "2.4.1".into(),
                    desc: Some("OpenH264 Video Codec provided by Cisco Systems, Inc.".into()),
                    architecture: Some(ArchitectureRef::get_or_intern(&interner, "x86_64")),
                    depends: vec![],
                    provides: vec![],
                    reason: Some(InstallReason::Dependency),
                    status: PackageInstallStatus::Installed,
                    ids: smallvec::smallvec![
                        PackageRef::get_or_intern(&interner, "org.freedesktop.Platform.openh264"),
                        PackageRef::get_or_intern(
                            &interner,
                            "org.freedesktop.Platform.openh264/x86_64/2.4.1"
                        )
                    ],
                },
                Package {
                    name: PackageRef::get_or_intern(
                        &interner,
                        "GNOME Application Platform version 46"
                    ),
                    version: "".into(),
                    desc: Some("Shared libraries used by GNOME applications".into()),
                    architecture: Some(ArchitectureRef::get_or_intern(&interner, "x86_64")),
                    depends: vec![],
                    provides: vec![],
                    reason: Some(InstallReason::Dependency),
                    status: PackageInstallStatus::Installed,
                    ids: smallvec::smallvec![
                        PackageRef::get_or_intern(&interner, "org.gnome.Platform"),
                        PackageRef::get_or_intern(&interner, "org.gnome.Platform/x86_64/46")
                    ],
                },
                Package {
                    name: PackageRef::get_or_intern(&interner, "Adwaita dark GTK theme"),
                    version: "".into(),
                    desc: Some("Dark variant of the Adwaita GTK theme".into()),
                    architecture: Some(ArchitectureRef::get_or_intern(&interner, "x86_64")),
                    depends: vec![],
                    provides: vec![],
                    reason: Some(InstallReason::Dependency),
                    status: PackageInstallStatus::Installed,
                    ids: smallvec::smallvec![
                        PackageRef::get_or_intern(&interner, "org.gtk.Gtk3theme.Adwaita-dark"),
                        PackageRef::get_or_intern(
                            &interner,
                            "org.gtk.Gtk3theme.Adwaita-dark/x86_64/3.22"
                        )
                    ],
                },
                Package {
                    name: PackageRef::get_or_intern(&interner, "Breeze GTK theme"),
                    version: "6.0.5".into(),
                    desc: Some("Breeze GTK theme matching the KDE Breeze theme".into()),
                    architecture: Some(ArchitectureRef::get_or_intern(&interner, "x86_64")),
                    depends: vec![],
                    provides: vec![],
                    reason: Some(InstallReason::Dependency),
                    status: PackageInstallStatus::Installed,
                    ids: smallvec::smallvec![
                        PackageRef::get_or_intern(&interner, "org.gtk.Gtk3theme.Breeze"),
                        PackageRef::get_or_intern(
                            &interner,
                            "org.gtk.Gtk3theme.Breeze/x86_64/3.22"
                        )
                    ],
                },
                Package {
                    name: PackageRef::get_or_intern(&interner, "Adwaita theme"),
                    version: "".into(),
                    desc: Some("Adwaita widget theme matching the GNOME adwaita theme".into()),
                    architecture: Some(ArchitectureRef::get_or_intern(&interner, "x86_64")),
                    depends: vec![],
                    provides: vec![],
                    reason: Some(InstallReason::Dependency),
                    status: PackageInstallStatus::Installed,
                    ids: smallvec::smallvec![
                        PackageRef::get_or_intern(&interner, "org.kde.KStyle.Adwaita"),
                        PackageRef::get_or_intern(&interner, "org.kde.KStyle.Adwaita/x86_64/6.6")
                    ],
                },
                Package {
                    name: PackageRef::get_or_intern(&interner, "KDE Application Platform"),
                    version: "".into(),
                    desc: Some("Shared libraries used by KDE applications".into()),
                    architecture: Some(ArchitectureRef::get_or_intern(&interner, "x86_64")),
                    depends: vec![],
                    provides: vec![],
                    reason: Some(InstallReason::Dependency),
                    status: PackageInstallStatus::Installed,
                    ids: smallvec::smallvec![
                        PackageRef::get_or_intern(&interner, "org.kde.Platform"),
                        PackageRef::get_or_intern(&interner, "org.kde.Platform/x86_64/6.6")
                    ],
                },
            ]
        );
    }
}
