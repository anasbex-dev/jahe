//! Stable status values returned by recoverable runtime operations.

/// Numeric values are part of the C ABI. Zero means success.
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeStatus {
    Ok = 0,
    InvalidArgument = 1,
    InvalidPointer = 2,
    OutOfMemory = 3,
    IoError = 4,
    InternalError = 5,
}

impl RuntimeStatus {
    pub const fn code(self) -> u32 {
        self as u32
    }
}
