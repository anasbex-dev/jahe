use super::{ConstEvalError, ConstEvalErrorKind as ErrorKind, Ty, TypedConstant};
use crate::{
    ir::Constant,
    parser::ast::{Expr, ExprKind, Type},
};
use std::collections::HashMap;

pub(crate) fn evaluate(
    expr: &Expr,
    expected: Option<Ty>,
    constants: &HashMap<String, TypedConstant>,
    pointer_width: u8,
) -> Result<TypedConstant, ConstEvalError> {
    eval(expr, expected, constants, pointer_width)
}

fn failure(kind: ErrorKind, expr: &Expr, operation: &str, ty: Option<Ty>) -> ConstEvalError {
    ConstEvalError {
        kind,
        span: expr.span,
        operation: operation.to_owned(),
        ty,
    }
}

fn eval(
    expr: &Expr,
    hint: Option<Ty>,
    constants: &HashMap<String, TypedConstant>,
    pointer_width: u8,
) -> Result<TypedConstant, ConstEvalError> {
    match &expr.kind {
        ExprKind::Name(name) => constants
            .get(name)
            .cloned()
            .ok_or_else(|| failure(ErrorKind::InvalidDependency(name.clone()), expr, name, None)),
        ExprKind::Integer(literal) => {
            let magnitude = literal.magnitude().ok_or_else(|| {
                failure(
                    ErrorKind::UnsupportedExpression,
                    expr,
                    "integer literal",
                    hint,
                )
            })?;
            let ty = hint.filter(|ty| ty.is_integer()).unwrap_or_else(|| {
                if magnitude <= i32::MAX as u128 {
                    Ty::I32
                } else if magnitude <= i64::MAX as u128 {
                    Ty::I64
                } else if magnitude <= i128::MAX as u128 {
                    Ty::I128
                } else {
                    Ty::U128
                }
            });
            let width = ty.integer_width(pointer_width).unwrap();
            let max = if ty.is_signed_integer() {
                signed_max(width) as u128
            } else {
                mask(width)
            };
            if magnitude > max {
                return Err(failure(
                    ErrorKind::IntegerOverflow,
                    expr,
                    "integer literal",
                    Some(ty),
                ));
            }
            Ok(TypedConstant {
                ty,
                value: Constant::IntegerBits(magnitude),
            })
        }
        ExprKind::Float(value) => {
            let ty = match hint.filter(|ty| matches!(ty, Ty::F32 | Ty::F64)) {
                Some(ty) => ty,
                _ => Ty::F64,
            };
            let value = if ty == Ty::F32 {
                f64::from(*value as f32)
            } else {
                *value
            };
            Ok(TypedConstant {
                ty,
                value: Constant::Float(value),
            })
        }
        ExprKind::Bool(value) => Ok(TypedConstant {
            ty: Ty::Bool,
            value: Constant::Bool(*value),
        }),
        ExprKind::Character(value) => Ok(TypedConstant {
            ty: Ty::Char,
            value: Constant::Character(*value),
        }),
        ExprKind::String(value) => Ok(TypedConstant {
            ty: Ty::Str,
            value: Constant::String(value.clone()),
        }),
        ExprKind::Unary { op, value } if op == "!" => {
            let operand = eval(value, Some(Ty::Bool), constants, pointer_width)?;
            if let Constant::Bool(value) = operand.value {
                Ok(TypedConstant {
                    ty: Ty::Bool,
                    value: Constant::Bool(!value),
                })
            } else {
                Err(failure(
                    ErrorKind::InvalidTypeOperation,
                    expr,
                    op,
                    Some(operand.ty),
                ))
            }
        }
        ExprKind::Unary { op, value } if op == "-" => {
            let operand = if let ExprKind::Integer(literal) = &value.kind {
                let ty = hint.filter(|ty| ty.is_integer()).unwrap_or(Ty::I32);
                let width = ty.integer_width(pointer_width).unwrap();
                let magnitude = literal.magnitude().ok_or_else(|| {
                    failure(
                        ErrorKind::UnsupportedExpression,
                        value,
                        "integer literal",
                        Some(ty),
                    )
                })?;
                if !ty.is_signed_integer() || magnitude > signed_min_magnitude(width) {
                    return Err(failure(
                        ErrorKind::IntegerUnderflow,
                        expr,
                        "unary -",
                        Some(ty),
                    ));
                }
                let bits = (0u128.wrapping_sub(magnitude)) & mask(width);
                return Ok(TypedConstant {
                    ty,
                    value: Constant::IntegerBits(bits),
                });
            } else {
                eval(value, hint, constants, pointer_width)?
            };
            match operand.value {
                Constant::IntegerBits(bits) if operand.ty.is_integer() => {
                    let width = operand.ty.integer_width(pointer_width).unwrap();
                    if !operand.ty.is_signed_integer() {
                        if bits & mask(width) == 0 {
                            return Ok(TypedConstant {
                                ty: operand.ty,
                                value: Constant::IntegerBits(0),
                            });
                        }
                        return Err(failure(
                            ErrorKind::IntegerUnderflow,
                            expr,
                            "unary -",
                            Some(operand.ty),
                        ));
                    }
                    let signed = signed_value(bits, width);
                    let result = signed.checked_neg().ok_or_else(|| {
                        failure(
                            ErrorKind::IntegerOverflow,
                            expr,
                            "unary -",
                            Some(operand.ty),
                        )
                    })?;
                    check_signed(result, width)
                        .map_err(|kind| failure(kind, expr, "unary -", Some(operand.ty)))?;
                    Ok(TypedConstant {
                        ty: operand.ty,
                        value: Constant::IntegerBits((result as u128) & mask(width)),
                    })
                }
                Constant::Float(value) if operand.ty == Ty::F32 => Ok(TypedConstant {
                    ty: Ty::F32,
                    value: Constant::Float(f64::from(-(value as f32))),
                }),
                Constant::Float(value) if operand.ty == Ty::F64 => Ok(TypedConstant {
                    ty: Ty::F64,
                    value: Constant::Float(-value),
                }),
                _ => Err(failure(
                    ErrorKind::InvalidTypeOperation,
                    expr,
                    "unary -",
                    Some(operand.ty),
                )),
            }
        }
        ExprKind::Cast { value, target } => {
            let target = parse_type(target)
                .ok_or_else(|| failure(ErrorKind::InvalidTypeOperation, expr, "as", None))?;
            let source = eval(value, None, constants, pointer_width)?;
            cast(source, target, expr, pointer_width)
        }
        ExprKind::Binary { left, op, right } if op == "&&" || op == "||" => {
            let lhs = eval(left, Some(Ty::Bool), constants, pointer_width)?;
            let Constant::Bool(a) = lhs.value else {
                return Err(failure(
                    ErrorKind::InvalidTypeOperation,
                    expr,
                    op,
                    Some(lhs.ty),
                ));
            };
            if (op == "&&" && !a) || (op == "||" && a) {
                return Ok(TypedConstant {
                    ty: Ty::Bool,
                    value: Constant::Bool(a),
                });
            }
            let rhs = eval(right, Some(Ty::Bool), constants, pointer_width)?;
            if let Constant::Bool(b) = rhs.value {
                Ok(TypedConstant {
                    ty: Ty::Bool,
                    value: Constant::Bool(b),
                })
            } else {
                Err(failure(
                    ErrorKind::InvalidTypeOperation,
                    expr,
                    op,
                    Some(rhs.ty),
                ))
            }
        }
        ExprKind::Binary { left, op, right } => {
            let comparison = matches!(op.as_str(), "==" | "!=" | "<" | "<=" | ">" | ">=");
            let operand_hint = if comparison { None } else { hint };
            let lhs = eval(left, operand_hint, constants, pointer_width)?;
            let rhs = eval(right, Some(lhs.ty), constants, pointer_width)?;
            if lhs.ty != rhs.ty {
                return Err(failure(
                    ErrorKind::InvalidTypeOperation,
                    expr,
                    op,
                    Some(lhs.ty),
                ));
            }
            if comparison {
                compare(lhs, rhs, op, expr, pointer_width)
            } else {
                binary(lhs, rhs, op, expr, pointer_width)
            }
        }
        _ => Err(failure(
            ErrorKind::UnsupportedExpression,
            expr,
            "constant expression",
            hint,
        )),
    }
}

