//! Cranelift object backend for the verified JAHE IR.
use super::{BackendError, BackendErrorKind};
use crate::ir::{
    BinaryOp, BlockId, Constant, Diagnostic, Function, InstructionKind, Terminator, Type, Value,
    ValueId, VerifiedModule,
};
use cranelift_codegen::{
    ir::{
        condcodes::{FloatCC, IntCC},
        types, AbiParam, Block, BlockArg, InstBuilder, UserFuncName, Value as ClifValue,
    },
    settings, Context,
};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift_module::{default_libcall_names, DataDescription, FuncId, Linkage, Module};
use cranelift_object::{ObjectBuilder, ObjectModule};
#[cfg(windows)]
use std::os::windows::process::CommandExt;
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);
const COMPILER_RUNTIME_ABI_VERSION: u32 = 1;

#[derive(Debug, Default, Clone, Copy)]
pub struct NativeBackend;

impl NativeBackend {
    /// Emit a COFF object for the native Windows x86_64 target.
    pub fn emit_object(&self, verified: &VerifiedModule<'_>) -> Result<Vec<u8>, BackendError> {
        let module = verified.module();
        jahe_runtime::abi::check_abi_compatibility(
            COMPILER_RUNTIME_ABI_VERSION,
            jahe_runtime::abi::abi_version(),
        )
        .map_err(|error| {
            BackendError::new(BackendErrorKind::RuntimeAbiMismatch, error.to_string())
        })?;
        let isa_builder = cranelift_native::builder().map_err(|e| {
            native_error(
                module.functions.first(),
                format!("native target unavailable: {e}"),
            )
        })?;
        let triple = isa_builder.triple();
        if triple.architecture.to_string() != "x86_64"
            || !triple.operating_system.to_string().contains("windows")
        {
            return Err(native_error(
                module.functions.first(),
                format!("native build requires Windows x86_64; detected {triple}"),
            ));
        }

        let flags = settings::Flags::new(settings::builder());
        let isa = isa_builder.finish(flags).map_err(|e| {
            native_error(
                module.functions.first(),
                format!("could not create native ISA: {e}"),
            )
        })?;
        let mut object = ObjectModule::new(
            ObjectBuilder::new(
                isa,
                module.name.as_bytes().to_vec(),
                default_libcall_names(),
            )
            .map_err(|e| native_error(module.functions.first(), e.to_string()))?,
        );

        let mut ids = HashMap::<String, FuncId>::new();
        for function in &module.functions {
            if function
                .params
                .iter()
                .any(|param| matches!(param.value.ty, Type::I128 | Type::U128))
                || matches!(function.return_type, Type::I128 | Type::U128)
            {
                return Err(unsupported(
                    function.span,
                    format!(
                        "function `{}` uses an i128 ABI parameter or return, which Cranelift's default Windows x64 ABI does not support",
                        function.name
                    ),
                ));
            }
            let signature = function_signature(&object, function);
            let id = object
                .declare_function(
                    native_function_symbol(&function.name),
                    Linkage::Export,
                    &signature,
                )
                .map_err(|e| native_error(Some(function), e.to_string()))?;
            ids.insert(function.name.clone(), id);
        }

        let mut needs_print = false;
        for function in &module.functions {
            for instruction in function.blocks.iter().flat_map(|b| &b.instructions) {
                if let InstructionKind::Call { function: name, .. } = &instruction.kind {
                    if module.functions.iter().any(|f| f.name == *name) {
                        continue;
                    }
                    if name == "println" {
                        needs_print = true;
                    } else {
                        return Err(unsupported(
                            instruction.span,
                            format!("external function `{name}` has no native host binding"),
                        ));
                    }
                }
            }
        }
        if needs_print {
            for ty in [
                Type::I32,
                Type::I64,
                Type::F32,
                Type::F64,
                Type::Bool,
                Type::Char,
                Type::Str,
            ] {
                let signature = helper_signature(&object, ty);
                let id = object
                    .declare_function(print_helper(ty), Linkage::Import, &signature)
                    .map_err(|e| native_error(module.functions.first(), e.to_string()))?;
                ids.insert(print_helper(ty).to_owned(), id);
            }
        }

        let pointer_type = object.isa().pointer_type();
        let frontend_config = object.isa().frontend_config();
        for function in &module.functions {
            let func_id = ids[&function.name];
            let mut ctx = Context::new();
            ctx.func = cranelift_codegen::ir::Function::with_name_signature(
                UserFuncName::user(0, func_id.as_u32()),
                function_signature(&object, function),
            );
            let mut builder_ctx = FunctionBuilderContext::new();
            let mut builder = FunctionBuilder::new(&mut ctx.func, &mut builder_ctx);

            let blocks: HashMap<BlockId, Block> = function
                .blocks
                .iter()
                .map(|b| (b.id, builder.create_block()))
                .collect();
            let mut value_map = HashMap::<ValueId, ClifValue>::new();
            for param in &function.params {
                let value = builder.append_block_param(
                    blocks[&BlockId(0)],
                    clif_type(param.value.ty, pointer_type)?,
                );
                value_map.insert(param.value.id, value);
            }
            for block in &function.blocks {
                let clif_block = blocks[&block.id];
                for inst in &block.instructions {
                    if let InstructionKind::Phi { .. } = &inst.kind {
                        let result = inst.result.as_ref().ok_or_else(|| {
                            unsupported(inst.span, "phi instruction has no result")
                        })?;
                        let param = builder
                            .append_block_param(clif_block, clif_type(result.ty, pointer_type)?);
                        value_map.insert(result.id, param);
                    }
                }
            }

            for block in &function.blocks {
                builder.switch_to_block(blocks[&block.id]);
                for inst in &block.instructions {
                    let Some(result) = inst.result.as_ref() else {
                        if !matches!(inst.kind, InstructionKind::Call { .. }) {
                            return Err(unsupported(
                                inst.span,
                                "instruction without a result is not a call",
                            ));
                        }
                        emit_instruction(
                            &mut builder,
                            &mut object,
                            &ids,
                            &mut value_map,
                            function,
                            inst,
                            pointer_type,
                        )?;
                        continue;
                    };
                    if matches!(inst.kind, InstructionKind::Phi { .. }) {
                        continue;
                    }
                    let value = emit_instruction(
                        &mut builder,
                        &mut object,
                        &ids,
                        &mut value_map,
                        function,
                        inst,
                        pointer_type,
                    )?;
                    if let Some(value) = value {
                        value_map.insert(result.id, value);
                    }
                }

                let term = block.terminator.as_ref().ok_or_else(|| {
                    unsupported(
                        block.span,
                        format!("block `{}` has no terminator", block.name),
                    )
                })?;
                match term {
                    Terminator::Return(value) => {
                        let values = value
                            .as_ref()
                            .map(|v| get_value(&value_map, v))
                            .transpose()?
                            .into_iter()
                            .collect::<Vec<_>>();
                        builder.ins().return_(&values);
                    }
                    Terminator::Branch(target) => {
                        let args = edge_values(function, block.id, *target, &value_map)?
                            .into_iter()
                            .map(BlockArg::from)
                            .collect::<Vec<_>>();
                        builder.ins().jump(blocks[target], &args);
                    }
                    Terminator::ConditionalBranch {
                        condition,
                        then_block,
                        else_block,
                    } => {
                        let cond = get_value(&value_map, condition)?;
                        let then_args = edge_values(function, block.id, *then_block, &value_map)?
                            .into_iter()
                            .map(BlockArg::from)
                            .collect::<Vec<_>>();
                        let else_args = edge_values(function, block.id, *else_block, &value_map)?
                            .into_iter()
                            .map(BlockArg::from)
                            .collect::<Vec<_>>();
                        builder.ins().brif(
                            cond,
                            blocks[then_block],
                            &then_args,
                            blocks[else_block],
                            &else_args,
                        );
                    }
                    Terminator::Unreachable => {
                        builder
                            .ins()
                            .trap(cranelift_codegen::ir::TrapCode::unwrap_user(1));
                    }
                }
            }
            builder.seal_all_blocks();
            builder.finalize(frontend_config);
            object
                .define_function(func_id, &mut ctx)
                .map_err(|e| native_error(Some(function), e.to_string()))?;
        }
        object
            .finish()
            .emit()
            .map_err(|e| native_error(module.functions.first(), e.to_string()))
    }

