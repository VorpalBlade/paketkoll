//! Handles resolving specifiers

use std::borrow::Cow;
use std::collections::HashMap;

use compact_str::CompactString;
use thiserror::Error;

#[cfg(feature = "user")]
pub use user::UserResolver;

use crate::architecture::Architecture;

mod private {
    pub trait Sealed {}
}

/// Trait that allows resolving specifiers
pub trait Resolve: private::Sealed {
    /// Resolve specifiers in the input string
    fn resolve<'input>(&self, input: &'input str) -> Result<Cow<'input, str>, SpecifierError>;
}

/// Specifiers that don't change between user and host mode
trait InvariantProvider: private::Sealed {
    /// %a
    fn architecture(&self) -> Option<&str>;
    /// %A
    fn os_image_version(&self) -> &str;
    /// %b
    fn boot_id(&self) -> &str;
    /// %B
    fn os_build_id(&self) -> &str;

    /// %H
    fn host_name(&self) -> &str;
    /// %l
    fn short_host_name(&self) -> &str;

    /// %m
    fn machine_id(&self) -> &str;
    /// %M
    fn os_image_id(&self) -> &str;
    /// %o
    fn os_id(&self) -> &str;

    /// %T (typically /tmp)
    fn temp_directory(&self) -> &str;
    /// %v
    fn kernel_release(&self) -> &str;
    /// %V (typically /var/tmp)
    fn persistent_temp_directory(&self) -> &str;

    /// %w
    fn os_version_id(&self) -> &str;
    /// %W
    fn os_variant_id(&self) -> &str;
}

/// Specifiers that change between host and user mode
trait VariantProvider: private::Sealed {
    /// %C
    fn cache_directory(&self) -> &str;
    /// %g
    fn user_group_name(&self) -> &str;
    /// %G
    fn user_gid(&self) -> u32;
    /// %h
    fn user_home_directory(&self) -> &str;
    /// %L
    fn log_directory(&self) -> &str;
    /// %S
    fn state_directory(&self) -> &str;
    /// %t
    fn runtime_directory(&self) -> &str;
    /// %u
    fn user_name(&self) -> &str;
    /// %U
    fn user_uid(&self) -> u32;
}