fn parse_type(ty: &Type) -> Option<Ty> {
    Ty::parse(ty)
}

fn mask(width: u32) -> u128 {
    if width == 128 {
        u128::MAX
    } else {
        (1u128 << width) - 1
    }
}

fn signed_min(width: u32) -> i128 {
    if width == 128 {
        i128::MIN
    } else {
        -(1i128 << (width - 1))
    }
}

fn signed_max(width: u32) -> i128 {
    if width == 128 {
        i128::MAX
    } else {
        (1i128 << (width - 1)) - 1
    }
}

fn signed_min_magnitude(width: u32) -> u128 {
    1u128 << (width - 1)
}

fn signed_value(bits: u128, width: u32) -> i128 {
    let bits = bits & mask(width);
    if width == 128 {
        bits as i128
    } else if bits & (1u128 << (width - 1)) == 0 {
        bits as i128
    } else {
        (bits | !mask(width)) as i128
    }
}

fn check_signed(value: i128, width: u32) -> Result<(), ErrorKind> {
    if value < signed_min(width) {
        Err(ErrorKind::IntegerUnderflow)
    } else if value > signed_max(width) {
        Err(ErrorKind::IntegerOverflow)
    } else {
        Ok(())
    }
}

fn integer_bits(value: &TypedConstant) -> Option<u128> {
    match value.value {
        Constant::Integer(value) => Some(value as u128),
        Constant::IntegerBits(value) => Some(value),
        _ => None,
    }
}