    /// Emit an object and link it with the versioned JAHE runtime static library.
    /// The object is retained next to the executable.
    pub fn build_executable(
        &self,
        verified: &VerifiedModule<'_>,
        object_path: &Path,
        executable_path: &Path,
    ) -> Result<(), BackendError> {
        let bytes = self.emit_object(verified)?;
        let main = verified
            .module()
            .functions
            .iter()
            .find(|f| f.name == "main")
            .ok_or_else(|| {
                BackendError::new(
                    BackendErrorKind::MissingEntry,
                    "native executable requires `main() -> i32`",
                )
            })?;
        if !main.params.is_empty() || main.return_type != Type::I32 {
            return Err(unsupported(
                main.span,
                "native executable entry point must have signature `main() -> i32`",
            ));
        }
        fs::write(object_path, bytes).map_err(|e| {
            BackendError::new(
                BackendErrorKind::NativeCodegen,
                format!("could not write {}: {e}", object_path.display()),
            )
        })?;
        build_and_link_runtime(object_path, executable_path)
    }
}

fn function_signature(
    module: &ObjectModule,
    function: &Function,
) -> cranelift_codegen::ir::Signature {
    let mut signature = module.make_signature();
    signature.call_conv = module.isa().default_call_conv();
    let ptr = module.isa().pointer_type();
    signature.params.extend(
        function
            .params
            .iter()
            .map(|p| AbiParam::new(clif_type(p.value.ty, ptr).expect("verified type"))),
    );
    if !matches!(function.return_type, Type::Void | Type::Never) {
        signature.returns.push(AbiParam::new(
            clif_type(function.return_type, ptr).expect("verified type"),
        ));
    }
    signature
}

fn native_function_symbol(name: &str) -> &str {
    if name == "main" {
        "jahe_program_main"
    } else {
        name
    }
}

fn helper_signature(module: &ObjectModule, ty: Type) -> cranelift_codegen::ir::Signature {
    let mut signature = module.make_signature();
    signature.call_conv = module.isa().default_call_conv();
    signature.params.push(AbiParam::new(
        clif_type(ty, module.isa().pointer_type()).expect("printable type"),
    ));
    signature.returns.push(AbiParam::new(types::I32));
    signature
}

fn clif_type(
    ty: Type,
    ptr: cranelift_codegen::ir::Type,
) -> Result<cranelift_codegen::ir::Type, BackendError> {
    Ok(match ty {
        Type::I8 => types::I8,
        Type::I16 => types::I16,
        Type::I32 => types::I32,
        Type::I64 => types::I64,
        Type::I128 | Type::U128 => types::I128,
        Type::ISize | Type::USize => ptr,
        Type::U8 => types::I8,
        Type::U16 => types::I16,
        Type::U32 => types::I32,
        Type::U64 => types::I64,
        Type::F32 => types::F32,
        Type::F64 => types::F64,
        Type::Bool => types::I8,
        Type::Char => types::I32,
        Type::Str => ptr,
        Type::Void => {
            return Err(BackendError::new(
                BackendErrorKind::NativeCodegen,
                "void has no Cranelift value type",
            ))
        }
        Type::Never => {
            return Err(BackendError::new(
                BackendErrorKind::NativeCodegen,
                "never has no Cranelift value type",
            ));
        }
    })
}

