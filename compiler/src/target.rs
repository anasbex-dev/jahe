/// Compiler target information used for target-dependent scalar types.
/// The bootstrap compiler currently supports only Windows x86_64 natively.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TargetSpec {
    pub triple: &'static str,
    pub pointer_width: u8,
}

impl TargetSpec {
    pub const WINDOWS_X86_64_MSVC: Self = Self {
        triple: "x86_64-pc-windows-msvc",
        pointer_width: 64,
    };
}
