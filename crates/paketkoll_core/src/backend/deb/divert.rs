//! Parser for dpkg-divert

use eyre::OptionExt;
use eyre::WrapErr;
use paketkoll_types::intern::Interner;
use paketkoll_types::intern::PackageRef;
use std::collections::BTreeMap;
use std::io::BufRead;
use std::path::PathBuf;

/// Describes a diversion by dpkg-divert
///
/// The path will be diverted for all other packages than the one listed
#[derive(Debug, PartialEq, Eq, Hash, Clone)]
pub(super) struct Diversion {
    pub by_package: PackageRef,
    pub new_path: PathBuf,
}

/// Mapping from old path to new path and which package
pub(super) type Diversions = BTreeMap<PathBuf, Diversion>;

/// Get all diversions from dpkg-divert --list
pub(super) fn get_diversions(interner: &Interner) -> eyre::Result<Diversions> {
    let mut cmd = std::process::Command::new("dpkg-divert");
    cmd.arg("--list");
    let output = cmd.output().wrap_err("Failed to run dpkg-divert")?;

    parse_diversions(std::io::Cursor::new(output.stdout), interner)
}

/// Parse output from dpkg-divert --list
fn parse_diversions(mut input: impl BufRead, interner: &Interner) -> eyre::Result<Diversions> {
    let mut results = Diversions::new();

    let re = regex::Regex::new(r"^diversion of (?<orig>.+) to (?<new>.+) by (?<pkg>.+)$")?;

    let mut line = String::new();
    while input.read_line(&mut line)? > 0 {
        let trimmed = line.trim_end();
        let captures = re.captures(trimmed);
        if let Some(captures) = captures {
            let orig_path: PathBuf = captures
                .name("orig")
                .ok_or_eyre("Failed to extract orig path")?
                .as_str()
                .into();
            let new_path = captures
                .name("new")
                .ok_or_eyre("Failed to extract new path")?
                .as_str()
                .into();
            let by_package = PackageRef::get_or_intern(
                interner,
                captures
                    .name("pkg")
                    .ok_or_eyre("Failed to extract package")?
                    .as_str(),
            );

            let had_entry = results
                .insert(
                    orig_path.clone(),
                    Diversion {
                        by_package,
                        new_path,
                    },
                )
                .is_some();
            if had_entry {
                return Err(eyre::eyre!(
                    "Duplicate diversion for path {:?}. Don't know how to handle",
                    orig_path
                ));
            }
        }

        line.clear();
    }

    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::Diversion;
    use super::Diversions;
    use super::parse_diversions;
    use paketkoll_types::intern::Interner;
    use paketkoll_types::intern::PackageRef;
    use pretty_assertions::assert_eq;

    #[test]
    fn test_parse_diversions() {
        // Actual data from a raspberry pi
        let input = indoc::indoc! {r"
            diversion of /usr/lib/python3.11/EXTERNALLY-MANAGED to /usr/lib/python3.11/EXTERNALLY-MANAGED.orig by raspberrypi-sys-mods
            diversion of /usr/share/man/man1/parallel.1.gz to /usr/share/man/man1/parallel.moreutils.1.gz by parallel
            diversion of /usr/share/man/man1/sh.1.gz to /usr/share/man/man1/sh.distrib.1.gz by dash
            diversion of /usr/bin/parallel to /usr/bin/parallel.moreutils by parallel
            diversion of /bin/sh to /bin/sh.distrib by dash
            "};

        let interner = Interner::new();
        let parsed = parse_diversions(input.as_bytes(), &interner).unwrap();

        let expected = Diversions::from_iter(vec![
            (
                "/usr/lib/python3.11/EXTERNALLY-MANAGED".into(),
                Diversion {
                    new_path: "/usr/lib/python3.11/EXTERNALLY-MANAGED.orig".into(),
                    by_package: PackageRef::get_or_intern(&interner, "raspberrypi-sys-mods"),
                },
            ),
            (
                "/usr/share/man/man1/parallel.1.gz".into(),
                Diversion {
                    new_path: "/usr/share/man/man1/parallel.moreutils.1.gz".into(),
                    by_package: PackageRef::get_or_intern(&interner, "parallel"),
                },
            ),
            (
                "/usr/share/man/man1/sh.1.gz".into(),
                Diversion {
                    new_path: "/usr/share/man/man1/sh.distrib.1.gz".into(),
                    by_package: PackageRef::get_or_intern(&interner, "dash"),
                },
            ),
            (
                "/usr/bin/parallel".into(),
                Diversion {
                    new_path: "/usr/bin/parallel.moreutils".into(),
                    by_package: PackageRef::get_or_intern(&interner, "parallel"),
                },
            ),
            (
                "/bin/sh".into(),
                Diversion {
                    new_path: "/bin/sh.distrib".into(),
                    by_package: PackageRef::get_or_intern(&interner, "dash"),
                },
            ),
        ]);

        assert_eq!(parsed, expected);
    }
}