fn binary(
    lhs: TypedConstant,
    rhs: TypedConstant,
    op: &str,
    expr: &Expr,
    pointer_width: u8,
) -> Result<TypedConstant, ConstEvalError> {
    let ty = lhs.ty;
    if ty.is_integer() {
        let width = ty.integer_width(pointer_width).unwrap();
        let mask = mask(width);
        let a = integer_bits(&lhs)
            .ok_or_else(|| failure(ErrorKind::InvalidTypeOperation, expr, op, Some(ty)))?
            & mask;
        let b = integer_bits(&rhs)
            .ok_or_else(|| failure(ErrorKind::InvalidTypeOperation, expr, op, Some(ty)))?
            & mask;
        let result = if ty.is_signed_integer() {
            let (a, b) = (signed_value(a, width), signed_value(b, width));
            (match op {
                "+" => checked_signed(a.checked_add(b), expr, op, ty, width)?,
                "-" => checked_signed(a.checked_sub(b), expr, op, ty, width)?,
                "*" => checked_signed(a.checked_mul(b), expr, op, ty, width)?,
                "/" | "%" => {
                    if b == 0 {
                        return Err(failure(ErrorKind::DivisionByZero, expr, op, Some(ty)));
                    }
                    if a == signed_min(width) && b == -1 {
                        if op == "/" {
                            return Err(failure(
                                ErrorKind::SignedDivisionOverflow,
                                expr,
                                op,
                                Some(ty),
                            ));
                        }
                        0
                    } else if op == "/" {
                        a / b
                    } else {
                        a % b
                    }
                }
                "&" => (a as u128 & b as u128) as i128,
                "|" => (a as u128 | b as u128) as i128,
                "^" => (a as u128 ^ b as u128) as i128,
                "<<" | ">>" => {
                    if b < 0 || b >= i128::from(width) {
                        return Err(failure(ErrorKind::InvalidShift, expr, op, Some(ty)));
                    }
                    let count = b as u32;
                    if op == "<<" {
                        let shifted = checked_signed(a.checked_shl(count), expr, op, ty, width)?;
                        if (shifted >> count) != a {
                            return Err(failure(ErrorKind::IntegerOverflow, expr, op, Some(ty)));
                        }
                        shifted
                    } else {
                        a >> count
                    }
                }
                _ => return Err(failure(ErrorKind::InvalidTypeOperation, expr, op, Some(ty))),
            }) as u128
                & mask
        } else {
            let result = match op {
                "+" => a
                    .checked_add(b)
                    .ok_or_else(|| failure(ErrorKind::IntegerOverflow, expr, op, Some(ty)))?,
                "-" => a
                    .checked_sub(b)
                    .ok_or_else(|| failure(ErrorKind::IntegerUnderflow, expr, op, Some(ty)))?,
                "*" => a
                    .checked_mul(b)
                    .ok_or_else(|| failure(ErrorKind::IntegerOverflow, expr, op, Some(ty)))?,
                "/" | "%" => {
                    if b == 0 {
                        return Err(failure(ErrorKind::DivisionByZero, expr, op, Some(ty)));
                    }
                    if op == "/" {
                        a / b
                    } else {
                        a % b
                    }
                }
                "&" => a & b,
                "|" => a | b,
                "^" => a ^ b,
                "<<" | ">>" => {
                    if b >= u128::from(width) {
                        return Err(failure(ErrorKind::InvalidShift, expr, op, Some(ty)));
                    }
                    let count = b as u32;
                    if op == "<<" {
                        if a > (mask >> count) {
                            return Err(failure(ErrorKind::IntegerOverflow, expr, op, Some(ty)));
                        }
                        a.checked_shl(count).ok_or_else(|| {
                            failure(ErrorKind::IntegerOverflow, expr, op, Some(ty))
                        })?
                    } else {
                        a >> count
                    }
                }
                _ => return Err(failure(ErrorKind::InvalidTypeOperation, expr, op, Some(ty))),
            };
            if result > mask {
                return Err(failure(ErrorKind::IntegerOverflow, expr, op, Some(ty)));
            }
            result
        };
        return Ok(TypedConstant {
            ty,
            value: Constant::IntegerBits(result),
        });
    }
    match (ty, lhs.value, rhs.value) {
        (Ty::F32, Constant::Float(a), Constant::Float(b)) => {
            let (a, b) = (a as f32, b as f32);
            let result = match op {
                "+" => a + b,
                "-" => a - b,
                "*" => a * b,
                "/" => a / b,
                _ => return Err(failure(ErrorKind::InvalidTypeOperation, expr, op, Some(ty))),
            };
            Ok(TypedConstant {
                ty,
                value: Constant::Float(f64::from(result)),
            })
        }
        (Ty::F64, Constant::Float(a), Constant::Float(b)) => {
            let result = match op {
                "+" => a + b,
                "-" => a - b,
                "*" => a * b,
                "/" => a / b,
                _ => return Err(failure(ErrorKind::InvalidTypeOperation, expr, op, Some(ty))),
            };
            Ok(TypedConstant {
                ty,
                value: Constant::Float(result),
            })
        }
        _ => Err(failure(ErrorKind::InvalidTypeOperation, expr, op, Some(ty))),
    }
}