#[allow(clippy::too_many_arguments)]
fn emit_instruction(
    builder: &mut FunctionBuilder<'_>,
    module: &mut ObjectModule,
    ids: &HashMap<String, FuncId>,
    values: &mut HashMap<ValueId, ClifValue>,
    function: &Function,
    inst: &crate::ir::Instruction,
    pointer_type: cranelift_codegen::ir::Type,
) -> Result<Option<ClifValue>, BackendError> {
    let val = |v: &Value| get_value(values, v);
    match &inst.kind {
        InstructionKind::Phi { .. } => Ok(None),
        InstructionKind::Constant(constant) => {
            let ty = inst
                .result
                .as_ref()
                .ok_or_else(|| unsupported(inst.span, "constant has no result"))?
                .ty;
            let value = match (constant, ty) {
                (Constant::Integer(i), ty) if ty.is_integer() => {
                    builder.ins().iconst(clif_type(ty, pointer_type)?, *i)
                }
                (Constant::IntegerBits(bits), ty) if ty.is_integer() => {
                    let clty = clif_type(ty, pointer_type)?;
                    if clty.bits() <= 64 {
                        builder.ins().iconst(clty, *bits as i64)
                    } else {
                        let lo = builder.ins().iconst(types::I64, *bits as u64 as i64);
                        let hi = builder
                            .ins()
                            .iconst(types::I64, (*bits >> 64) as u64 as i64);
                        let lo = builder.ins().uextend(types::I128, lo);
                        let hi = builder.ins().uextend(types::I128, hi);
                        let shift = builder.ins().iconst(types::I64, 64);
                        let hi = builder.ins().ishl(hi, shift);
                        builder.ins().bor(lo, hi)
                    }
                }
                (Constant::Bool(b), Type::Bool) => builder.ins().iconst(types::I8, i64::from(*b)),
                (Constant::Character(c), Type::Char) => builder.ins().iconst(types::I32, *c as i64),
                (Constant::Float(f), Type::F32) => builder.ins().f32const(*f as f32),
                (Constant::Float(f), Type::F64) => builder.ins().f64const(*f),
                (Constant::String(s), Type::Str) => {
                    if s.as_bytes().contains(&0) {
                        return Err(unsupported(
                            inst.span,
                            "strings containing NUL bytes are not supported by the native backend",
                        ));
                    }
                    let mut bytes = s.as_bytes().to_vec();
                    bytes.push(0);
                    let name = format!(
                        "jahe_string_{}_{}",
                        function.name,
                        inst.result.as_ref().unwrap().id.0
                    );
                    let data = module
                        .declare_data(&name, Linkage::Local, false, false)
                        .map_err(|e| native_error(Some(function), e.to_string()))?;
                    let mut desc = DataDescription::new();
                    desc.define(bytes.into_boxed_slice());
                    module
                        .define_data(data, &desc)
                        .map_err(|e| native_error(Some(function), e.to_string()))?;
                    let global = module.declare_data_in_func(data, &mut builder.func);
                    builder.ins().symbol_value(pointer_type, global)
                }
                _ => {
                    return Err(unsupported(
                        inst.span,
                        format!("constant `{constant:?}` is incompatible with `{ty}`"),
                    ))
                }
            };
            Ok(Some(value))
        }
        InstructionKind::Binary { op, left, right } => {
            let lhs = val(left)?;
            let rhs = val(right)?;
            use BinaryOp::*;
            let result = match left.ty {
                Type::I32 | Type::I64 | Type::Bool => match op {
                    Add if left.ty != Type::Bool => {
                        let (value, overflow) = builder.ins().sadd_overflow(lhs, rhs);
                        builder
                            .ins()
                            .trapnz(overflow, cranelift_codegen::ir::TrapCode::unwrap_user(2));
                        value
                    }
                    Sub if left.ty != Type::Bool => {
                        let (value, overflow) = builder.ins().ssub_overflow(lhs, rhs);
                        builder
                            .ins()
                            .trapnz(overflow, cranelift_codegen::ir::TrapCode::unwrap_user(2));
                        value
                    }
                    Mul if left.ty != Type::Bool => {
                        let (value, overflow) = builder.ins().smul_overflow(lhs, rhs);
                        builder
                            .ins()
                            .trapnz(overflow, cranelift_codegen::ir::TrapCode::unwrap_user(2));
                        value
                    }
                    Div | Rem if left.ty != Type::Bool => {
                        let zero = builder.ins().iconst(clif_type(left.ty, pointer_type)?, 0);
                        let is_zero = builder.ins().icmp(IntCC::Equal, rhs, zero);
                        builder
                            .ins()
                            .trapnz(is_zero, cranelift_codegen::ir::TrapCode::unwrap_user(3));
                        let minimum = if left.ty == Type::I32 {
                            i32::MIN as i64
                        } else {
                            i64::MIN
                        };
                        let min_value = builder
                            .ins()
                            .iconst(clif_type(left.ty, pointer_type)?, minimum);
                        let minus_one = builder.ins().iconst(clif_type(left.ty, pointer_type)?, -1);
                        let lhs_min = builder.ins().icmp(IntCC::Equal, lhs, min_value);
                        let rhs_negative_one = builder.ins().icmp(IntCC::Equal, rhs, minus_one);
                        let overflow = builder.ins().band(lhs_min, rhs_negative_one);
                        builder
                            .ins()
                            .trapnz(overflow, cranelift_codegen::ir::TrapCode::unwrap_user(2));
                        if *op == Rem {
                            builder.ins().srem(lhs, rhs)
                        } else {
                            builder.ins().sdiv(lhs, rhs)
                        }
                    }
                    BitAnd if left.ty != Type::Bool => builder.ins().band(lhs, rhs),
                    BitOr if left.ty != Type::Bool => builder.ins().bor(lhs, rhs),
                    BitXor if left.ty != Type::Bool => builder.ins().bxor(lhs, rhs),
                    ShiftLeft | ShiftRight if left.ty != Type::Bool => {
                        let ty = clif_type(left.ty, pointer_type)?;
                        let width = if left.ty == Type::I32 { 32 } else { 64 };
                        let zero = builder.ins().iconst(ty, 0);
                        let upper = builder.ins().iconst(ty, width);
                        let negative = builder.ins().icmp(IntCC::SignedLessThan, rhs, zero);
                        let too_large =
                            builder
                                .ins()
                                .icmp(IntCC::SignedGreaterThanOrEqual, rhs, upper);
                        let invalid_count = builder.ins().bor(negative, too_large);
                        builder.ins().trapnz(
                            invalid_count,
                            cranelift_codegen::ir::TrapCode::unwrap_user(5),
                        );
                        if *op == ShiftRight {
                            builder.ins().sshr(lhs, rhs)
                        } else {
                            let shifted = builder.ins().ishl(lhs, rhs);
                            let restored = builder.ins().sshr(shifted, rhs);
                            let overflow = builder.ins().icmp(IntCC::NotEqual, restored, lhs);
                            builder
                                .ins()
                                .trapnz(overflow, cranelift_codegen::ir::TrapCode::unwrap_user(2));
                            shifted
                        }
                    }
                    Equal => builder.ins().icmp(IntCC::Equal, lhs, rhs),
                    NotEqual => builder.ins().icmp(IntCC::NotEqual, lhs, rhs),
                    Less => builder.ins().icmp(IntCC::SignedLessThan, lhs, rhs),
                    LessEqual => builder.ins().icmp(IntCC::SignedLessThanOrEqual, lhs, rhs),
                    Greater => builder.ins().icmp(IntCC::SignedGreaterThan, lhs, rhs),
                    GreaterEqual => builder
                        .ins()
                        .icmp(IntCC::SignedGreaterThanOrEqual, lhs, rhs),
                    Div | Rem => {
                        return Err(unsupported(
                            inst.span,
                            "division/remainder is not defined for bool",
                        ))
                    }
                    Add | Sub | Mul => {
                        return Err(unsupported(inst.span, "arithmetic is not defined for bool"))
                    }
                    BitAnd | BitOr | BitXor | ShiftLeft | ShiftRight => {
                        return Err(unsupported(
                            inst.span,
                            "bitwise operations are not defined for bool",
                        ))
                    }
                },
                Type::F32 | Type::F64 => match op {
                    Add => builder.ins().fadd(lhs, rhs),
                    Sub => builder.ins().fsub(lhs, rhs),
                    Mul => builder.ins().fmul(lhs, rhs),
                    Div => builder.ins().fdiv(lhs, rhs),
                    Rem => {
                        return Err(unsupported(
                            inst.span,
                            "remainder is defined only for integers",
                        ))
                    }
                    BitAnd | BitOr | BitXor | ShiftLeft | ShiftRight => {
                        return Err(unsupported(
                            inst.span,
                            "bitwise and shift operations require integers",
                        ))
                    }
                    Equal => builder.ins().fcmp(FloatCC::Equal, lhs, rhs),
                    NotEqual => builder.ins().fcmp(FloatCC::NotEqual, lhs, rhs),
                    Less => builder.ins().fcmp(FloatCC::LessThan, lhs, rhs),
                    LessEqual => builder.ins().fcmp(FloatCC::LessThanOrEqual, lhs, rhs),
                    Greater => builder.ins().fcmp(FloatCC::GreaterThan, lhs, rhs),
                    GreaterEqual => builder.ins().fcmp(FloatCC::GreaterThanOrEqual, lhs, rhs),
                },
                Type::Str => {
                    return Err(unsupported(
                        inst.span,
                        "string comparisons are not supported by the native backend",
                    ))
                }
                Type::Char => match op {
                    Equal => builder.ins().icmp(IntCC::Equal, lhs, rhs),
                    NotEqual => builder.ins().icmp(IntCC::NotEqual, lhs, rhs),
                    _ => {
                        return Err(unsupported(
                            inst.span,
                            "only equality comparisons are defined for char",
                        ))
                    }
                },
                Type::Void => {
                    return Err(unsupported(inst.span, "void operands are not supported"))
                }
                Type::Never => return Err(unsupported(inst.span, "never has no runtime value")),
                Type::I8
                | Type::I16
                | Type::I128
                | Type::ISize
                | Type::U8
                | Type::U16
                | Type::U32
                | Type::U64
                | Type::U128
                | Type::USize => {
                    native_integer_binary(builder, *op, lhs, rhs, left.ty, inst.span, pointer_type)?
                }
            };
            Ok(Some(result))
        }
        InstructionKind::Unary {
            op: crate::ir::UnaryOp::Negate,
            value,
        } => {
            let value = val(value)?;
            Ok(Some(builder.ins().fneg(value)))
        }
        InstructionKind::Cast { value, target } => {
            let source = val(value)?;
            Ok(Some(native_cast(
                builder,
                source,
                value.ty,
                *target,
                inst.span,
                pointer_type,
            )?))
        }
        InstructionKind::Call {
            function: name,
            args,
        } => {
            let args = args.iter().map(val).collect::<Result<Vec<_>, _>>()?;
            let id = if name == "println" {
                let ty = match &inst.kind {
                    InstructionKind::Call { args, .. } => args.first().map(|v| v.ty),
                    _ => None,
                }
                .ok_or_else(|| unsupported(inst.span, "println expects one argument"))?;
                if args.len() != 1 {
                    return Err(unsupported(inst.span, "println expects one argument"));
                }
                ids.get(print_helper(ty)).copied().ok_or_else(|| {
                    unsupported(inst.span, format!("println does not support `{ty}`"))
                })?
            } else {
                *ids.get(name)
                    .ok_or_else(|| unsupported(inst.span, format!("unknown function `{name}`")))?
            };
            let reference = module.declare_func_in_func(id, &mut builder.func);
            let call = builder.ins().call(reference, &args);
            let result = builder.inst_results(call).first().copied();
            if name == "println" {
                let status = result.ok_or_else(|| {
                    unsupported(inst.span, "native println runtime returned no status value")
                })?;
                builder
                    .ins()
                    .trapnz(status, cranelift_codegen::ir::TrapCode::unwrap_user(5));
                return Ok(None);
            }
            Ok(result)
        }
    }
}