/// Type of error when constructing providers
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ProviderError {
    #[error("Failed to read {0}: {1}")]
    ReadFile(&'static str, #[source] std::io::Error),
    #[error("Failed to load user data: {0}")]
    UserError(#[from] nix::errno::Errno),
    #[error("User missing")]
    UserMissingError,
    #[error("Failed to resolve directory: {0}")]
    DirectoryError(&'static str),
}

/// Resolver for system instance of systemd-tmpfiles
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SystemResolver {
    architecture: Option<Architecture>,
    os_image_version: CompactString,
    boot_id: CompactString,
    os_build_id: CompactString,
    host_name: CompactString,
    machine_id: CompactString,
    os_image_id: CompactString,
    os_id: CompactString,
    kernel_release: CompactString,
    os_version_id: CompactString,
    os_variant_id: CompactString,
    temp_directory: CompactString,
    persistent_temp_directory: CompactString,
}

impl SystemResolver {
    /// Create a new instance based on the currently running system
    pub fn new_from_running_system() -> Result<Self, ProviderError> {
        // Load and parse /etc/os-release
        let os_release = std::fs::read_to_string("/etc/os-release")
            .map_err(|e| ProviderError::ReadFile("/etc/os-release", e))?;
        let os_release = parse_os_release(&os_release);

        // Get uname
        let utsname = nix::sys::utsname::uname().expect("Cannot error");

        // Get machine ID
        let machine_id = std::fs::read_to_string("/etc/machine-id")
            .map_err(|e| ProviderError::ReadFile("/etc/machine-id", e))?;

        // Get boot ID
        // TODO: Do we need to reformat this from UUID to some other format?
        let boot_id = std::fs::read_to_string("/proc/sys/kernel/random/boot_id")
            .map_err(|e| ProviderError::ReadFile("/proc/sys/kernel/random/boot_id", e))?;

        // Get temp dirs:
        let tmp_dir = {
            let mut tmp_dir = None;
            for env_var in ["TMPDIR", "TMP", "TEMP"] {
                if let Ok(dir) = std::env::var(env_var) {
                    tmp_dir = dir.into();
                    break;
                }
            }
            tmp_dir
        };
        let tmp_dir = tmp_dir.as_deref();

        Ok(Self {
            architecture: Architecture::from_uname(utsname.machine().to_string_lossy().as_ref()),
            os_image_version: os_release
                .get("IMAGE_VERSION")
                .copied()
                .unwrap_or("")
                .into(),
            boot_id: boot_id.into(),
            os_build_id: os_release.get("BUILD_ID").copied().unwrap_or("").into(),
            host_name: utsname.nodename().to_string_lossy().into(),
            machine_id: machine_id.into(),
            os_image_id: os_release.get("IMAGE_ID").copied().unwrap_or("").into(),
            os_id: os_release.get("ID").copied().unwrap_or("").into(),
            kernel_release: utsname.release().to_string_lossy().into(),
            os_version_id: os_release.get("VERSION_ID").copied().unwrap_or("").into(),
            os_variant_id: os_release.get("VARIANT_ID").copied().unwrap_or("").into(),
            temp_directory: tmp_dir.unwrap_or("/tmp").into(),
            persistent_temp_directory: tmp_dir.unwrap_or("/var/tmp").into(),
        })
    }
}

impl Resolve for SystemResolver {
    #[inline]
    fn resolve<'input>(&self, input: &'input str) -> Result<Cow<'input, str>, SpecifierError> {
        apply_specifiers(input, self)
    }
}

impl private::Sealed for SystemResolver {}

impl InvariantProvider for SystemResolver {
    fn architecture(&self) -> Option<&str> {
        self.architecture.as_ref().map(Into::into)
    }

    fn os_image_version(&self) -> &str {
        &self.os_image_version
    }

    fn boot_id(&self) -> &str {
        &self.boot_id
    }

    fn os_build_id(&self) -> &str {
        &self.os_build_id
    }

    fn host_name(&self) -> &str {
        &self.host_name
    }

    fn short_host_name(&self) -> &str {
        self.host_name
            .split_once('.')
            .map(|(name, _)| name)
            .unwrap_or(&self.host_name)
    }

    fn machine_id(&self) -> &str {
        &self.machine_id
    }

    fn os_image_id(&self) -> &str {
        &self.os_image_id
    }

    fn os_id(&self) -> &str {
        &self.os_id
    }

    fn temp_directory(&self) -> &str {
        &self.temp_directory
    }

    fn kernel_release(&self) -> &str {
        &self.kernel_release
    }

    fn persistent_temp_directory(&self) -> &str {
        &self.persistent_temp_directory
    }

    fn os_version_id(&self) -> &str {
        &self.os_version_id
    }

    fn os_variant_id(&self) -> &str {
        &self.os_variant_id
    }
}

impl VariantProvider for SystemResolver {
    fn cache_directory(&self) -> &str {
        "/var/cache"
    }

    fn user_group_name(&self) -> &str {
        "root"
    }

    fn user_gid(&self) -> u32 {
        0
    }

    fn user_home_directory(&self) -> &str {
        "/root"
    }

    fn log_directory(&self) -> &str {
        "/var/log"
    }

    fn state_directory(&self) -> &str {
        "/var/lib"
    }

    fn runtime_directory(&self) -> &str {
        "/run"
    }

    fn user_name(&self) -> &str {
        "root"
    }

    fn user_uid(&self) -> u32 {
        0
    }
}

fn parse_os_release(buffer: &str) -> HashMap<&str, &str> {
    buffer
        .lines()
        .filter_map(|line| {
            let (key, value) = line.split_once('=')?;
            let value = value.trim_matches('"');
            Some((key, value))
        })
        .collect()
}

#[cfg(feature = "user")]
mod user {
    use compact_str::CompactString;

    use super::private;
    use super::InvariantProvider;
    use super::ProviderError;
    use super::Resolve;
    use super::SystemResolver;
    use super::VariantProvider;

    /// Resolver for user instance of systemd-tmpfiles
    #[derive(Debug, Clone, PartialEq, Eq, Hash)]
    pub struct UserResolver {
        system: SystemResolver,
        user_name: CompactString,
        uid: u32,
        group_name: CompactString,
        gid: u32,
        home_directory: CompactString,
        // Computed from state directory
        log_directory: CompactString,
        cache_directory: CompactString,
        state_directory: CompactString,
        runtime_directory: CompactString,
    }

    impl UserResolver {
        /// Create a new instance from the current user and a system
        /// provider (for values that don't change between users)
        pub fn new_from_current_user(
            system_provider: SystemResolver,
        ) -> Result<Self, ProviderError> {
            let uid = nix::unistd::Uid::current();
            let user = nix::unistd::User::from_uid(uid)
                .map_err(ProviderError::UserError)?
                .ok_or_else(|| ProviderError::UserMissingError)?;
            let group = nix::unistd::Group::from_gid(user.gid)
                .map_err(ProviderError::UserError)?
                .ok_or_else(|| ProviderError::UserMissingError)?;
            let home_dir = user.dir.as_os_str().to_string_lossy();
            let state_dir = dirs::state_dir()
                .ok_or_else(|| ProviderError::DirectoryError("$XDG_STATE_HOME"))?;
            let state_dir = state_dir.to_string_lossy();
            Ok(Self {
                system: system_provider,
                user_name: user.name.into(),
                uid: uid.as_raw(),
                group_name: group.name.into(),
                gid: user.gid.as_raw(),
                home_directory: home_dir.clone().into(),
                log_directory: format!("{state_dir}/log").into(),
                cache_directory: dirs::cache_dir()
                    .ok_or_else(|| ProviderError::DirectoryError("$XDG_CACHE_HOME"))?
                    .to_string_lossy()
                    .into(),
                state_directory: state_dir.into(),
                runtime_directory: dirs::runtime_dir()
                    .ok_or_else(|| ProviderError::DirectoryError("$XDG_RUNTIME_DIR"))?
                    .to_string_lossy()
                    .into(),
            })
        }
    }

    impl Resolve for UserResolver {
        #[inline]
        fn resolve<'input>(
            &self,
            input: &'input str,
        ) -> Result<std::borrow::Cow<'input, str>, super::SpecifierError> {
            super::apply_specifiers(input, self)
        }
    }

    impl private::Sealed for UserResolver {}

    /// Forward to the system provider for the invariant case
    impl InvariantProvider for UserResolver {
        fn architecture(&self) -> Option<&str> {
            self.system.architecture()
        }

        fn os_image_version(&self) -> &str {
            self.system.os_image_version()
        }

        fn boot_id(&self) -> &str {
            self.system.boot_id()
        }

        fn os_build_id(&self) -> &str {
            self.system.os_build_id()
        }

        fn host_name(&self) -> &str {
            self.system.host_name()
        }

        fn short_host_name(&self) -> &str {
            self.system.short_host_name()
        }

        fn machine_id(&self) -> &str {
            self.system.machine_id()
        }

        fn os_image_id(&self) -> &str {
            self.system.os_image_id()
        }

        fn os_id(&self) -> &str {
            self.system.os_id()
        }

        fn temp_directory(&self) -> &str {
            self.system.temp_directory()
        }

        fn kernel_release(&self) -> &str {
            self.system.kernel_release()
        }

        fn persistent_temp_directory(&self) -> &str {
            self.system.persistent_temp_directory()
        }

        fn os_version_id(&self) -> &str {
            self.system.os_version_id()
        }

        fn os_variant_id(&self) -> &str {
            self.system.os_variant_id()
        }
    }

    impl VariantProvider for UserResolver {
        fn cache_directory(&self) -> &str {
            &self.cache_directory
        }

        fn user_group_name(&self) -> &str {
            &self.group_name
        }

        fn user_gid(&self) -> u32 {
            self.gid
        }

        fn user_home_directory(&self) -> &str {
            &self.home_directory
        }

        fn log_directory(&self) -> &str {
            &self.log_directory
        }

        fn state_directory(&self) -> &str {
            &self.state_directory
        }

        fn runtime_directory(&self) -> &str {
            &self.runtime_directory
        }

        fn user_name(&self) -> &str {
            &self.user_name
        }

        fn user_uid(&self) -> u32 {
            self.uid
        }
    }
}

