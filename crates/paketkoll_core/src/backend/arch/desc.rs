//! Parse /var/lib/pacman/*/desc

// The format of this file is as follows:
// %NAME%
// package-name
//
// %VERSION%
// 1.2.3-4
//
// %BASE%
// base-package
// ...

use std::io::BufRead;

use crate::types::{ArchitectureRef, Dependency, InstallReason, PackageInstallStatus, PackageRef};
use compact_str::CompactString;
use paketkoll_types::intern::Interner;

impl crate::types::PackageInterned {
    pub(super) fn from_arch_linux_desc(
        mut readable: impl BufRead,
        interner: &Interner,
    ) -> anyhow::Result<Self> {
        let mut name: Option<PackageRef> = None;
        let mut arch: Option<ArchitectureRef> = None;
        let mut version: Option<CompactString> = None;
        let mut desc: Option<CompactString> = None;
        let mut depends: Vec<PackageRef> = Vec::new();
        let mut provides: Vec<PackageRef> = Vec::new();
        let mut reason: Option<InstallReason> = None;

        let mut line = String::new();
        while readable.read_line(&mut line)? > 0 {
            if line == "%NAME%\n" {
                line.clear();
                readable.read_line(&mut line)?;
                name = Some(PackageRef::get_or_intern(interner, line.trim_end()));
            } else if line == "%VERSION%\n" {
                line.clear();
                readable.read_line(&mut line)?;
                version = Some(line.trim_end().into());
            } else if line == "%ARCH%\n" {
                line.clear();
                readable.read_line(&mut line)?;
                arch = Some(ArchitectureRef::get_or_intern(interner, line.trim_end()));
            } else if line == "%DESC%\n" {
                line.clear();
                readable.read_line(&mut line)?;
                desc = Some(line.trim_end().into());
            } else if line == "%DEPENDS%\n" {
                parse_package_list(&mut readable, &mut depends, interner)?;
            } else if line == "%PROVIDES%\n" {
                parse_package_list(&mut readable, &mut provides, interner)?;
            } else if line == "%REASON%\n" {
                line.clear();
                readable.read_line(&mut line)?;
                // Reverse engineering note: 1 means dependency, not set means explicit
                reason = match line.trim_end() {
                    "1" => Some(InstallReason::Dependency),
                    _ => None,
                };
            }
            line.clear();
        }

        Ok(Self {
            name: name.ok_or_else(|| anyhow::anyhow!("No name"))?,
            architecture: arch,
            version: version.ok_or_else(|| anyhow::anyhow!("No version"))?,
            desc: Some(desc.ok_or_else(|| anyhow::anyhow!("No desc"))?),
            depends: depends.into_iter().map(Dependency::Single).collect(),
            provides,
            reason: Some(reason.unwrap_or(InstallReason::Explicit)),
            status: PackageInstallStatus::Installed,
            id: None,
        })
    }
}

/// Get the backup files list
pub(super) fn backup_files(mut readable: impl BufRead) -> Result<Vec<String>, anyhow::Error> {
    let mut backup_files = Vec::new();

    let mut line = String::new();
    while readable.read_line(&mut line)? > 0 {
        if line == "%BACKUP%\n" {
            parse_backup(&mut readable, &mut backup_files)?;
        }
        line.clear();
    }
    Ok(backup_files)
}

/// Parse a list of packages until we get a blank line
fn parse_package_list(
    readable: &mut impl BufRead,
    to_fill: &mut Vec<PackageRef>,
    interner: &Interner,
) -> Result<(), anyhow::Error> {
    let mut line = String::new();
    while readable.read_line(&mut line)? > 0 {
        let trimmed_line = line.trim_end();
        if trimmed_line.is_empty() {
            break;
        }
        let pkg = trimmed_line
            .split_once('=')
            .map(|(name, _)| name)
            .unwrap_or(trimmed_line);
        to_fill.push(PackageRef::get_or_intern(interner, pkg));
        line.clear();
    }
    Ok(())
}

/// Parse a list backup list
fn parse_backup(
    readable: &mut impl BufRead,
    to_fill: &mut Vec<String>,
) -> Result<(), anyhow::Error> {
    let mut line = String::new();
    while readable.read_line(&mut line)? > 0 {
        let trimmed_line = line.trim_end();
        if trimmed_line.is_empty() {
            break;
        }
        let filename = trimmed_line
            .split_once('\t')
            .map(|(name, _)| name.trim_end())
            .unwrap_or(trimmed_line);
        to_fill.push(filename.into());
        line.clear();
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use crate::types::Package;

    use super::*;
    use paketkoll_types::intern::Interner;
    use pretty_assertions::assert_eq;

    #[test]
    fn test_parse() {
        let input = indoc::indoc! {"
            %NAME%
            library-subpackage
            
            %VERSION%
            1.2.3-4
            
            %BASE%
            library-base
            
            %DESC%
            Some library
            
            %URL%
            https://example.com
            
            %ARCH%
            x86_64
            
            %BUILDDATE%
            1234567890
            
            %INSTALLDATE%
            9876543210
            
            %PACKAGER%
            Some dude <dude@example.com>
            
            %SIZE%
            123456
            
            %REASON%
            1
            
            %LICENSE%
            Apache
            
            %VALIDATION%
            pgp
            
            %DEPENDS%
            gcc-libs
            glibc
            somelib=1.2.3
            some-other-lib.so=4.5.6

            %PROVIDES%
            libfoo.so=1.2.3
            "};

        let interner = Interner::default();
        let desc = Package::from_arch_linux_desc(input.as_bytes(), &interner).unwrap();

        assert_eq!(
            desc,
            Package {
                name: PackageRef::get_or_intern(&interner, "library-subpackage"),
                version: "1.2.3-4".into(),
                desc: Some("Some library".into()),
                architecture: Some(ArchitectureRef::get_or_intern(&interner, "x86_64")),
                depends: vec![
                    Dependency::Single(PackageRef::get_or_intern(&interner, "gcc-libs")),
                    Dependency::Single(PackageRef::get_or_intern(&interner, "glibc")),
                    Dependency::Single(PackageRef::get_or_intern(&interner, "somelib")),
                    Dependency::Single(PackageRef::get_or_intern(&interner, "some-other-lib.so")),
                ],
                provides: vec![PackageRef::get_or_intern(&interner, "libfoo.so"),],
                reason: Some(InstallReason::Dependency),
                status: PackageInstallStatus::Installed,
                id: None,
            }
        );
    }

    #[test]
    fn test_backup_files() {
        let input = indoc::indoc! {"
            usr/share/doc/somefile
            usr/bin/some/other/file

            %BACKUP%
            etc/backup
            etc/backup2
            etc/backup3
            "};

        let backup_files = backup_files(input.as_bytes()).unwrap();

        assert_eq!(
            backup_files,
            vec![
                "etc/backup".to_string(),
                "etc/backup2".to_string(),
                "etc/backup3".to_string()
            ]
        );
    }
}