fn native_integer_binary(
    builder: &mut FunctionBuilder<'_>,
    op: BinaryOp,
    lhs: ClifValue,
    rhs: ClifValue,
    ty: Type,
    span: crate::lexer::token::Span,
    pointer_type: cranelift_codegen::ir::Type,
) -> Result<ClifValue, BackendError> {
    use BinaryOp::*;
    let clty = clif_type(ty, pointer_type)?;
    let width = ty.integer_width(pointer_type.bits() as u8).unwrap();
    let signed = ty.is_signed_integer();
    let trap = cranelift_codegen::ir::TrapCode::unwrap_user(2);
    let value = match op {
        Add | Sub | Mul => {
            let (result, overflow) = match (op, signed) {
                (Add, true) => builder.ins().sadd_overflow(lhs, rhs),
                (Sub, true) => builder.ins().ssub_overflow(lhs, rhs),
                (Mul, true) => builder.ins().smul_overflow(lhs, rhs),
                (Add, false) => builder.ins().uadd_overflow(lhs, rhs),
                (Sub, false) => builder.ins().usub_overflow(lhs, rhs),
                (Mul, false) => builder.ins().umul_overflow(lhs, rhs),
                _ => unreachable!(),
            };
            builder.ins().trapnz(overflow, trap);
            result
        }
        Div | Rem => {
            let zero = emit_integer_constant(builder, clty, 0);
            let is_zero = builder.ins().icmp(IntCC::Equal, rhs, zero);
            builder
                .ins()
                .trapnz(is_zero, cranelift_codegen::ir::TrapCode::unwrap_user(3));
            if width == 128 {
                let (quotient, remainder) = if signed {
                    let minimum = emit_integer_constant(builder, clty, 1u128 << 127);
                    let minus_one = emit_integer_constant(builder, clty, u128::MAX);
                    let is_min = builder.ins().icmp(IntCC::Equal, lhs, minimum);
                    let is_minus_one = builder.ins().icmp(IntCC::Equal, rhs, minus_one);
                    let overflow = builder.ins().band(is_min, is_minus_one);
                    builder.ins().trapnz(overflow, trap);
                    let lhs_negative = builder.ins().icmp(IntCC::SignedLessThan, lhs, zero);
                    let rhs_negative = builder.ins().icmp(IntCC::SignedLessThan, rhs, zero);
                    let abs_lhs = builder.ins().isub(zero, lhs);
                    let abs_rhs = builder.ins().isub(zero, rhs);
                    let abs_lhs = builder.ins().select(lhs_negative, abs_lhs, lhs);
                    let abs_rhs = builder.ins().select(rhs_negative, abs_rhs, rhs);
                    let (q, r) = native_udivrem128(builder, abs_lhs, abs_rhs);
                    let quotient_negative = builder.ins().bxor(lhs_negative, rhs_negative);
                    let negative_q = builder.ins().isub(zero, q);
                    let negative_r = builder.ins().isub(zero, r);
                    (
                        builder.ins().select(quotient_negative, negative_q, q),
                        builder.ins().select(lhs_negative, negative_r, r),
                    )
                } else {
                    native_udivrem128(builder, lhs, rhs)
                };
                return Ok(if op == Div { quotient } else { remainder });
            }
            if signed {
                let min_bits = 1u128 << (width - 1);
                let min = emit_integer_constant(builder, clty, min_bits);
                let minus_one = emit_integer_constant(builder, clty, u128::MAX);
                let is_min = builder.ins().icmp(IntCC::Equal, lhs, min);
                let is_negative_one = builder.ins().icmp(IntCC::Equal, rhs, minus_one);
                let overflow = builder.ins().band(is_min, is_negative_one);
                builder.ins().trapnz(overflow, trap);
                if op == Div {
                    builder.ins().sdiv(lhs, rhs)
                } else {
                    builder.ins().srem(lhs, rhs)
                }
            } else if op == Div {
                builder.ins().udiv(lhs, rhs)
            } else {
                builder.ins().urem(lhs, rhs)
            }
        }
        BitAnd => builder.ins().band(lhs, rhs),
        BitOr => builder.ins().bor(lhs, rhs),
        BitXor => builder.ins().bxor(lhs, rhs),
        ShiftLeft | ShiftRight => {
            let zero = emit_integer_constant(builder, clty, 0);
            let upper = emit_integer_constant(builder, clty, u128::from(width));
            let negative = if signed {
                builder.ins().icmp(IntCC::SignedLessThan, rhs, zero)
            } else {
                builder.ins().iconst(types::I8, 0)
            };
            let too_large = builder.ins().icmp(
                if signed {
                    IntCC::SignedGreaterThanOrEqual
                } else {
                    IntCC::UnsignedGreaterThanOrEqual
                },
                rhs,
                upper,
            );
            let invalid = builder.ins().bor(negative, too_large);
            builder
                .ins()
                .trapnz(invalid, cranelift_codegen::ir::TrapCode::unwrap_user(5));
            if op == ShiftRight {
                if signed {
                    builder.ins().sshr(lhs, rhs)
                } else {
                    builder.ins().ushr(lhs, rhs)
                }
            } else {
                let shifted = builder.ins().ishl(lhs, rhs);
                let restored = if signed {
                    builder.ins().sshr(shifted, rhs)
                } else {
                    builder.ins().ushr(shifted, rhs)
                };
                let overflow = builder.ins().icmp(IntCC::NotEqual, restored, lhs);
                builder.ins().trapnz(overflow, trap);
                shifted
            }
        }
        Equal => builder.ins().icmp(IntCC::Equal, lhs, rhs),
        NotEqual => builder.ins().icmp(IntCC::NotEqual, lhs, rhs),
        Less | LessEqual | Greater | GreaterEqual => {
            let cc = match (op, signed) {
                (Less, true) => IntCC::SignedLessThan,
                (LessEqual, true) => IntCC::SignedLessThanOrEqual,
                (Greater, true) => IntCC::SignedGreaterThan,
                (GreaterEqual, true) => IntCC::SignedGreaterThanOrEqual,
                (Less, false) => IntCC::UnsignedLessThan,
                (LessEqual, false) => IntCC::UnsignedLessThanOrEqual,
                (Greater, false) => IntCC::UnsignedGreaterThan,
                (GreaterEqual, false) => IntCC::UnsignedGreaterThanOrEqual,
                _ => unreachable!(),
            };
            builder.ins().icmp(cc, lhs, rhs)
        }
    };
    let _ = span;
    Ok(value)
}

