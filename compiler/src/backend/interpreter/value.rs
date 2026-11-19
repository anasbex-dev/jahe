use crate::ir::Type;
#[derive(Debug, Clone, PartialEq)]
pub enum RuntimeValue {
    I32(i32),
    I64(i64),
    /// Raw two's-complement/unsigned bits for integer types outside the legacy i32/i64 variants.
    Integer {
        bits: u128,
        ty: Type,
    },
    F32(f32),
    F64(f64),
    Bool(bool),
    Char(char),
    Str(String),
}
impl RuntimeValue {
    pub fn ty(&self) -> Type {
        match self {
            Self::I32(_) => Type::I32,
            Self::I64(_) => Type::I64,
            Self::Integer { ty, .. } => *ty,
            Self::F32(_) => Type::F32,
            Self::F64(_) => Type::F64,
            Self::Bool(_) => Type::Bool,
            Self::Char(_) => Type::Char,
            Self::Str(_) => Type::Str,
        }
    }
}
impl std::fmt::Display for RuntimeValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::I32(v) => write!(f, "{v}"),
            Self::I64(v) => write!(f, "{v}"),
            Self::Integer { bits, ty } => {
                let width = ty.integer_width(64).expect("integer runtime type");
                if ty.is_signed_integer() {
                    write!(f, "{}", signed_bits(*bits, width))
                } else {
                    write!(f, "{}", bits & integer_mask(width))
                }
            }
            Self::F32(v) => write!(f, "{v}"),
            Self::F64(v) => write!(f, "{v}"),
            Self::Bool(v) => write!(f, "{v}"),
            Self::Char(v) => write!(f, "{v}"),
            Self::Str(v) => f.write_str(v),
        }
    }
}

fn integer_mask(width: u8) -> u128 {
    if width == 128 {
        u128::MAX
    } else {
        (1u128 << width) - 1
    }
}
fn signed_bits(bits: u128, width: u8) -> i128 {
    if width == 128 {
        bits as i128
    } else {
        let bits = bits & integer_mask(width);
        let sign = 1u128 << (width - 1);
        if bits & sign == 0 {
            bits as i128
        } else {
            (bits | !integer_mask(width)) as i128
        }
    }
}
