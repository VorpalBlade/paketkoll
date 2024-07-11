//! Package backend for flatpak

use std::process::{Command, Stdio};

use anyhow::Context;
use paketkoll_types::{
    intern::{ArchitectureRef, PackageRef},
    package::{Package, PackageInstallStatus, PackageInterned},
};

use crate::utils::package_manager_transaction;

use super::{Name, PackageManager, Packages};

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

    fn as_backend_enum(&self) -> paketkoll_types::Backend {
        paketkoll_types::Backend::Flatpak
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
}

fn parse_flatpak_output(
    output: &str,
    interner: &paketkoll_types::intern::Interner,
) -> Result<Vec<PackageInterned>, anyhow::Error> {
    let mut packages = Vec::new();

    for line in output.lines() {
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() != 6 {
            anyhow::bail!("Unexpected number of columns in flatpak list: {}", line);
        }
        // Parse ref
        let arch = {
            let ref_parts: Vec<&str> = parts[0].split('/').collect();
            if ref_parts.len() != 3 {
                anyhow::bail!("Unexpected number of parts in flatpak ref: {}", parts[0]);
            }
            ref_parts[1]
        };

        let version = parts[3];
        let desc = if parts[4].is_empty() {
            None
        } else {
            Some(parts[4].into())
        };

        // Build package struct
        let package = Package {
            name: PackageRef::get_or_intern(interner, parts[2]),
            version: version.into(),
            desc,
            architecture: Some(ArchitectureRef::get_or_intern(interner, arch)),
            depends: vec![],
            provides: vec![],
            reason: None,
            status: PackageInstallStatus::Installed,
            id: Some(parts[0].into()),
        };
        packages.push(package);
    }
    Ok(packages)
}