fn emit_integer_constant(
    builder: &mut FunctionBuilder<'_>,
    ty: cranelift_codegen::ir::Type,
    bits: u128,
) -> ClifValue {
    if ty.bits() <= 64 {
        builder.ins().iconst(ty, bits as i64)
    } else {
        let lo = builder.ins().iconst(types::I64, bits as u64 as i64);
        let hi = builder.ins().iconst(types::I64, (bits >> 64) as u64 as i64);
        let lo = builder.ins().uextend(types::I128, lo);
        let hi = builder.ins().uextend(types::I128, hi);
        let shift = builder.ins().iconst(types::I64, 64);
        let hi = builder.ins().ishl(hi, shift);
        builder.ins().bor(lo, hi)
    }
}

/// Unsigned restoring division for internal i128 bit patterns. Each iteration
/// consumes one numerator bit. The carry bit preserves the 129th bit of the
/// shifted remainder; subtraction is intentionally modular and is selected only
/// when the full (possibly 129-bit) remainder is at least the divisor.
fn native_udivrem128(
    builder: &mut FunctionBuilder<'_>,
    numerator: ClifValue,
    divisor: ClifValue,
) -> (ClifValue, ClifValue) {
    let zero = emit_integer_constant(builder, types::I128, 0);
    let one = emit_integer_constant(builder, types::I128, 1);
    let top_bit = emit_integer_constant(builder, types::I128, 1u128 << 127);
    let mut quotient = zero;
    let mut remainder = zero;
    let shift_one = builder.ins().iconst(types::I64, 1);
    for bit_index in (0..128u32).rev() {
        let shift = builder.ins().iconst(types::I64, i64::from(bit_index));
        let incoming = builder.ins().ushr(numerator, shift);
        let incoming = builder.ins().band(incoming, one);
        let carry_bits = builder.ins().band(remainder, top_bit);
        let carry = builder.ins().icmp(IntCC::NotEqual, carry_bits, zero);
        let doubled = builder.ins().ishl(remainder, shift_one);
        let candidate = builder.ins().bor(doubled, incoming);
        let at_least_divisor =
            builder
                .ins()
                .icmp(IntCC::UnsignedGreaterThanOrEqual, candidate, divisor);
        let subtract = builder.ins().bor(carry, at_least_divisor);
        let reduced = builder.ins().isub(candidate, divisor);
        remainder = builder.ins().select(subtract, reduced, candidate);
        let quotient_bit = builder.ins().ishl(one, shift);
        let quotient_bit = builder.ins().select(subtract, quotient_bit, zero);
        quotient = builder.ins().bor(quotient, quotient_bit);
    }
    (quotient, remainder)
}

