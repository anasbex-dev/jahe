use super::{
    builtins::{BuiltinRegistry, HostContext, HostFunction},
    error::{RuntimeError, RuntimeErrorKind},
    frame::Frame,
    value::RuntimeValue,
};
use crate::{
    backend::{Backend, BackendError, BackendErrorKind},
    ir::{
        self, BinaryOp, BlockId, Constant, Function, InstructionKind, Module, Terminator, Type,
        Value, VerifiedModule,
    },
    lexer::token::Span,
};

#[derive(Debug, Clone, Copy)]
pub struct ExecutionLimits {
    pub max_steps: u64,
    pub max_call_depth: usize,
}
impl Default for ExecutionLimits {
    fn default() -> Self {
        Self {
            max_steps: 1_000_000,
            max_call_depth: 128,
        }
    }
}
#[derive(Debug, Clone, PartialEq)]
pub struct ExecutionResult {
    pub return_value: Option<RuntimeValue>,
    pub output: String,
    pub steps: u64,
}
pub struct Interpreter {
    pub limits: ExecutionLimits,
    pub builtins: BuiltinRegistry,
}
impl Default for Interpreter {
    fn default() -> Self {
        Self::new(ExecutionLimits::default())
    }
}
impl Interpreter {
    pub fn new(limits: ExecutionLimits) -> Self {
        Self {
            limits,
            builtins: BuiltinRegistry::with_standard(),
        }
    }
    pub fn with_builtins(limits: ExecutionLimits, builtins: BuiltinRegistry) -> Self {
        Self { limits, builtins }
    }
    pub fn register_builtin(
        &mut self,
        name: impl Into<String>,
        function: impl HostFunction + 'static,
    ) -> Result<(), super::builtins::RegistrationError> {
        self.builtins.register(name, function)
    }
    pub fn execute(&self, module: &Module) -> Result<ExecutionResult, BackendError> {
        let verified = ir::verify::Verifier
            .verify(module)
            .map_err(BackendError::verification)?;
        self.execute_verified(&verified)
    }
    pub fn execute_function(
        &self,
        module: &Module,
        name: &str,
        args: Vec<RuntimeValue>,
    ) -> Result<ExecutionResult, BackendError> {
        let verified = ir::verify::Verifier
            .verify(module)
            .map_err(BackendError::verification)?;
        self.execute_function_verified(&verified, name, args)
    }
    pub fn execute_verified(
        &self,
        module: &VerifiedModule<'_>,
    ) -> Result<ExecutionResult, BackendError> {
        let ir_module = module.module();
        let Some(main) = ir_module.functions.iter().find(|f| f.name == "main") else {
            return Err(BackendError::new(
                BackendErrorKind::MissingEntry,
                "module has no `main` entry function",
            ));
        };
        if !main.params.is_empty() {
            return Err(BackendError::new(
                BackendErrorKind::InvalidEntrySignature,
                "entry function `main` must not take parameters",
            ));
        }
        if !matches!(main.return_type, Type::I32 | Type::Void) {
            return Err(BackendError::new(
                BackendErrorKind::InvalidEntrySignature,
                "entry function `main` must return i32 or void",
            ));
        }
        self.execute_function_verified(module, "main", Vec::new())
    }
    pub fn execute_function_verified(
        &self,
        module: &VerifiedModule<'_>,
        name: &str,
        args: Vec<RuntimeValue>,
    ) -> Result<ExecutionResult, BackendError> {
        let ir_module = module.module();
        self.validate_external_registry(ir_module)?;
        let mut engine = Engine {
            module: ir_module,
            builtins: &self.builtins,
            limits: self.limits,
            steps: 0,
            context: HostContext::default(),
        };
        let value = engine
            .invoke(name, args, 0, None)
            .map_err(BackendError::runtime)?;
        Ok(ExecutionResult {
            return_value: value,
            output: engine.context.output,
            steps: engine.steps,
        })
    }
    fn validate_external_registry(&self, module: &Module) -> Result<(), BackendError> {
        for external in &module.externals {
            if let Some(registered)=self.builtins.signature(&external.name){
                if registered!=external.signature{return Err(BackendError::new(BackendErrorKind::ExternalSignatureMismatch,format!("registered host function `{}` has signature {}, but IR declares {}",external.name,display_signature(&registered),display_signature(&external.signature))));}
            }else if module.functions.iter().flat_map(|f|&f.blocks).flat_map(|b|&b.instructions).any(|i|matches!(&i.kind,InstructionKind::Call{function,..} if function==&external.name)){
                return Err(BackendError::new(BackendErrorKind::ExternalFunctionUnresolved,format!("external function `{}` is declared but has no registered host implementation",external.name)));
            }
        }
        Ok(())
    }
}

