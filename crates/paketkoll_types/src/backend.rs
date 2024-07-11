//! Declaration of backends

/// Which backend to use for the system package manager
#[derive(
    Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Clone, Copy, strum::Display, strum::EnumString,
)]
pub enum Backend {
    /// Backend for Arch Linux and derived distros (pacman)
    #[strum(to_string = "pacman")]
    Pacman,
    /// Backend for Debian and derived distros (dpkg/apt)
    #[strum(to_string = "apt")]
    Apt,
    /// Backend for flatpak (package list only)
    #[strum(to_string = "flatpak")]
    Flatpak,
    /// Backend for systemd-tmpfiles (file list only)
    #[strum(to_string = "systemd-tmpfiles")]
    SystemdTmpfiles,
}