fn native_cast(
    builder: &mut FunctionBuilder<'_>,
    value: ClifValue,
    source: Type,
    target: Type,
    span: crate::lexer::token::Span,
    pointer_type: cranelift_codegen::ir::Type,
) -> Result<ClifValue, BackendError> {
    use Type::*;
    if source.is_integer() && target.is_integer() {
        return native_integer_cast(builder, value, source, target, span, pointer_type);
    }
    let source_integer = source.is_integer();
    let target_integer = target.is_integer();
    let source_float = matches!(source, F32 | F64);
    let target_float = matches!(target, F32 | F64);
    let result = match (source_integer, source_float, target_integer, target_float) {
        (true, false, true, false) if source == target => value,
        (true, false, true, false) if source == I32 && target == I64 => {
            builder.ins().sextend(types::I64, value)
        }
        (true, false, true, false) if source == I64 && target == I32 => {
            let narrowed = builder.ins().ireduce(types::I32, value);
            let restored = builder.ins().sextend(types::I64, narrowed);
            let valid = builder.ins().icmp(IntCC::Equal, restored, value);
            let invalid = builder.ins().bnot(valid);
            builder
                .ins()
                .trapnz(invalid, cranelift_codegen::ir::TrapCode::unwrap_user(4));
            narrowed
        }
        (true, false, false, true) => {
            let to = clif_type(target, pointer_type)?;
            if source.is_signed_integer() {
                builder.ins().fcvt_from_sint(to, value)
            } else {
                builder.ins().fcvt_from_uint(to, value)
            }
        }
        (false, true, true, false) => {
            let float64 = if source == F32 {
                builder.ins().fpromote(types::F64, value)
            } else {
                value
            };
            let width = target.integer_width(pointer_type.bits() as u8).unwrap();
            let (min, upper) = if target.is_signed_integer() {
                (
                    -(2f64.powi(i32::from(width - 1))),
                    2f64.powi(i32::from(width - 1)),
                )
            } else {
                (0.0, 2f64.powi(i32::from(width)))
            };
            let lower_bound = builder.ins().f64const(min);
            let upper_bound = builder.ins().f64const(upper);
            let below = builder.ins().fcmp(FloatCC::LessThan, float64, lower_bound);
            let above = builder
                .ins()
                .fcmp(FloatCC::GreaterThanOrEqual, float64, upper_bound);
            let unordered = builder.ins().fcmp(FloatCC::Unordered, float64, float64);
            let out_of_range = builder.ins().bor(below, above);
            let invalid = builder.ins().bor(out_of_range, unordered);
            builder
                .ins()
                .trapnz(invalid, cranelift_codegen::ir::TrapCode::unwrap_user(4));
            if target.is_signed_integer() {
                builder
                    .ins()
                    .fcvt_to_sint(clif_type(target, pointer_type)?, float64)
            } else {
                builder
                    .ins()
                    .fcvt_to_uint(clif_type(target, pointer_type)?, float64)
            }
        }
        (false, true, false, true) if source == F32 && target == F64 => {
            builder.ins().fpromote(types::F64, value)
        }
        (false, true, false, true) if source == F64 && target == F32 => {
            builder.ins().fdemote(types::F32, value)
        }
        _ => {
            return Err(unsupported(
                span,
                format!("cast from `{source}` to `{target}` is unsupported"),
            ))
        }
    };
    Ok(result)
}