fn display_signature(signature: &ir::Signature) -> String {
    let params = signature
        .params
        .iter()
        .map(|p| match p {
            ir::ParameterType::Exact(ty) => ty.to_string(),
            ir::ParameterType::AnyValue => "any-value".to_string(),
        })
        .collect::<Vec<_>>()
        .join(", ");
    format!("({params}) -> {}", signature.return_type)
}
impl Backend for Interpreter {
    type Output = ExecutionResult;
    type Error = BackendError;

    fn execute(&self, module: &VerifiedModule<'_>) -> Result<Self::Output, Self::Error> {
        self.execute_verified(module)
    }
}

struct Engine<'a> {
    module: &'a Module,
    builtins: &'a BuiltinRegistry,
    limits: ExecutionLimits,
    steps: u64,
    context: HostContext,
}
impl Engine<'_> {
    fn tick(&mut self, span: Span) -> Result<(), RuntimeError> {
        if self.steps >= self.limits.max_steps {
            return Err(RuntimeError::new(
                RuntimeErrorKind::StepLimitExceeded,
                format!("execution exceeded {} steps", self.limits.max_steps),
                Some(span),
            ));
        }
        self.steps += 1;
        Ok(())
    }
    fn invoke(
        &mut self,
        name: &str,
        args: Vec<RuntimeValue>,
        depth: usize,
        call_span: Option<Span>,
    ) -> Result<Option<RuntimeValue>, RuntimeError> {
        if let Some(function) = self
            .module
            .functions
            .iter()
            .find(|f| f.name == name)
            .cloned()
        {
            if depth >= self.limits.max_call_depth {
                return Err(RuntimeError::new(
                    RuntimeErrorKind::CallDepthExceeded,
                    format!("maximum call depth {} exceeded", self.limits.max_call_depth),
                    call_span,
                ));
            }
            return self.call_function(function, args, depth);
        }
        let Some(external) = self
            .module
            .externals
            .iter()
            .find(|e| e.name == name)
            .cloned()
        else {
            return Err(RuntimeError::new(
                RuntimeErrorKind::UnresolvedExternal,
                format!("call references undeclared external `{name}`"),
                call_span,
            ));
        };
        let Some(host) = self.builtins.get(name) else {
            return Err(RuntimeError::new(
                RuntimeErrorKind::UnresolvedExternal,
                format!("external function `{name}` has no host implementation"),
                call_span,
            ));
        };
        let span = call_span.unwrap_or(external.span);
        if args.len() != external.signature.params.len()
            || args
                .iter()
                .zip(&external.signature.params)
                .any(|(arg, expected)| !expected.accepts(arg.ty()))
        {
            return Err(RuntimeError::new(
                RuntimeErrorKind::ExternalArgumentMismatch,
                format!(
                    "arguments do not match external `{name}` signature {}",
                    display_signature(&external.signature)
                ),
                Some(span),
            ));
        }
        let result = host.call(&args, &mut self.context, span)?;
        let valid = match (external.signature.return_type, &result) {
            (Type::Void, None) => true,
            (Type::Void, Some(_)) => false,
            (expected, Some(value)) => value.ty() == expected,
            (_, None) => false,
        };
        if !valid {
            return Err(RuntimeError::new(
                RuntimeErrorKind::ExternalReturnMismatch,
                format!(
                    "host function `{name}` returned a value incompatible with `{}`",
                    external.signature.return_type
                ),
                Some(span),
            ));
        }
        return Ok(result);
    }
    fn call_function(
        &mut self,
        function: Function,
        args: Vec<RuntimeValue>,
        depth: usize,
    ) -> Result<Option<RuntimeValue>, RuntimeError> {
        if args.len() != function.params.len() {
            return Err(RuntimeError::new(
                RuntimeErrorKind::ArgumentCount,
                format!(
                    "function `{}` expected {} arguments, found {}",
                    function.name,
                    function.params.len(),
                    args.len()
                ),
                Some(function.span),
            ));
        }
        let mut frame = Frame {
            values: Default::default(),
            predecessor: None,
            block: BlockId(0),
        };
        for (param, arg) in function.params.iter().zip(args) {
            if param.value.ty != arg.ty() {
                return Err(RuntimeError::new(
                    RuntimeErrorKind::ArgumentType,
                    format!(
                        "parameter `{}` expected {}, found {}",
                        param.name,
                        param.value.ty,
                        arg.ty()
                    ),
                    Some(param.value.span),
                ));
            }
            frame.values.insert(param.value.id, arg);
        }
        loop {
            let Some(block) = function
                .blocks
                .iter()
                .find(|b| b.id == frame.block)
                .cloned()
            else {
                return Err(RuntimeError::new(
                    RuntimeErrorKind::InvalidControlFlow,
                    format!("block {} is missing in `{}`", frame.block.0, function.name),
                    Some(function.span),
                ));
            };
            // Phi operands are read from the incoming edge before any phi result is written.
            let mut pending = Vec::new();
            for inst in block
                .instructions
                .iter()
                .take_while(|i| matches!(i.kind, InstructionKind::Phi { .. }))
            {
                self.tick(inst.span)?;
                let InstructionKind::Phi { incoming } = &inst.kind else {
                    continue;
                };
                let predecessor = frame.predecessor.ok_or_else(|| {
                    RuntimeError::new(
                        RuntimeErrorKind::InvalidControlFlow,
                        "phi reached without a predecessor",
                        Some(inst.span),
                    )
                })?;
                let (_, operand) = incoming
                    .iter()
                    .find(|(b, _)| *b == predecessor)
                    .ok_or_else(|| {
                        RuntimeError::new(
                            RuntimeErrorKind::InvalidControlFlow,
                            format!("phi has no input for predecessor {}", predecessor.0),
                            Some(inst.span),
                        )
                    })?;
                let result = inst.result.as_ref().ok_or_else(|| {
                    RuntimeError::new(
                        RuntimeErrorKind::InvalidValue,
                        "phi has no result",
                        Some(inst.span),
                    )
                })?;
                pending.push((result.id, self.read(&frame, operand)?));
            }
            for (id, value) in pending {
                frame.values.insert(id, value);
            }
            for inst in block
                .instructions
                .iter()
                .filter(|i| !matches!(i.kind, InstructionKind::Phi { .. }))
            {
                self.tick(inst.span)?;
                let result = match &inst.kind {
                    InstructionKind::Constant(c) => {
                        Some(constant(c, inst.result.as_ref().map(|v| v.ty), inst.span)?)
                    }
                    InstructionKind::Binary { op, left, right } => Some(binary(
                        *op,
                        self.read(&frame, left)?,
                        self.read(&frame, right)?,
                        inst.span,
                    )?),
                    InstructionKind::Unary {
                        op: ir::UnaryOp::Negate,
                        value,
                    } => {
                        let value = self.read(&frame, value)?;
                        Some(match value {
                            RuntimeValue::F32(v) => RuntimeValue::F32(-v),
                            RuntimeValue::F64(v) => RuntimeValue::F64(-v),
                            _ => {
                                return Err(RuntimeError::new(
                                    RuntimeErrorKind::InvalidValue,
                                    "invalid operand to floating negation",
                                    Some(inst.span),
                                ))
                            }
                        })
                    }
                    InstructionKind::Cast { value, target } => {
                        Some(cast(self.read(&frame, value)?, *target, inst.span)?)
                    }
                    InstructionKind::Call {
                        function: name,
                        args,
                    } => {
                        let values = args
                            .iter()
                            .map(|a| self.read(&frame, a))
                            .collect::<Result<Vec<_>, _>>()?;
                        self.invoke(name, values, depth + 1, Some(inst.span))?
                    }
                    InstructionKind::Phi { .. } => {
                        return Err(RuntimeError::new(
                            RuntimeErrorKind::InvalidControlFlow,
                            "phi appears after a non-phi instruction",
                            Some(inst.span),
                        ))
                    }
                };
                match (&inst.result, result) {
                    (Some(def), Some(value)) => {
                        if def.ty != value.ty() {
                            return Err(RuntimeError::new(
                                RuntimeErrorKind::InvalidValue,
                                format!(
                                    "instruction result expected {}, got {}",
                                    def.ty,
                                    value.ty()
                                ),
                                Some(inst.span),
                            ));
                        }
                        frame.values.insert(def.id, value);
                    }
                    (None, None) => {}
                    _ => {
                        return Err(RuntimeError::new(
                            RuntimeErrorKind::InvalidValue,
                            "instruction result/value mismatch",
                            Some(inst.span),
                        ))
                    }
                }
            }
            let term = block.terminator.ok_or_else(|| {
                RuntimeError::new(
                    RuntimeErrorKind::InvalidControlFlow,
                    format!("block `{}` is unterminated", block.name),
                    Some(block.span),
                )
            })?;
            self.tick(block.span)?;
            match term {
                Terminator::Return(value) => {
                    return value.as_ref().map(|v| self.read(&frame, v)).transpose()
                }
                Terminator::Branch(target) => {
                    frame.predecessor = Some(block.id);
                    frame.block = target;
                }
                Terminator::ConditionalBranch {
                    condition,
                    then_block,
                    else_block,
                } => {
                    let value = self.read(&frame, &condition)?;
                    let RuntimeValue::Bool(test) = value else {
                        return Err(RuntimeError::new(
                            RuntimeErrorKind::InvalidValue,
                            "branch condition is not bool",
                            Some(condition.span),
                        ));
                    };
                    frame.predecessor = Some(block.id);
                    frame.block = if test { then_block } else { else_block };
                }
                Terminator::Unreachable => {
                    return Err(RuntimeError::new(
                        RuntimeErrorKind::InvalidControlFlow,
                        "execution reached an unreachable terminator",
                        Some(block.span),
                    ))
                }
            }
        }
    }
    fn read(&self, frame: &Frame, value: &Value) -> Result<RuntimeValue, RuntimeError> {
        frame.values.get(&value.id).cloned().ok_or_else(|| {
            RuntimeError::new(
                RuntimeErrorKind::MissingValue,
                format!("SSA value %{} is unavailable", value.id.0),
                Some(value.span),
            )
        })
    }
}

