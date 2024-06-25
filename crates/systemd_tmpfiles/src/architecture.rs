//! Systemd architecture mapping (based on systemd.unit(5))

use strum::IntoStaticStr;

/// Architectures as systemd understands them
#[derive(Debug, Clone, PartialEq, Eq, Hash, IntoStaticStr)]
pub(crate) enum Architecture {
    #[strum(serialize = "x86")]
    X86,
    #[strum(serialize = "x86-64")]
    X86_64,
    #[strum(serialize = "ppc")]
    Ppc,
    #[strum(serialize = "ppc-le")]
    PpcLE,
    #[strum(serialize = "ppc64")]
    Ppc64,
    #[strum(serialize = "ppc64-le")]
    Ppc64LE,
    #[strum(serialize = "ia64")]
    Ia64,
    #[strum(serialize = "parisc")]
    Parisc,
    #[strum(serialize = "parisc64")]
    Parisc64,
    #[strum(serialize = "s390")]
    S390,
    #[strum(serialize = "s390x")]
    S390x,
    #[strum(serialize = "sparc")]
    Sparc,
    #[strum(serialize = "sparc64")]
    Sparc64,
    #[strum(serialize = "mips")]
    Mips,
    #[strum(serialize = "mips64")]
    Mips64,
    #[strum(serialize = "alpha")]
    Alpha,
    #[strum(serialize = "arm")]
    Arm,
    #[strum(serialize = "arm-be")]
    ArmBe,
    #[strum(serialize = "arm64")]
    Arm64,
    #[strum(serialize = "arm64-be")]
    Arm64Be,
    #[strum(serialize = "sh")]
    Sh,
    #[strum(serialize = "sh64")]
    Sh64,
    #[strum(serialize = "mk68k")]
    M68k,
    #[strum(serialize = "tilegx")]
    Tilegx,
    #[strum(serialize = "cris")]
    Cris,
    #[strum(serialize = "arc")]
    Arc,
    #[strum(serialize = "arc-be")]
    ArcBe,
    #[strum(serialize = "riscv32")]
    Riscv32,
    #[strum(serialize = "riscv64")]
    Riscv64,
}

impl Architecture {
    pub(crate) fn from_uname(uname_arch: &str) -> Option<Self> {
        match uname_arch {
            // x86
            "i386" => Some(Self::X86),
            "i486" => Some(Self::X86),
            "i586" => Some(Self::X86),
            "i686" => Some(Self::X86),
            "x86_64" => Some(Self::X86_64),
            // ARM
            "aarch64" => Some(Self::Arm64),
            "aarch64_be" => Some(Self::Arm64Be),
            // PPC
            "ppc" => Some(Self::Ppc),
            "ppcle" => Some(Self::PpcLE),
            "ppc64" => Some(Self::Ppc64),
            "ppc64le" => Some(Self::Ppc64LE),
            // RISCV (not documented, but actually supported)
            "riscv32" => Some(Self::Riscv32),
            "riscv64" => Some(Self::Riscv64),
            // MIPS
            "mips" => Some(Self::Mips),
            "mips64" => Some(Self::Mips64),
            // SH
            "sh5" => Some(Self::Sh64),
            // s390
            "s390" => Some(Self::S390),
            "s390x" => Some(Self::S390x),
            // SPARC
            "sparc" => Some(Self::Sparc),
            "sparc64" => Some(Self::Sparc64),
            // Alpha
            "alpha" => Some(Self::Alpha),
            // IA64
            "ia64" => Some(Self::Ia64),
            // Parisc
            "parisc" => Some(Self::Parisc),
            "parisc64" => Some(Self::Parisc64),
            // M68k
            "m68k" => Some(Self::M68k),
            // TileGX
            "tilegx" => Some(Self::Tilegx),
            // CRIS
            "crisv32" => Some(Self::Cris),
            // ARC
            "arc" => Some(Self::Arc),
            "arceb" => Some(Self::ArcBe),
            _ => {
                if uname_arch.starts_with("arm") {
                    // Map 32-bit ARMs, too many variants to list explicitly
                    if uname_arch.ends_with('b') {
                        Some(Self::ArmBe)
                    } else {
                        Some(Self::Arm)
                    }
                } else if uname_arch.starts_with("sh") {
                    // Too many SH too
                    Some(Self::Sh)
                } else {
                    None
                }
            }
        }
    }
}