/// Type of error when resolving specifiers
#[derive(Debug, Error)]
pub enum SpecifierError {
    #[error("Invalid specifier: {0}")]
    InvalidSpecifier(char),
    #[error("Trailing specifier")]
    TrailingSpecifier,
    #[error("Unknown value for specifier: {0}")]
    UnknownValue(char),
}

/// Apply specifiers to a string according to systemd-tmpfiles rules
fn apply_specifiers<'input>(
    input: &'input str,
    provider: &(impl VariantProvider + InvariantProvider),
) -> Result<Cow<'input, str>, SpecifierError> {
    let index = memchr::memchr(b'%', input.as_bytes());
    let Some(mut index) = index else {
        return Ok(Cow::Borrowed(input));
    };
    let mut old_idx = 0;
    // Guess that we need some capacity
    let mut buffer = Vec::with_capacity(input.len() + 32);

    loop {
        buffer.extend(input[old_idx..index].as_bytes());
        old_idx = index + 2;
        let next_char = *input
            .as_bytes()
            .get(index + 1)
            .ok_or(SpecifierError::TrailingSpecifier)?;
        match next_char {
            b'a' => {
                if let Some(arch) = provider.architecture() {
                    buffer.extend(arch.as_bytes());
                } else {
                    return Err(SpecifierError::UnknownValue('a'));
                }
            }
            b'A' => buffer.extend(provider.os_image_version().as_bytes()),
            b'b' => buffer.extend(provider.boot_id().as_bytes()),
            b'B' => buffer.extend(provider.os_build_id().as_bytes()),
            b'C' => buffer.extend(provider.cache_directory().as_bytes()),
            b'g' => buffer.extend(provider.user_group_name().as_bytes()),
            b'G' => buffer.extend(provider.user_gid().to_string().as_bytes()),
            b'h' => buffer.extend(provider.user_home_directory().as_bytes()),
            b'H' => buffer.extend(provider.host_name().as_bytes()),
            b'l' => buffer.extend(provider.short_host_name().as_bytes()),
            b'L' => buffer.extend(provider.log_directory().as_bytes()),
            b'm' => buffer.extend(provider.machine_id().as_bytes()),
            b'M' => buffer.extend(provider.os_image_id().as_bytes()),
            b'o' => buffer.extend(provider.os_id().as_bytes()),
            b'S' => buffer.extend(provider.state_directory().as_bytes()),
            b't' => buffer.extend(provider.runtime_directory().as_bytes()),
            b'T' => buffer.extend(provider.temp_directory().as_bytes()),
            b'u' => buffer.extend(provider.user_name().as_bytes()),
            b'U' => buffer.extend(provider.user_uid().to_string().as_bytes()),
            b'v' => buffer.extend(provider.kernel_release().as_bytes()),
            b'V' => buffer.extend(provider.persistent_temp_directory().as_bytes()),
            b'w' => buffer.extend(provider.os_version_id().as_bytes()),
            b'W' => buffer.extend(provider.os_variant_id().as_bytes()),
            b'%' => buffer.push(b'%'),
            _ => return Err(SpecifierError::InvalidSpecifier(next_char.into())),
        }
        let new_index = memchr::memchr(b'%', input[old_idx..].as_bytes());
        if let Some(new_index) = new_index {
            index = old_idx + new_index;
        } else {
            buffer.extend(input[old_idx..].as_bytes());
            return Ok(Cow::Owned(
                String::from_utf8(buffer).expect("Invalid UTF-8"),
            ));
        }
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn test_apply_specifiers() {
        let provider = super::SystemResolver {
            architecture: Some(super::Architecture::X86_64),
            os_image_version: "1.0".into(),
            boot_id: "1234".into(),
            os_build_id: "rolling".into(),
            host_name: "localhost".into(),
            machine_id: "1234".into(),
            os_image_id: "arch".into(),
            os_id: "arch".into(),
            kernel_release: "6.9.3-zen1-1-zen".into(),
            os_version_id: "1.0".into(),
            os_variant_id: "rolling".into(),
            temp_directory: "/tmp".into(),
            persistent_temp_directory: "/var/tmp".into(),
        };
        assert_eq!(
            super::apply_specifiers("Text %u | %% | %H!", &provider).unwrap(),
            "Text root | % | localhost!"
        );
        assert_eq!(super::apply_specifiers("", &provider).unwrap(), "");
        assert_eq!(super::apply_specifiers("%%%%", &provider).unwrap(), "%%");
        assert_eq!(super::apply_specifiers("aa", &provider).unwrap(), "aa");
        assert_eq!(
            super::apply_specifiers("ö%uö", &provider).unwrap(),
            "örootö"
        );
        assert_eq!(
            super::apply_specifiers("Text %a | %A | %b | %B | %C | %g | %G | %h | %H | %l | %L | %m | %M | %o | %S | %t | %T | %u | %U | %v | %V | %w | %W | %%", &provider).unwrap(),
            "Text x86-64 | 1.0 | 1234 | rolling | /var/cache | root | 0 | /root | localhost | localhost | /var/log | 1234 | arch | arch | /var/lib | /run | /tmp | root | 0 | 6.9.3-zen1-1-zen | /var/tmp | 1.0 | rolling | %"
        );

        // Test error cases
        assert!(super::apply_specifiers("%", &provider).is_err());
        assert!(super::apply_specifiers("%%%", &provider).is_err());
        assert!(super::apply_specifiers("%u%", &provider).is_err());
        assert!(super::apply_specifiers("%z", &provider).is_err());
        assert!(super::apply_specifiers("%ö", &provider).is_err());
    }

    #[test]
    fn test_parse_os_release() {
        let buffer = indoc::indoc! {r#"
            NAME="Arch Linux"
            PRETTY_NAME="Arch Linux"
            ID=arch
            BUILD_ID=rolling
            ANSI_COLOR="38;2;23;147;209"
            HOME_URL="https://archlinux.org/"
            DOCUMENTATION_URL="https://wiki.archlinux.org/"
            SUPPORT_URL="https://bbs.archlinux.org/"
            BUG_REPORT_URL="https://gitlab.archlinux.org/groups/archlinux/-/issues"
            PRIVACY_POLICY_URL="https://terms.archlinux.org/docs/privacy-policy/"
            LOGO=archlinux-logo
            "#
        };

        let os_release = super::parse_os_release(buffer);
        assert_eq!(os_release["NAME"], "Arch Linux");
        assert_eq!(os_release["PRETTY_NAME"], "Arch Linux");
        assert_eq!(os_release["ID"], "arch");
        assert_eq!(os_release["BUILD_ID"], "rolling");
        assert_eq!(os_release["ANSI_COLOR"], "38;2;23;147;209");
        assert_eq!(os_release["HOME_URL"], "https://archlinux.org/");
        assert_eq!(
            os_release["DOCUMENTATION_URL"],
            "https://wiki.archlinux.org/"
        );
        assert_eq!(os_release["SUPPORT_URL"], "https://bbs.archlinux.org/");
        assert_eq!(
            os_release["BUG_REPORT_URL"],
            "https://gitlab.archlinux.org/groups/archlinux/-/issues"
        );
        assert_eq!(
            os_release["PRIVACY_POLICY_URL"],
            "https://terms.archlinux.org/docs/privacy-policy/"
        );
        assert_eq!(os_release["LOGO"], "archlinux-logo");
    }
}