fn constant(c: &Constant, ty: Option<Type>, span: Span) -> Result<RuntimeValue, RuntimeError> {
    match (c, ty) {
        (Constant::Integer(v), Some(Type::I32)) => {
            i32::try_from(*v).map(RuntimeValue::I32).map_err(|_| {
                RuntimeError::new(
                    RuntimeErrorKind::InvalidValue,
                    "integer constant out of i32 range",
                    Some(span),
                )
            })
        }
        (Constant::Integer(v), Some(Type::I64)) => Ok(RuntimeValue::I64(*v)),
        (Constant::Integer(v), Some(ty)) if ty.is_integer() => {
            integer_from_i128(i128::from(*v), ty).map_err(|_| {
                RuntimeError::new(
                    RuntimeErrorKind::InvalidValue,
                    format!("integer constant out of `{ty}` range"),
                    Some(span),
                )
            })
        }
        (Constant::IntegerBits(bits), Some(ty)) if ty.is_integer() => {
            let width = ty.integer_width(64).unwrap();
            if width < 128 && (*bits >> width) != 0 {
                return Err(RuntimeError::new(
                    RuntimeErrorKind::InvalidValue,
                    format!("integer bit pattern exceeds `{ty}` width"),
                    Some(span),
                ));
            }
            Ok(integer_value(*bits, ty))
        }
        (Constant::Float(v), Some(Type::F32)) => Ok(RuntimeValue::F32(*v as f32)),
        (Constant::Float(v), Some(Type::F64)) => Ok(RuntimeValue::F64(*v)),
        (Constant::String(v), Some(Type::Str)) => Ok(RuntimeValue::Str(v.clone())),
        (Constant::Bool(v), Some(Type::Bool)) => Ok(RuntimeValue::Bool(*v)),
        (Constant::Character(v), Some(Type::Char)) => Ok(RuntimeValue::Char(*v)),
        _ => Err(RuntimeError::new(
            RuntimeErrorKind::InvalidValue,
            "constant/type mismatch",
            Some(span),
        )),
    }
}
fn cast(value: RuntimeValue, target: Type, span: Span) -> Result<RuntimeValue, RuntimeError> {
    use RuntimeValue as V;
    let invalid = || {
        RuntimeError::new(
            RuntimeErrorKind::InvalidCast,
            format!("value cannot be represented as `{target}`"),
            Some(span),
        )
    };
    if target.is_integer() {
        if let Some((bits, source)) = integer_parts(&value) {
            return cast_integer(bits, source, target).map_err(|_| invalid());
        }
    }
    if matches!(target, Type::F32 | Type::F64) {
        if let Some((bits, source)) = integer_parts(&value) {
            let width = source.integer_width(64).unwrap();
            let number = if source.is_signed_integer() {
                signed_from_bits(bits, width) as f64
            } else {
                (bits & integer_mask(width)) as f64
            };
            return match target {
                Type::F32 => Ok(V::F32(number as f32)),
                Type::F64 => Ok(V::F64(number)),
                _ => unreachable!(),
            };
        }
    }
    match (value, target) {
        (V::I32(v), Type::I32) => Ok(V::I32(v)),
        (V::I32(v), Type::I64) => Ok(V::I64(i64::from(v))),
        (V::I32(v), Type::F32) => Ok(V::F32(v as f32)),
        (V::I32(v), Type::F64) => Ok(V::F64(v as f64)),
        (V::I64(v), Type::I32) => i32::try_from(v).map(V::I32).map_err(|_| invalid()),
        (V::I64(v), Type::I64) => Ok(V::I64(v)),
        (V::I64(v), Type::F32) => Ok(V::F32(v as f32)),
        (V::I64(v), Type::F64) => Ok(V::F64(v as f64)),
        (V::F32(v), Type::I32) => float_to_i32(f64::from(v), span).map(V::I32),
        (V::F32(v), Type::I64) => float_to_i64(f64::from(v), span).map(V::I64),
        (V::F32(v), target) if target.is_integer() => float_to_integer(f64::from(v), target, span),
        (V::F32(v), Type::F32) => Ok(V::F32(v)),
        (V::F32(v), Type::F64) => Ok(V::F64(f64::from(v))),
        (V::F64(v), Type::I32) => float_to_i32(v, span).map(V::I32),
        (V::F64(v), Type::I64) => float_to_i64(v, span).map(V::I64),
        (V::F64(v), target) if target.is_integer() => float_to_integer(v, target, span),
        (V::F64(v), Type::F32) => Ok(V::F32(v as f32)),
        (V::F64(v), Type::F64) => Ok(V::F64(v)),
        _ => Err(invalid()),
    }
}
fn float_to_integer(value: f64, target: Type, span: Span) -> Result<RuntimeValue, RuntimeError> {
    let width = target.integer_width(64).unwrap();
    let valid = if target.is_signed_integer() {
        value.is_finite()
            && value >= -2f64.powi(i32::from(width - 1))
            && value < 2f64.powi(i32::from(width - 1))
    } else {
        value.is_finite() && value >= 0.0 && value < 2f64.powi(i32::from(width))
    };
    if !valid {
        return Err(RuntimeError::new(
            RuntimeErrorKind::InvalidCast,
            format!("float is outside the range of `{target}`"),
            Some(span),
        ));
    }
    let truncated = value.trunc();
    let bits = if target.is_signed_integer() {
        (truncated as i128) as u128
    } else {
        truncated as u128
    };
    Ok(integer_value(bits, target))
}
fn float_to_i32(value: f64, span: Span) -> Result<i32, RuntimeError> {
    if value.is_finite() && value >= i32::MIN as f64 && value < (i32::MAX as f64) + 1.0 {
        Ok(value.trunc() as i32)
    } else {
        Err(RuntimeError::new(
            RuntimeErrorKind::InvalidCast,
            "float is outside the range of `i32`",
            Some(span),
        ))
    }
}
fn float_to_i64(value: f64, span: Span) -> Result<i64, RuntimeError> {
    const I64_UPPER_EXCLUSIVE: f64 = 9_223_372_036_854_775_808.0;
    if value.is_finite() && value >= i64::MIN as f64 && value < I64_UPPER_EXCLUSIVE {
        Ok(value.trunc() as i64)
    } else {
        Err(RuntimeError::new(
            RuntimeErrorKind::InvalidCast,
            "float is outside the range of `i64`",
            Some(span),
        ))
    }
}
fn binary(
    op: BinaryOp,
    left: RuntimeValue,
    right: RuntimeValue,
    span: Span,
) -> Result<RuntimeValue, RuntimeError> {
    use RuntimeValue as V;
    match (left, right) {
        (V::I32(a), V::I32(b)) => integer32(op, a, b, span),
        (V::I64(a), V::I64(b)) => integer64(op, a, b, span),
        (V::Integer { bits: a, ty: ta }, V::Integer { bits: b, ty: tb }) if ta == tb => {
            integer_wide(op, a, b, ta, span)
        }
        (V::F32(a), V::F32(b)) => float32(op, a, b, span),
        (V::F64(a), V::F64(b)) => float64(op, a, b, span),
        (V::Bool(a), V::Bool(b)) => compare(op, a.cmp(&b))
            .map(V::Bool)
            .ok_or_else(|| invalid_binary(span)),
        (V::Char(a), V::Char(b)) => match op {
            BinaryOp::Equal => Ok(V::Bool(a == b)),
            BinaryOp::NotEqual => Ok(V::Bool(a != b)),
            _ => Err(invalid_binary(span)),
        },
        (V::Str(a), V::Str(b)) => compare(op, a.cmp(&b))
            .map(V::Bool)
            .ok_or_else(|| invalid_binary(span)),
        _ => Err(invalid_binary(span)),
    }
}
fn invalid_binary(span: Span) -> RuntimeError {
    RuntimeError::new(
        RuntimeErrorKind::InvalidValue,
        "binary operand type mismatch",
        Some(span),
    )
}