fn native_integer_cast(
    builder: &mut FunctionBuilder<'_>,
    value: ClifValue,
    source: Type,
    target: Type,
    span: crate::lexer::token::Span,
    pointer_type: cranelift_codegen::ir::Type,
) -> Result<ClifValue, BackendError> {
    let sw = source.integer_width(pointer_type.bits() as u8).unwrap();
    let tw = target.integer_width(pointer_type.bits() as u8).unwrap();
    let source_ty = clif_type(source, pointer_type)?;
    let target_ty = clif_type(target, pointer_type)?;
    let signed_source = source.is_signed_integer();
    let signed_target = target.is_signed_integer();
    let trap = cranelift_codegen::ir::TrapCode::unwrap_user(4);
    if signed_source && !signed_target {
        let zero = emit_integer_constant(builder, source_ty, 0);
        let negative = builder.ins().icmp(IntCC::SignedLessThan, value, zero);
        builder.ins().trapnz(negative, trap);
    }
    let converted = if tw > sw {
        if signed_source && signed_target {
            builder.ins().sextend(target_ty, value)
        } else {
            builder.ins().uextend(target_ty, value)
        }
    } else if tw < sw {
        let narrowed = builder.ins().ireduce(target_ty, value);
        let restored = if signed_target {
            builder.ins().sextend(source_ty, narrowed)
        } else {
            builder.ins().uextend(source_ty, narrowed)
        };
        let valid = builder.ins().icmp(IntCC::Equal, restored, value);
        let invalid = builder.ins().bnot(valid);
        builder.ins().trapnz(invalid, trap);
        narrowed
    } else {
        if !signed_source && signed_target {
            let highbit = emit_integer_constant(builder, source_ty, 1u128 << (sw - 1));
            let has_sign_bit =
                builder
                    .ins()
                    .icmp(IntCC::UnsignedGreaterThanOrEqual, value, highbit);
            builder.ins().trapnz(has_sign_bit, trap);
        }
        value
    };
    if source == target {
        let _ = span;
    }
    Ok(converted)
}

fn edge_values(
    function: &Function,
    predecessor: BlockId,
    target: BlockId,
    values: &HashMap<ValueId, ClifValue>,
) -> Result<Vec<ClifValue>, BackendError> {
    let target_block = function
        .blocks
        .iter()
        .find(|b| b.id == target)
        .ok_or_else(|| {
            native_error(
                Some(function),
                format!("branch targets missing block {}", target.0),
            )
        })?;
    let mut args = Vec::new();
    for inst in &target_block.instructions {
        if let InstructionKind::Phi { incoming } = &inst.kind {
            let value = incoming
                .iter()
                .find(|(from, _)| *from == predecessor)
                .map(|(_, v)| v)
                .ok_or_else(|| {
                    unsupported(
                        inst.span,
                        format!("phi has no input for predecessor {}", predecessor.0),
                    )
                })?;
            args.push(get_value(values, value)?);
        } else {
            break;
        }
    }
    Ok(args)
}

fn get_value(
    values: &HashMap<ValueId, ClifValue>,
    value: &Value,
) -> Result<ClifValue, BackendError> {
    values.get(&value.id).copied().ok_or_else(|| {
        unsupported(
            value.span,
            format!(
                "SSA value %{} is unavailable during native lowering",
                value.id.0
            ),
        )
    })
}

fn print_helper(ty: Type) -> &'static str {
    match ty {
        Type::I8
        | Type::I16
        | Type::I128
        | Type::ISize
        | Type::U8
        | Type::U16
        | Type::U32
        | Type::U64
        | Type::U128
        | Type::USize => "jahe_rt_v1_unsupported_integer_print",
        Type::Never => "jahe_rt_v1_unsupported_never_print",
        Type::I32 => "jahe_rt_v1_print_i32",
        Type::I64 => "jahe_rt_v1_print_i64",
        Type::F32 => "jahe_rt_v1_print_f32",
        Type::F64 => "jahe_rt_v1_print_f64",
        Type::Bool => "jahe_rt_v1_print_bool",
        Type::Char => "jahe_rt_v1_print_char",
        Type::Str => "jahe_rt_v1_print_str",
        Type::Void => "jahe_rt_v1_print_void",
    }
}