fn checked_signed(
    value: Option<i128>,
    expr: &Expr,
    op: &str,
    ty: Ty,
    width: u32,
) -> Result<i128, ConstEvalError> {
    let value = value.ok_or_else(|| failure(ErrorKind::IntegerOverflow, expr, op, Some(ty)))?;
    check_signed(value, width).map_err(|kind| failure(kind, expr, op, Some(ty)))?;
    Ok(value)
}

fn compare(
    lhs: TypedConstant,
    rhs: TypedConstant,
    op: &str,
    expr: &Expr,
    pointer_width: u8,
) -> Result<TypedConstant, ConstEvalError> {
    let result = match (lhs.ty, lhs.value, rhs.value) {
        (ty, Constant::Integer(a), Constant::Integer(b)) if ty.is_integer() => {
            let width = ty.integer_width(pointer_width).unwrap();
            let (a, b) = if ty.is_signed_integer() {
                (
                    signed_value(a as u128, width),
                    signed_value(b as u128, width),
                )
            } else {
                (a as u128 as i128, b as u128 as i128)
            };
            compare_ord(a.cmp(&b), op)
        }
        (ty, Constant::IntegerBits(a), Constant::IntegerBits(b)) if ty.is_integer() => {
            let width = ty.integer_width(pointer_width).unwrap();
            let (a, b) = if ty.is_signed_integer() {
                (signed_value(a, width), signed_value(b, width))
            } else {
                (a as i128, b as i128)
            };
            // Unsigned u128 ordering must not pass through i128.
            if ty.is_signed_integer() {
                compare_ord(a.cmp(&b), op)
            } else {
                compare_ord(a_as_u128(a).cmp(&a_as_u128(b)), op)
            }
        }
        (Ty::F32, Constant::Float(a), Constant::Float(b)) => {
            compare_float((a as f32) as f64, (b as f32) as f64, op)
        }
        (Ty::F64, Constant::Float(a), Constant::Float(b)) => compare_float(a, b, op),
        (Ty::Bool, Constant::Bool(a), Constant::Bool(b)) => compare_ord(a.cmp(&b), op),
        (Ty::Str, Constant::String(a), Constant::String(b)) if matches!(op, "==" | "!=") => {
            if op == "==" {
                a == b
            } else {
                a != b
            }
        }
        (Ty::Char, Constant::Character(a), Constant::Character(b)) if matches!(op, "==" | "!=") => {
            if op == "==" {
                a == b
            } else {
                a != b
            }
        }
        _ => {
            return Err(failure(
                ErrorKind::InvalidTypeOperation,
                expr,
                op,
                Some(lhs.ty),
            ))
        }
    };
    Ok(TypedConstant {
        ty: Ty::Bool,
        value: Constant::Bool(result),
    })
}