fn integer_mask(width: u8) -> u128 {
    if width == 128 {
        u128::MAX
    } else {
        (1u128 << width) - 1
    }
}
fn signed_from_bits(bits: u128, width: u8) -> i128 {
    if width == 128 {
        bits as i128
    } else {
        let mask = integer_mask(width);
        let value = bits & mask;
        let sign = 1u128 << (width - 1);
        if value & sign == 0 {
            value as i128
        } else {
            (value | !mask) as i128
        }
    }
}
fn integer_parts(value: &RuntimeValue) -> Option<(u128, Type)> {
    match value {
        RuntimeValue::I32(v) => Some((*v as u32 as u128, Type::I32)),
        RuntimeValue::I64(v) => Some((*v as u64 as u128, Type::I64)),
        RuntimeValue::Integer { bits, ty } => Some((*bits, *ty)),
        _ => None,
    }
}
fn integer_value(bits: u128, ty: Type) -> RuntimeValue {
    let width = ty.integer_width(64).unwrap();
    let bits = bits & integer_mask(width);
    match ty {
        Type::I32 => RuntimeValue::I32(bits as u32 as i32),
        Type::I64 => RuntimeValue::I64(bits as u64 as i64),
        _ => RuntimeValue::Integer { bits, ty },
    }
}
fn integer_from_i128(value: i128, ty: Type) -> Result<RuntimeValue, ()> {
    let width = ty.integer_width(64).ok_or(())?;
    let bits = if ty.is_signed_integer() {
        let min = if width == 128 {
            i128::MIN
        } else {
            -(1i128 << (width - 1))
        };
        let max = if width == 128 {
            i128::MAX
        } else {
            (1i128 << (width - 1)) - 1
        };
        if value < min || value > max {
            return Err(());
        }
        value as u128
    } else {
        if value < 0 || (value as u128) > integer_mask(width) {
            return Err(());
        }
        value as u128
    };
    Ok(integer_value(bits, ty))
}
fn cast_integer(bits: u128, source: Type, target: Type) -> Result<RuntimeValue, ()> {
    let sw = source.integer_width(64).ok_or(())?;
    let tw = target.integer_width(64).ok_or(())?;
    let source_bits = bits & integer_mask(sw);
    if source.is_signed_integer() {
        integer_from_i128(signed_from_bits(source_bits, sw), target)
    } else {
        let value = source_bits;
        if target.is_signed_integer() {
            let max = if tw == 128 {
                i128::MAX as u128
            } else {
                (1u128 << (tw - 1)) - 1
            };
            if value > max {
                return Err(());
            }
        } else if value > integer_mask(tw) {
            return Err(());
        }
        Ok(integer_value(value, target))
    }
}
fn integer_wide(
    op: BinaryOp,
    a: u128,
    b: u128,
    ty: Type,
    span: Span,
) -> Result<RuntimeValue, RuntimeError> {
    let width = ty.integer_width(64).unwrap();
    let mask = integer_mask(width);
    let a = a & mask;
    let b = b & mask;
    let signed = ty.is_signed_integer();
    let out = if matches!(
        op,
        BinaryOp::Equal
            | BinaryOp::NotEqual
            | BinaryOp::Less
            | BinaryOp::LessEqual
            | BinaryOp::Greater
            | BinaryOp::GreaterEqual
    ) {
        let ord = if signed {
            signed_from_bits(a, width).cmp(&signed_from_bits(b, width))
        } else {
            a.cmp(&b)
        };
        return compare(op, ord)
            .map(RuntimeValue::Bool)
            .ok_or_else(|| invalid_binary(span));
    } else if matches!(op, BinaryOp::ShiftLeft | BinaryOp::ShiftRight) {
        let count = if signed {
            signed_from_bits(b, width)
        } else {
            i128::try_from(b).unwrap_or(i128::MAX)
        };
        if count < 0 || count >= i128::from(width) {
            return Err(RuntimeError::new(
                RuntimeErrorKind::InvalidShift,
                format!("shift count must be between 0 and {}", width - 1),
                Some(span),
            ));
        }
        let count = count as u32;
        if op == BinaryOp::ShiftRight {
            if signed {
                (signed_from_bits(a, width) >> count) as u128 & mask
            } else {
                a >> count
            }
        } else {
            let shifted = a.wrapping_shl(count) & mask;
            let overflow = if signed {
                ((signed_from_bits(shifted, width) >> count) as u128 & mask) != a
            } else {
                (shifted >> count) != a
            };
            if overflow {
                return Err(integer_overflow(ty, span));
            }
            shifted
        }
    } else if matches!(op, BinaryOp::BitAnd | BinaryOp::BitOr | BinaryOp::BitXor) {
        match op {
            BinaryOp::BitAnd => a & b,
            BinaryOp::BitOr => a | b,
            _ => a ^ b,
        }
    } else if signed {
        let x = signed_from_bits(a, width);
        let y = signed_from_bits(b, width);
        if matches!(op, BinaryOp::Div | BinaryOp::Rem) && y == 0 {
            return Err(RuntimeError::new(
                RuntimeErrorKind::DivisionByZero,
                "integer division by zero",
                Some(span),
            ));
        }
        let value = match op {
            BinaryOp::Add => x.checked_add(y),
            BinaryOp::Sub => x.checked_sub(y),
            BinaryOp::Mul => x.checked_mul(y),
            BinaryOp::Div => x.checked_div(y),
            BinaryOp::Rem => x.checked_rem(y),
            _ => None,
        }
        .ok_or_else(|| integer_overflow(ty, span))?;
        let min = if width == 128 {
            i128::MIN
        } else {
            -(1i128 << (width - 1))
        };
        let max = if width == 128 {
            i128::MAX
        } else {
            (1i128 << (width - 1)) - 1
        };
        if value < min || value > max {
            return Err(integer_overflow(ty, span));
        }
        value as u128 & mask
    } else {
        if matches!(op, BinaryOp::Div | BinaryOp::Rem) && b == 0 {
            return Err(RuntimeError::new(
                RuntimeErrorKind::DivisionByZero,
                "integer division by zero",
                Some(span),
            ));
        }
        match op {
            BinaryOp::Add => a.checked_add(b),
            BinaryOp::Sub => a.checked_sub(b),
            BinaryOp::Mul => a.checked_mul(b),
            BinaryOp::Div => Some(a / b),
            BinaryOp::Rem => Some(a % b),
            _ => None,
        }
        .filter(|value| *value <= mask)
        .ok_or_else(|| integer_overflow(ty, span))?
    };
    Ok(integer_value(out, ty))
}
fn integer_overflow(ty: Type, span: Span) -> RuntimeError {
    RuntimeError::new(
        RuntimeErrorKind::IntegerOverflow,
        format!("{ty} arithmetic overflow"),
        Some(span),
    )
}
fn div_zero<T>(b: T, zero: T, span: Span) -> Result<(), RuntimeError>
where
    T: PartialEq,
{
    if b == zero {
        Err(RuntimeError::new(
            RuntimeErrorKind::DivisionByZero,
            "integer division by zero",
            Some(span),
        ))
    } else {
        Ok(())
    }
}
fn integer32(op: BinaryOp, a: i32, b: i32, s: Span) -> Result<RuntimeValue, RuntimeError> {
    if op == BinaryOp::ShiftLeft {
        let shifted = checked_left_shift(i64::from(a), i64::from(b), 32, s)? as i32;
        return Ok(RuntimeValue::I32(shifted));
    }
    if op == BinaryOp::ShiftRight {
        return Ok(RuntimeValue::I32(a >> valid_shift(i64::from(b), 32, s)?));
    }
    let checked = match op {
        BinaryOp::Add => a.checked_add(b),
        BinaryOp::Sub => a.checked_sub(b),
        BinaryOp::Mul => a.checked_mul(b),
        BinaryOp::Div => {
            div_zero(b, 0, s)?;
            a.checked_div(b)
        }
        BinaryOp::Rem => {
            div_zero(b, 0, s)?;
            a.checked_rem(b)
        }
        BinaryOp::BitAnd => Some(a & b),
        BinaryOp::BitOr => Some(a | b),
        BinaryOp::BitXor => Some(a ^ b),
        _ => None,
    };
    if matches!(
        op,
        BinaryOp::Add
            | BinaryOp::Sub
            | BinaryOp::Mul
            | BinaryOp::Div
            | BinaryOp::Rem
            | BinaryOp::BitAnd
            | BinaryOp::BitOr
            | BinaryOp::BitXor
    ) {
        return checked.map(RuntimeValue::I32).ok_or_else(|| {
            RuntimeError::new(
                RuntimeErrorKind::IntegerOverflow,
                "i32 arithmetic overflow",
                Some(s),
            )
        });
    }
    compare(op, a.cmp(&b))
        .map(RuntimeValue::Bool)
        .ok_or_else(|| invalid_binary(s))
}
fn integer64(op: BinaryOp, a: i64, b: i64, s: Span) -> Result<RuntimeValue, RuntimeError> {
    if op == BinaryOp::ShiftLeft {
        return checked_left_shift(a, b, 64, s).map(RuntimeValue::I64);
    }
    if op == BinaryOp::ShiftRight {
        return Ok(RuntimeValue::I64(a >> valid_shift(b, 64, s)?));
    }
    let checked = match op {
        BinaryOp::Add => a.checked_add(b),
        BinaryOp::Sub => a.checked_sub(b),
        BinaryOp::Mul => a.checked_mul(b),
        BinaryOp::Div => {
            div_zero(b, 0, s)?;
            a.checked_div(b)
        }
        BinaryOp::Rem => {
            div_zero(b, 0, s)?;
            a.checked_rem(b)
        }
        BinaryOp::BitAnd => Some(a & b),
        BinaryOp::BitOr => Some(a | b),
        BinaryOp::BitXor => Some(a ^ b),
        _ => None,
    };
    if matches!(
        op,
        BinaryOp::Add
            | BinaryOp::Sub
            | BinaryOp::Mul
            | BinaryOp::Div
            | BinaryOp::Rem
            | BinaryOp::BitAnd
            | BinaryOp::BitOr
            | BinaryOp::BitXor
    ) {
        return checked.map(RuntimeValue::I64).ok_or_else(|| {
            RuntimeError::new(
                RuntimeErrorKind::IntegerOverflow,
                "i64 arithmetic overflow",
                Some(s),
            )
        });
    }
    compare(op, a.cmp(&b))
        .map(RuntimeValue::Bool)
        .ok_or_else(|| invalid_binary(s))
}
fn compare(op: BinaryOp, o: std::cmp::Ordering) -> Option<bool> {
    Some(match op {
        BinaryOp::Equal => o.is_eq(),
        BinaryOp::NotEqual => !o.is_eq(),
        BinaryOp::Less => o.is_lt(),
        BinaryOp::LessEqual => !o.is_gt(),
        BinaryOp::Greater => o.is_gt(),
        BinaryOp::GreaterEqual => !o.is_lt(),
        _ => return None,
    })
}
fn valid_shift(count: i64, width: u32, span: Span) -> Result<u32, RuntimeError> {
    u32::try_from(count)
        .ok()
        .filter(|count| *count < width)
        .ok_or_else(|| {
            RuntimeError::new(
                RuntimeErrorKind::InvalidShift,
                format!("shift count must be between 0 and {}", width - 1),
                Some(span),
            )
        })
}
fn checked_left_shift(value: i64, count: i64, width: u32, span: Span) -> Result<i64, RuntimeError> {
    let count = valid_shift(count, width, span)?;
    let shifted = if width == 32 {
        i64::from((value as i32).wrapping_shl(count))
    } else {
        value.wrapping_shl(count)
    };
    if width == 32 {
        if ((shifted as i32) >> count) == value as i32 {
            Ok(shifted)
        } else {
            Err(RuntimeError::new(
                RuntimeErrorKind::IntegerOverflow,
                "i32 left shift overflow",
                Some(span),
            ))
        }
    } else {
        if (shifted >> count) == value {
            Ok(shifted)
        } else {
            Err(RuntimeError::new(
                RuntimeErrorKind::IntegerOverflow,
                "i64 left shift overflow",
                Some(span),
            ))
        }
    }
}
fn float32(op: BinaryOp, a: f32, b: f32, s: Span) -> Result<RuntimeValue, RuntimeError> {
    use RuntimeValue::F32;
    let value = match op {
        BinaryOp::Add => Some(F32(a + b)),
        BinaryOp::Sub => Some(F32(a - b)),
        BinaryOp::Mul => Some(F32(a * b)),
        BinaryOp::Div => Some(F32(a / b)),
        _ => float_compare(op, a as f64, b as f64).map(RuntimeValue::Bool),
    };
    value.ok_or_else(|| invalid_binary(s))
}
fn float64(op: BinaryOp, a: f64, b: f64, s: Span) -> Result<RuntimeValue, RuntimeError> {
    use RuntimeValue::F64;
    let value = match op {
        BinaryOp::Add => Some(F64(a + b)),
        BinaryOp::Sub => Some(F64(a - b)),
        BinaryOp::Mul => Some(F64(a * b)),
        BinaryOp::Div => Some(F64(a / b)),
        _ => float_compare(op, a, b).map(RuntimeValue::Bool),
    };
    value.ok_or_else(|| invalid_binary(s))
}

fn float_compare(op: BinaryOp, a: f64, b: f64) -> Option<bool> {
    Some(match op {
        BinaryOp::Equal => a == b,
        BinaryOp::NotEqual => a != b,
        BinaryOp::Less => a < b,
        BinaryOp::LessEqual => a <= b,
        BinaryOp::Greater => a > b,
        BinaryOp::GreaterEqual => a >= b,
        _ => return None,
    })
}