fn native_error(function: Option<&Function>, message: impl Into<String>) -> BackendError {
    let message = message.into();
    let mut error = BackendError::new(BackendErrorKind::NativeCodegen, message.clone());
    if let Some(function) = function {
        error.diagnostics.push(Diagnostic {
            code: "NAT0001",
            message,
            span: function.span,
        });
    }
    error
}

fn unsupported(span: crate::lexer::token::Span, message: impl Into<String>) -> BackendError {
    let message = message.into();
    let mut error = BackendError::new(BackendErrorKind::NativeCodegen, message.clone());
    error.diagnostics.push(Diagnostic {
        code: "NAT0002",
        message,
        span,
    });
    error
}

fn build_and_link_runtime(object_path: &Path, executable_path: &Path) -> Result<(), BackendError> {
    let envs = msvc_environment()?;
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("compiler package has a repository parent")
        .join("runtime/Cargo.toml");
    let target_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/jahe-runtime");
    let cargo_args = vec![
        "build".into(),
        "--offline".into(),
        "--release".into(),
        "--manifest-path".into(),
        manifest.display().to_string(),
        "--target-dir".into(),
        target_dir.display().to_string(),
    ];
    run_tool("cargo", &cargo_args, &envs, BackendErrorKind::Linker)?;
    let runtime_lib = target_dir.join("release/jahe_runtime.lib");
    if !runtime_lib.is_file() {
        return Err(BackendError::new(
            BackendErrorKind::Linker,
            format!(
                "runtime ABI v{COMPILER_RUNTIME_ABI_VERSION} archive was not produced at {}",
                runtime_lib.display()
            ),
        ));
    }
    let launcher_id = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
    let launcher = std::env::temp_dir().join(format!(
        "jahe_native_launcher_{}_{}.rs",
        std::process::id(),
        launcher_id
    ));
    fs::write(
        &launcher,
        "extern \"C\" { fn jahe_program_main() -> i32; }\nfn main() { let code = unsafe { jahe_program_main() }; std::process::exit(code); }\n",
    )
    .map_err(|e| {
        BackendError::new(
            BackendErrorKind::Linker,
            format!("could not prepare native entry-point shim: {e}"),
        )
    })?;
    let link_args = vec![
        "--edition=2021".into(),
        "--crate-name=jahe_native_launcher".into(),
        "--target=x86_64-pc-windows-msvc".into(),
        launcher.display().to_string(),
        "-C".into(),
        format!("link-arg={}", object_path.display()),
        "-C".into(),
        format!("link-arg={}", runtime_lib.display()),
        "-o".into(),
        executable_path.display().to_string(),
    ];
    let result = run_tool("rustc", &link_args, &envs, BackendErrorKind::Linker);
    let _ = fs::remove_file(launcher);
    result
}

fn msvc_environment() -> Result<Vec<(String, String)>, BackendError> {
    let devcmd = find_vsdevcmd().ok_or_else(|| {
        BackendError::new(
            BackendErrorKind::Linker,
            "MSVC build tools not found; run from a Visual Studio x64 developer environment",
        )
    })?;
    let setup = format!(
        "/d /c call \"{}\" -arch=x64 -host_arch=x64 >nul && set",
        devcmd.display()
    );
    #[cfg(windows)]
    let output = Command::new("cmd.exe").raw_arg(setup).output();
    #[cfg(not(windows))]
    let output: std::io::Result<std::process::Output> = Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "MSVC is Windows-only",
    ));
    let output = output.map_err(|e| {
        BackendError::new(
            BackendErrorKind::Linker,
            format!("could not initialize MSVC tools: {e}"),
        )
    })?;
    if !output.status.success() {
        return Err(BackendError::new(
            BackendErrorKind::Linker,
            format!(
                "Visual Studio environment setup failed:\n{}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            ),
        ));
    }
    let mut envs = Vec::new();
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        if let Some((key, value)) = line.split_once('=') {
            if !key.is_empty() && !key.starts_with('=') {
                envs.push((key.to_owned(), value.to_owned()));
            }
        }
    }
    Ok(envs)
}

fn run_tool(
    tool: &str,
    arguments: &[String],
    envs: &[(String, String)],
    error_kind: BackendErrorKind,
) -> Result<(), BackendError> {
    let mut command = Command::new(tool);
    command.args(arguments);
    for (key, value) in envs {
        command.env(key, value);
    }
    let output = command
        .output()
        .map_err(|e| BackendError::new(error_kind.clone(), format!("could not run {tool}: {e}")))?;
    if output.status.success() {
        return Ok(());
    }
    let details = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    if details.contains("unresolved external symbol jahe_rt_v") {
        return Err(BackendError::new(
            BackendErrorKind::RuntimeAbiMismatch,
            format!("could not resolve versioned JAHE runtime symbols (compiler ABI v{COMPILER_RUNTIME_ABI_VERSION}); rebuild the matching runtime archive.\n{details}"),
        ));
    }
    Err(BackendError::new(
        error_kind,
        format!("{tool} failed:\n{details}"),
    ))
}

fn find_vsdevcmd() -> Option<PathBuf> {
    let root = std::env::var_os("ProgramFiles(x86)").map(PathBuf::from)?;
    let vs = root.join("Microsoft Visual Studio");
    for edition in ["BuildTools", "Community", "Professional", "Enterprise"] {
        let p = vs
            .join("18")
            .join(edition)
            .join("Common7/Tools/VsDevCmd.bat");
        if p.is_file() {
            return Some(p);
        }
    }
    let versions = fs::read_dir(vs).ok()?;
    for version in versions.flatten() {
        for edition in ["BuildTools", "Community", "Professional", "Enterprise"] {
            let p = version
                .path()
                .join(edition)
                .join("Common7/Tools/VsDevCmd.bat");
            if p.is_file() {
                return Some(p);
            }
        }
    }
    None
}