fn a_as_u128(value: i128) -> u128 {
    value as u128
}

fn compare_ord(order: std::cmp::Ordering, op: &str) -> bool {
    use std::cmp::Ordering::*;
    match op {
        "==" => order == Equal,
        "!=" => order != Equal,
        "<" => order == Less,
        "<=" => order != Greater,
        ">" => order == Greater,
        ">=" => order != Less,
        _ => false,
    }
}

fn compare_float(a: f64, b: f64, op: &str) -> bool {
    match op {
        "==" => a == b,
        "!=" => a != b,
        "<" => a < b,
        "<=" => a <= b,
        ">" => a > b,
        ">=" => a >= b,
        _ => false,
    }
}

fn cast(
    source: TypedConstant,
    target: Ty,
    expr: &Expr,
    pointer_width: u8,
) -> Result<TypedConstant, ConstEvalError> {
    let fail = || failure(ErrorKind::OutOfRangeCast, expr, "as", Some(target));
    if source.ty.is_integer() && target.is_integer() {
        let source_width = source.ty.integer_width(pointer_width).unwrap();
        let target_width = target.integer_width(pointer_width).unwrap();
        let bits = integer_bits(&source).ok_or_else(fail)? & mask(source_width);
        let (negative, magnitude) = if source.ty.is_signed_integer() {
            let signed = signed_value(bits, source_width);
            (signed < 0, signed.unsigned_abs())
        } else {
            (false, bits)
        };
        let representable = if target.is_signed_integer() {
            if negative {
                magnitude <= signed_min_magnitude(target_width)
            } else {
                magnitude <= signed_max(target_width) as u128
            }
        } else {
            !negative && magnitude <= mask(target_width)
        };
        if !representable {
            return Err(fail());
        }
        let out = if negative {
            (0u128.wrapping_sub(magnitude)) & mask(target_width)
        } else {
            magnitude & mask(target_width)
        };
        return Ok(TypedConstant {
            ty: target,
            value: Constant::IntegerBits(out),
        });
    }
    if source.ty.is_integer() && matches!(target, Ty::F32 | Ty::F64) {
        let width = source.ty.integer_width(pointer_width).unwrap();
        let bits = integer_bits(&source).ok_or_else(fail)? & mask(width);
        let value = if source.ty.is_signed_integer() {
            signed_value(bits, width) as f64
        } else {
            bits as f64
        };
        return Ok(TypedConstant {
            ty: target,
            value: Constant::Float(if target == Ty::F32 {
                f64::from(value as f32)
            } else {
                value
            }),
        });
    }
    if let Constant::Float(value) = source.value {
        if matches!(source.ty, Ty::F32 | Ty::F64) && matches!(target, Ty::F32 | Ty::F64) {
            let value = if target == Ty::F32 {
                f64::from(value as f32)
            } else {
                value
            };
            return Ok(TypedConstant {
                ty: target,
                value: Constant::Float(value),
            });
        }
        if source.ty == Ty::F32 && target.is_integer() { /* stored value already has f32 precision */
        }
        if (source.ty == Ty::F32 || source.ty == Ty::F64) && target.is_integer() {
            let width = target.integer_width(pointer_width).unwrap();
            let v = if source.ty == Ty::F32 {
                f64::from(value as f32)
            } else {
                value
            };
            let (lower, upper) = if target.is_signed_integer() {
                (
                    -(2f64).powi(width as i32 - 1),
                    (2f64).powi(width as i32 - 1),
                )
            } else {
                (0.0, (2f64).powi(width as i32))
            };
            if !v.is_finite() || v < lower || v >= upper {
                return Err(fail());
            }
            let out = if target.is_signed_integer() {
                (v.trunc() as i128 as u128) & mask(width)
            } else {
                (v.trunc() as u128) & mask(width)
            };
            return Ok(TypedConstant {
                ty: target,
                value: Constant::IntegerBits(out),
            });
        }
    }
    Err(failure(
        ErrorKind::InvalidTypeOperation,
        expr,
        "as",
        Some(target),
    ))
}
