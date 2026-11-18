//! Versioned ABI constants and compatibility checks.

/// Version implemented by this runtime crate.
pub const ABI_VERSION: u32 = 1;
/// All exported runtime symbols start with this ABI-versioned prefix.
pub const ABI_PREFIX: &str = "jahe_rt_v1_";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AbiVersionMismatch {
    pub compiler_version: u32,
    pub runtime_version: u32,
}

impl std::fmt::Display for AbiVersionMismatch {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "compiler runtime ABI v{} is incompatible with runtime ABI v{}",
            self.compiler_version, self.runtime_version
        )
    }
}

impl std::error::Error for AbiVersionMismatch {}

pub fn check_abi_compatibility(
    compiler_version: u32,
    runtime_version: u32,
) -> Result<(), AbiVersionMismatch> {
    if compiler_version == runtime_version {
        Ok(())
    } else {
        Err(AbiVersionMismatch {
            compiler_version,
            runtime_version,
        })
    }
}

/// Exported so native build tools can query the archive's ABI when needed.
#[no_mangle]
pub extern "C" fn jahe_rt_v1_abi_version() -> u32 {
    ABI_VERSION
}

pub fn abi_version() -> u32 {
    jahe_rt_v1_abi_version()
}