impl PackageManager for Flatpak {
    /// Flatpak uses the package ref (or partial ref, i.e. application ID) for installation
    fn transact(
        &self,
        install: &[compact_str::CompactString],
        uninstall: &[compact_str::CompactString],
        ask_confirmation: bool,
    ) -> anyhow::Result<()> {
        if !install.is_empty() {
            package_manager_transaction(
                "flatpak",
                "install",
                install,
                ask_confirmation.then_some("--noninteractive"),
            )
            .context("Failed to install with flatpak")?;
        }
        if !uninstall.is_empty() {
            package_manager_transaction(
                "flatpak",
                "uninstall",
                uninstall,
                ask_confirmation.then_some("--noninteractive"),
            )
            .context("Failed to uninstall with flatpak")?;
        }
        Ok(())
    }
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
                    id: Some("com.github.tchx84.Flatseal/x86_64/stable".into()),
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
                    id: Some("org.fedoraproject.MediaWriter/x86_64/stable".into()),
                },
                Package {
                    name: PackageRef::get_or_intern(&interner, "Freedesktop Platform"),
                    version: "23.08.19".into(),
                    desc: Some("Runtime platform for applications".into()),
                    architecture: Some(ArchitectureRef::get_or_intern(&interner, "x86_64")),
                    depends: vec![],
                    provides: vec![],
                    reason: None,
                    status: PackageInstallStatus::Installed,
                    id: Some("org.freedesktop.Platform/x86_64/23.08".into()),
                },
                Package {
                    name: PackageRef::get_or_intern(&interner, "Mesa"),
                    version: "24.0.7".into(),
                    desc: Some("Mesa - The 3D Graphics Library".into()),
                    architecture: Some(ArchitectureRef::get_or_intern(&interner, "x86_64")),
                    depends: vec![],
                    provides: vec![],
                    reason: None,
                    status: PackageInstallStatus::Installed,
                    id: Some("org.freedesktop.Platform.GL.default/x86_64/23.08".into()),
                },
                Package {
                    name: PackageRef::get_or_intern(&interner, "Mesa (Extra)"),
                    version: "24.0.7".into(),
                    desc: Some("Mesa - The 3D Graphics Library".into()),
                    architecture: Some(ArchitectureRef::get_or_intern(&interner, "x86_64")),
                    depends: vec![],
                    provides: vec![],
                    reason: None,
                    status: PackageInstallStatus::Installed,
                    id: Some("org.freedesktop.Platform.GL.default/x86_64/23.08-extra".into()),
                },
                Package {
                    name: PackageRef::get_or_intern(&interner, "nvidia-550-78"),
                    version: "".into(),
                    desc: None,
                    architecture: Some(ArchitectureRef::get_or_intern(&interner, "x86_64")),
                    depends: vec![],
                    provides: vec![],
                    reason: None,
                    status: PackageInstallStatus::Installed,
                    id: Some("org.freedesktop.Platform.GL.nvidia-550-78/x86_64/1.4".into()),
                },
                Package {
                    name: PackageRef::get_or_intern(&interner, "Intel"),
                    version: "".into(),
                    desc: None,
                    architecture: Some(ArchitectureRef::get_or_intern(&interner, "x86_64")),
                    depends: vec![],
                    provides: vec![],
                    reason: None,
                    status: PackageInstallStatus::Installed,
                    id: Some("org.freedesktop.Platform.VAAPI.Intel/x86_64/23.08".into()),
                },
                Package {
                    name: PackageRef::get_or_intern(&interner, "openh264"),
                    version: "2.1.0".into(),
                    desc: Some("OpenH264 Video Codec provided by Cisco Systems, Inc.".into()),
                    architecture: Some(ArchitectureRef::get_or_intern(&interner, "x86_64")),
                    depends: vec![],
                    provides: vec![],
                    reason: None,
                    status: PackageInstallStatus::Installed,
                    id: Some("org.freedesktop.Platform.openh264/x86_64/2.2.0".into()),
                },
                Package {
                    name: PackageRef::get_or_intern(&interner, "openh264"),
                    version: "2.4.1".into(),
                    desc: Some("OpenH264 Video Codec provided by Cisco Systems, Inc.".into()),
                    architecture: Some(ArchitectureRef::get_or_intern(&interner, "x86_64")),
                    depends: vec![],
                    provides: vec![],
                    reason: None,
                    status: PackageInstallStatus::Installed,
                    id: Some("org.freedesktop.Platform.openh264/x86_64/2.4.1".into()),
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
                    reason: None,
                    status: PackageInstallStatus::Installed,
                    id: Some("org.gnome.Platform/x86_64/46".into()),
                },
                Package {
                    name: PackageRef::get_or_intern(&interner, "Adwaita dark GTK theme"),
                    version: "".into(),
                    desc: Some("Dark variant of the Adwaita GTK theme".into()),
                    architecture: Some(ArchitectureRef::get_or_intern(&interner, "x86_64")),
                    depends: vec![],
                    provides: vec![],
                    reason: None,
                    status: PackageInstallStatus::Installed,
                    id: Some("org.gtk.Gtk3theme.Adwaita-dark/x86_64/3.22".into()),
                },
                Package {
                    name: PackageRef::get_or_intern(&interner, "Breeze GTK theme"),
                    version: "6.0.5".into(),
                    desc: Some("Breeze GTK theme matching the KDE Breeze theme".into()),
                    architecture: Some(ArchitectureRef::get_or_intern(&interner, "x86_64")),
                    depends: vec![],
                    provides: vec![],
                    reason: None,
                    status: PackageInstallStatus::Installed,
                    id: Some("org.gtk.Gtk3theme.Breeze/x86_64/3.22".into()),
                },
                Package {
                    name: PackageRef::get_or_intern(&interner, "Adwaita theme"),
                    version: "".into(),
                    desc: Some("Adwaita widget theme matching the GNOME adwaita theme".into()),
                    architecture: Some(ArchitectureRef::get_or_intern(&interner, "x86_64")),
                    depends: vec![],
                    provides: vec![],
                    reason: None,
                    status: PackageInstallStatus::Installed,
                    id: Some("org.kde.KStyle.Adwaita/x86_64/6.6".into()),
                },
                Package {
                    name: PackageRef::get_or_intern(&interner, "KDE Application Platform"),
                    version: "".into(),
                    desc: Some("Shared libraries used by KDE applications".into()),
                    architecture: Some(ArchitectureRef::get_or_intern(&interner, "x86_64")),
                    depends: vec![],
                    provides: vec![],
                    reason: None,
                    status: PackageInstallStatus::Installed,
                    id: Some("org.kde.Platform/x86_64/6.6".into()),
                },
            ]
        );
    }
}
