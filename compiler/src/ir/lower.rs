use super::{
    BasicBlock, BinaryOp, BlockId, Constant, Diagnostic, ExternalFunction, Function, Instruction,
    InstructionKind, Module, Parameter, ParameterType, Signature, Terminator, Type, UnaryOp, Value,
    ValueId,
};
use crate::{
    lexer::token::Span,
    parser::ast::{
        Expr, ExprKind, Function as AstFunction, Module as AstModule, Statement as AstStatement,
    },
    semantic,
};
use std::collections::HashMap;

pub fn lower(ast: &AstModule) -> Result<Module, Vec<Diagnostic>> {
    lower_for_target(ast, crate::target::TargetSpec::WINDOWS_X86_64_MSVC)
}

pub fn lower_for_target(
    ast: &AstModule,
    target: crate::target::TargetSpec,
) -> Result<Module, Vec<Diagnostic>> {
    if target != crate::target::TargetSpec::WINDOWS_X86_64_MSVC {
        return Err(vec![Diagnostic {
            code: "E0901",
            message: format!(
                "target `{}` is not implemented; only Windows x86_64 MSVC is supported",
                target.triple
            ),
            span: ast
                .functions
                .first()
                .map(|f| f.span)
                .unwrap_or(Span { line: 1, column: 1 }),
        }]);
    }
    let mut resolved = ast.clone();
    let sem = semantic::resolve_for_target(&mut resolved, target);
    if !sem.is_empty() {
        return Err(sem
            .into_iter()
            .map(|e| Diagnostic {
                code: e.code,
                message: e.message,
                span: e.span,
            })
            .collect());
    }
    let ast = &resolved;
    let sigs = ast
        .functions
        .iter()
        .map(|f| {
            (
                f.name.clone(),
                f.params.iter().map(|p| map_ty(&p.ty.0)).collect::<Vec<_>>(),
                f.return_type
                    .as_ref()
                    .map(|t| map_ty(&t.0))
                    .unwrap_or(Type::Void),
            )
        })
        .collect::<Vec<_>>();
    let mut errs = Vec::new();
    let mut constant_values = HashMap::new();
    let mut constants = HashMap::new();
    for c in &ast.constants {
        if let Some(value) = semantic::eval_const_expression_with(&c.value, &constant_values) {
            constant_values.insert(c.name.clone(), value.clone());
            constants.insert(c.name.clone(), (map_ty(&c.ty.0), value));
        }
    }
    let functions = ast
        .functions
        .iter()
        .map(|f| FunctionLowerer::new(f, &sigs, &constants, &mut errs, target).lower())
        .collect();
    if !errs.is_empty() {
        return Err(errs);
    }
    let m = Module {
        name: ast.name.clone(),
        externals: vec![ExternalFunction::new(
            "println",
            Signature {
                params: vec![ParameterType::AnyValue],
                return_type: Type::Void,
            },
            ast.functions
                .first()
                .map(|f| f.span)
                .unwrap_or(Span { line: 1, column: 1 }),
        )],
        functions,
    };
    Ok(m)
}
fn map_ty(s: &str) -> Type {
    match s {
        "i8" => Type::I8,
        "i16" => Type::I16,
        "i32" => Type::I32,
        "i64" => Type::I64,
        "i128" => Type::I128,
        "isize" => Type::ISize,
        "u8" => Type::U8,
        "u16" => Type::U16,
        "u32" => Type::U32,
        "u64" => Type::U64,
        "u128" => Type::U128,
        "usize" => Type::USize,
        "f32" => Type::F32,
        "f64" => Type::F64,
        "bool" => Type::Bool,
        "char" => Type::Char,
        "str" => Type::Str,
        "void" => Type::Void,
        "never" => Type::Never,
        _ => unreachable!("semantic analysis accepts only known types"),
    }
}
fn integer_mask(width: u32) -> u128 {
    if width == 128 {
        u128::MAX
    } else {
        (1u128 << width) - 1
    }
}
fn op(s: &str) -> BinaryOp {
    match s {
        "+" => BinaryOp::Add,
        "-" => BinaryOp::Sub,
        "*" => BinaryOp::Mul,
        "/" => BinaryOp::Div,
        "%" => BinaryOp::Rem,
        "&" => BinaryOp::BitAnd,
        "|" => BinaryOp::BitOr,
        "^" => BinaryOp::BitXor,
        "<<" => BinaryOp::ShiftLeft,
        ">>" => BinaryOp::ShiftRight,
        "==" => BinaryOp::Equal,
        "!=" => BinaryOp::NotEqual,
        "<" => BinaryOp::Less,
        "<=" => BinaryOp::LessEqual,
        ">" => BinaryOp::Greater,
        ">=" => BinaryOp::GreaterEqual,
        _ => unreachable!(),
    }
}
fn eval_constant(expr: &Expr, values: &HashMap<String, Constant>) -> Option<Constant> {
    semantic::eval_const_expression_with(expr, values)
}
#[derive(Clone)]
struct Binding {
    value: Value,
    mutable: bool,
    constant: Option<Constant>,
}
struct FunctionLowerer<'a> {
    ast: &'a AstFunction,
    signatures: &'a [(String, Vec<Type>, Type)],
    constants: &'a HashMap<String, (Type, Constant)>,
    errors: &'a mut Vec<Diagnostic>,
    blocks: Vec<BasicBlock>,
    current: Option<usize>,
    next_value: u32,
    scopes: Vec<HashMap<String, Binding>>,
    ret: Type,
    target: crate::target::TargetSpec,
}
impl<'a> FunctionLowerer<'a> {
    fn new(
        ast: &'a AstFunction,
        sigs: &'a [(String, Vec<Type>, Type)],
        constants: &'a HashMap<String, (Type, Constant)>,
        errors: &'a mut Vec<Diagnostic>,
        target: crate::target::TargetSpec,
    ) -> Self {
        let ret = ast
            .return_type
            .as_ref()
            .map(|t| map_ty(&t.0))
            .unwrap_or(Type::Void);
        let entry = BasicBlock {
            id: BlockId(0),
            name: "entry".into(),
            instructions: vec![],
            terminator: None,
            span: ast.span,
        };
        let mut x = Self {
            ast,
            signatures: sigs,
            constants,
            errors,
            blocks: vec![entry],
            current: Some(0),
            next_value: 0,
            scopes: vec![HashMap::new()],
            ret,
            target,
        };
        let ps = ast
            .params
            .iter()
            .map(|p| {
                let v = x.value(map_ty(&p.ty.0), p.span);
                Parameter {
                    name: p.name.clone(),
                    value: v,
                }
            })
            .collect::<Vec<_>>();
        for (p, v) in ast.params.iter().zip(ps) {
            x.scopes[0].insert(
                p.name.clone(),
                Binding {
                    value: v.value,
                    mutable: false,
                    constant: None,
                },
            );
        }
        x
    }
    fn lower(mut self) -> Function {
        for s in &self.ast.body {
            self.statement(s);
        }
        if let Some(i) = self.current {
            if self.blocks[i].terminator.is_none() {
                if self.ret == Type::Void {
                    self.blocks[i].terminator = Some(Terminator::Return(None));
                } else {
                    self.errors.push(Diagnostic {
                        code: "E0601",
                        message: format!(
                            "function `{}` may finish without returning `{}`",
                            self.ast.name, self.ret
                        ),
                        span: self.ast.span,
                    });
                }
            }
        }
        let params = self
            .ast
            .params
            .iter()
            .map(|p| {
                let b = self.scopes[0].get(&p.name).unwrap();
                Parameter {
                    name: p.name.clone(),
                    value: b.value.clone(),
                }
            })
            .collect();
        Function {
            name: self.ast.name.clone(),
            params,
            return_type: self.ret,
            blocks: self.blocks,
            span: self.ast.span,
        }
    }
    fn value(&mut self, ty: Type, span: Span) -> Value {
        let v = Value {
            id: ValueId(self.next_value),
            ty,
            span,
        };
        self.next_value += 1;
        v
    }
    fn block(&mut self, name: String, span: Span) -> BlockId {
        let id = BlockId(self.blocks.len() as u32);
        let name = format!("{name}.{}", id.0);
        self.blocks.push(BasicBlock {
            id,
            name,
            instructions: vec![],
            terminator: None,
            span,
        });
        id
    }
    fn index(&self, id: BlockId) -> usize {
        id.0 as usize
    }
    fn emit(&mut self, kind: InstructionKind, ty: Option<Type>, span: Span) -> Option<Value> {
        let result = ty.map(|t| self.value(t, span));
        if let Some(i) = self.current {
            self.blocks[i].instructions.push(Instruction {
                result: result.clone(),
                kind,
                span,
            });
        }
        result
    }
    fn terminate(&mut self, t: Terminator) {
        if let Some(i) = self.current {
            self.blocks[i].terminator = Some(t);
            self.current = None;
        }
    }
    fn branch_to(&mut self, to: BlockId) {
        self.terminate(Terminator::Branch(to));
    }
    fn binding(&self, name: &str) -> Option<Binding> {
        self.scopes.iter().rev().find_map(|s| s.get(name).cloned())
    }
    fn statement(&mut self, s: &AstStatement) {
        if self.current.is_none() {
            let span = match s {
                AstStatement::Let { span, .. }
                | AstStatement::Const(crate::parser::ast::Constant { span, .. })
                | AstStatement::Assign { span, .. }
                | AstStatement::Return(_, span)
                | AstStatement::Expression(_, span)
                | AstStatement::Block(_, span)
                | AstStatement::If { span, .. }
                | AstStatement::While { span, .. } => *span,
            };
            self.errors.push(Diagnostic {
                code: "E0602",
                message: "unreachable statement".into(),
                span,
            });
            return;
        }
        match s {
            AstStatement::Let {
                name,
                ty,
                value,
                mutable,
                span,
                ..
            } => {
                let expected = ty.as_ref().map(|ty| map_ty(&ty.0));
                if let Some(v) = self.expr_expected(value, expected) {
                    let b = Binding {
                        value: v,
                        mutable: *mutable,
                        constant: None,
                    };
                    self.scopes.last_mut().unwrap().insert(name.clone(), b);
                    let _ = span;
                }
            }
            AstStatement::Const(c) => {
                let mut const_env = self
                    .constants
                    .iter()
                    .map(|(name, (_, value))| (name.clone(), value.clone()))
                    .collect::<HashMap<_, _>>();
                for scope in &self.scopes {
                    for (name, binding) in scope {
                        if let Some(value) = &binding.constant {
                            const_env.insert(name.clone(), value.clone());
                        }
                    }
                }
                if let Some(constant) = eval_constant(&c.value, &const_env) {
                    let ty = map_ty(&c.ty.0);
                    if let Some(v) = self.emit(
                        InstructionKind::Constant(constant.clone()),
                        Some(ty),
                        c.span,
                    ) {
                        self.scopes.last_mut().unwrap().insert(
                            c.name.clone(),
                            Binding {
                                value: v,
                                mutable: false,
                                constant: Some(constant),
                            },
                        );
                    }
                } else {
                    self.errors.push(Diagnostic {
                        code: "E0701",
                        message: "constant expression could not be evaluated at compile time"
                            .into(),
                        span: c.value.span,
                    });
                }
            }
            AstStatement::Assign { name, value, .. } => {
                let binding = self.binding(name);
                let rhs = self.expr_expected(value, binding.as_ref().map(|b| b.value.ty));
                if let (Some(rhs), Some(binding)) = (rhs, binding) {
                    if !binding.mutable {
                        self.errors.push(Diagnostic {
                            code: "E0204",
                            message: format!("cannot assign immutable `{name}`"),
                            span: value.span,
                        });
                    } else if let Some(scope) =
                        self.scopes.iter_mut().rev().find(|s| s.contains_key(name))
                    {
                        scope.get_mut(name).unwrap().value = Value {
                            ty: binding.value.ty,
                            ..rhs
                        };
                    }
                }
            }
            AstStatement::Return(e, _) => {
                let v = e
                    .as_ref()
                    .and_then(|e| self.expr_expected(e, Some(self.ret)));
                self.terminate(Terminator::Return(v));
            }
            AstStatement::Expression(e, _) => {
                self.expr(e);
            }
            AstStatement::Block(body, _) => {
                self.scopes.push(HashMap::new());
                for s in body {
                    self.statement(s);
                }
                self.scopes.pop();
            }
            AstStatement::If {
                condition,
                then_body,
                else_body,
                span,
            } => self.if_stmt(condition, then_body, else_body, *span),
            AstStatement::While {
                condition,
                body,
                span,
            } => self.while_stmt(condition, body, *span),
        }
    }
    fn if_stmt(
        &mut self,
        condition: &Expr,
        then_body: &[AstStatement],
        else_body: &[AstStatement],
        span: Span,
    ) {
        let Some(cond) = self.expr_expected(condition, Some(Type::Bool)) else {
            return;
        };
        let pre = self.current.unwrap();
        let then_id = self.block("if.then".into(), span);
        let else_id = self.block("if.else".into(), span);
        let merge_id = self.block("if.merge".into(), span);
        self.current = Some(pre);
        self.terminate(Terminator::ConditionalBranch {
            condition: cond,
            then_block: then_id,
            else_block: else_id,
        });
        let base = self.scopes.clone();
        self.current = Some(self.index(then_id));
        self.scopes = base.clone();
        self.scopes.push(HashMap::new());
        for s in then_body {
            self.statement(s);
        }
        self.scopes.pop();
        let then_end = self.current.map(|i| BlockId(i as u32));
        if then_end.is_some() {
            self.branch_to(merge_id);
        }
        let then_env = self.scopes.clone();
        self.current = Some(self.index(else_id));
        self.scopes = base.clone();
        self.scopes.push(HashMap::new());
        for s in else_body {
            self.statement(s);
        }
        self.scopes.pop();
        let else_end = self.current.map(|i| BlockId(i as u32));
        if else_end.is_some() {
            self.branch_to(merge_id);
        }
        let else_env = self.scopes.clone();
        match (then_end, else_end) {
            (None, None) => {
                self.remove_block(merge_id);
                self.current = None;
                self.scopes = base;
            }
            (Some(_), None) => {
                self.current = Some(self.index(merge_id));
                self.scopes = then_env;
            }
            (None, Some(_)) => {
                self.current = Some(self.index(merge_id));
                self.scopes = else_env;
            }
            (Some(t), Some(e)) => {
                self.current = Some(self.index(merge_id));
                self.scopes = base;
                self.merge_mutables(&then_env, &else_env, t, e, span);
            }
        }
    }
    fn remove_block(&mut self, id: BlockId) {
        let index = self.index(id);
        self.blocks.remove(index);
        for block in &mut self.blocks {
            if block.id.0 > id.0 {
                block.id.0 -= 1;
            }
            match &mut block.terminator {
                Some(Terminator::Branch(target)) => {
                    if target.0 > id.0 {
                        target.0 -= 1;
                    }
                }
                Some(Terminator::ConditionalBranch {
                    then_block,
                    else_block,
                    ..
                }) => {
                    if then_block.0 > id.0 {
                        then_block.0 -= 1;
                    }
                    if else_block.0 > id.0 {
                        else_block.0 -= 1;
                    }
                }
                _ => {}
            }
            for instruction in &mut block.instructions {
                if let InstructionKind::Phi { incoming } = &mut instruction.kind {
                    for (pred, _) in incoming {
                        if pred.0 > id.0 {
                            pred.0 -= 1;
                        }
                    }
                }
            }
        }
    }
    fn merge_mutables(
        &mut self,
        a: &[HashMap<String, Binding>],
        b: &[HashMap<String, Binding>],
        pred_a: BlockId,
        pred_b: BlockId,
        span: Span,
    ) {
        for si in 0..self.scopes.len() {
            let names = self.scopes[si]
                .iter()
                .filter(|(_, v)| v.mutable)
                .map(|(n, _)| n.clone())
                .collect::<Vec<_>>();
            for n in names {
                let av = a.get(si).and_then(|s| s.get(&n)).map(|x| x.value.clone());
                let bv = b.get(si).and_then(|s| s.get(&n)).map(|x| x.value.clone());
                if let (Some(av), Some(bv)) = (av, bv) {
                    if av.id != bv.id {
                        let phi = self.value(av.ty, span);
                        let block = self.current.unwrap();
                        self.blocks[block].instructions.push(Instruction {
                            result: Some(phi.clone()),
                            kind: InstructionKind::Phi {
                                incoming: vec![(pred_a, av), (pred_b, bv)],
                            },
                            span,
                        });
                        self.scopes[si].get_mut(&n).unwrap().value = phi;
                    }
                }
            }
        }
    }
    fn while_stmt(&mut self, condition: &Expr, body: &[AstStatement], span: Span) {
        let pre = self.current.unwrap();
        let infinite = matches!(&condition.kind, ExprKind::Bool(true));
        let header = self.block("while.header".into(), span);
        let header_index = self.index(header);
        let body_id = self.block("while.body".into(), span);
        let exit = (!infinite).then(|| self.block("while.exit".into(), span));
        self.current = Some(pre);
        self.branch_to(header);
        self.current = Some(self.index(header));
        let base = self.scopes.clone();
        let mut phis = Vec::new();
        for (si, scope) in base.iter().enumerate() {
            for (name, b) in scope {
                if b.mutable {
                    let phi = self.value(b.value.ty, span);
                    self.blocks[header_index].instructions.push(Instruction {
                        result: Some(phi.clone()),
                        kind: InstructionKind::Phi {
                            incoming: vec![(BlockId(pre as u32), b.value.clone())],
                        },
                        span,
                    });
                    self.scopes[si].get_mut(name).unwrap().value = phi.clone();
                    phis.push((si, name.clone(), phi));
                }
            }
        }
        if infinite {
            self.branch_to(body_id);
        } else {
            let Some(cond) = self.expr_expected(condition, Some(Type::Bool)) else {
                return;
            };
            self.terminate(Terminator::ConditionalBranch {
                condition: cond,
                then_block: body_id,
                else_block: exit.unwrap(),
            });
        }
        let header_env = self.scopes.clone();
        self.current = Some(self.index(body_id));
        self.scopes = header_env.clone();
        self.scopes.push(HashMap::new());
        for s in body {
            self.statement(s);
        }
        self.scopes.pop();
        if let Some(latch_i) = self.current {
            let latch = BlockId(latch_i as u32);
            let out = self.scopes.clone();
            self.branch_to(header);
            for (si, name, phi) in phis {
                if let Some(v) = out
                    .get(si)
                    .and_then(|s| s.get(&name))
                    .map(|x| x.value.clone())
                {
                    if let Some(inst) =
                        self.blocks[header_index].instructions.iter_mut().find(|i| {
                            matches!(&i.kind, InstructionKind::Phi { .. })
                                && i.result.as_ref().is_some_and(|r| r.id == phi.id)
                        })
                    {
                        if let InstructionKind::Phi { incoming } = &mut inst.kind {
                            incoming.push((latch, v));
                        }
                    }
                }
            }
        }
        if let Some(exit) = exit {
            self.current = Some(self.index(exit));
            self.scopes = header_env;
        } else {
            self.current = None;
        }
    }
    fn expr(&mut self, e: &Expr) -> Option<Value> {
        self.expr_expected(e, None)
    }
    fn expr_expected(&mut self, e: &Expr, expected: Option<Type>) -> Option<Value> {
        let span = e.span;
        match &e.kind {
            ExprKind::Name(n) => self.binding(n).map(|b| b.value).or_else(|| {
                self.constants.get(n).cloned().and_then(|(ty, value)| {
                    self.emit(InstructionKind::Constant(value), Some(ty), span)
                })
            }),
            ExprKind::Integer(v) => {
                let Some(magnitude) = v.magnitude() else {
                    self.errors.push(Diagnostic {
                        code: "E0811",
                        message: format!("integer literal `{}` exceeds u128::MAX", v.raw),
                        span,
                    });
                    return None;
                };
                let inferred = if magnitude <= i32::MAX as u128 {
                    Type::I32
                } else if magnitude <= i64::MAX as u128 {
                    Type::I64
                } else if magnitude <= i128::MAX as u128 {
                    Type::I128
                } else {
                    Type::U128
                };
                let t = expected.filter(|t| t.is_integer()).unwrap_or(inferred);
                let width = t.integer_width(self.target.pointer_width).unwrap() as u32;
                let max = if t.is_signed_integer() {
                    if width == 128 {
                        i128::MAX as u128
                    } else {
                        (1u128 << (width - 1)) - 1
                    }
                } else if width == 128 {
                    u128::MAX
                } else {
                    (1u128 << width) - 1
                };
                if magnitude > max {
                    self.errors.push(Diagnostic {
                        code: "E0811",
                        message: format!(
                            "integer literal `{}` is outside the range of `{t}`",
                            v.raw
                        ),
                        span,
                    });
                    return None;
                }
                let constant = match t {
                    Type::I32 | Type::I64 if magnitude <= i64::MAX as u128 => {
                        Constant::Integer(magnitude as i64)
                    }
                    _ => Constant::IntegerBits(magnitude),
                };
                self.emit(InstructionKind::Constant(constant), Some(t), span)
            }
            ExprKind::Float(v) => self.emit(
                InstructionKind::Constant(Constant::Float(*v)),
                Some(
                    expected
                        .filter(|t| matches!(t, Type::F32 | Type::F64))
                        .unwrap_or(Type::F64),
                ),
                span,
            ),
            ExprKind::String(v) => self.emit(
                InstructionKind::Constant(Constant::String(v.clone())),
                Some(Type::Str),
                span,
            ),
            ExprKind::Bool(v) => self.emit(
                InstructionKind::Constant(Constant::Bool(*v)),
                Some(Type::Bool),
                span,
            ),
            ExprKind::Character(v) => self.emit(
                InstructionKind::Constant(Constant::Character(*v)),
                Some(Type::Char),
                span,
            ),
            ExprKind::Binary {
                left,
                op: oper,
                right,
            } => {
                if oper == "&&" || oper == "||" {
                    return self.short_circuit(left, right, oper == "&&", span);
                }
                let comparison = ["==", "!=", "<", "<=", ">", ">="].contains(&oper.as_str());
                let l = self.expr_expected(left, if comparison { None } else { expected })?;
                let r = self.expr_expected(right, Some(l.ty))?;
                let ty = if comparison { Type::Bool } else { l.ty };
                self.emit(
                    InstructionKind::Binary {
                        op: op(oper),
                        left: l,
                        right: r,
                    },
                    Some(ty),
                    span,
                )
            }
            ExprKind::Call { callee, args } => {
                let ExprKind::Name(function) = &callee.kind else {
                    self.errors.push(Diagnostic {
                        code: "E0810",
                        message: "unresolved member call cannot be lowered in scalar 0.8".into(),
                        span,
                    });
                    return None;
                };
                let signature = self.signatures.iter().find(|s| s.0 == *function).cloned();
                let values = args
                    .iter()
                    .enumerate()
                    .map(|(i, a)| {
                        self.expr_expected(a, signature.as_ref().and_then(|s| s.1.get(i).copied()))
                    })
                    .collect::<Option<Vec<_>>>()?;
                let ret = signature.map(|s| s.2).unwrap_or(Type::Void);
                let call = self.emit(
                    InstructionKind::Call {
                        function: function.clone(),
                        args: values,
                    },
                    if matches!(ret, Type::Void | Type::Never) {
                        None
                    } else {
                        Some(ret)
                    },
                    span,
                );
                if ret == Type::Never {
                    self.terminate(Terminator::Unreachable);
                    None
                } else {
                    call
                }
            }
            ExprKind::Member { member, .. } => {
                self.errors.push(Diagnostic {
                    code: "E0810",
                    message: format!("member `{member}` has no scalar resolution in compiler 0.8"),
                    span,
                });
                None
            }
            ExprKind::TupleIndex { index, .. } => {
                self.errors.push(Diagnostic {
                    code: "E0810",
                    message: format!(
                        "tuple projection .{index} is not implemented in compiler 0.8"
                    ),
                    span,
                });
                None
            }
            ExprKind::Cast { value, target } => {
                let source = self.expr_expected(value, None)?;
                let target = map_ty(&target.0);
                self.emit(
                    InstructionKind::Cast {
                        value: source,
                        target,
                    },
                    Some(target),
                    span,
                )
            }
            ExprKind::Unary { op: unary, value } if unary == "-" => {
                if let ExprKind::Integer(magnitude) = &value.kind {
                    let Some(magnitude) = magnitude.magnitude() else {
                        self.errors.push(Diagnostic {
                            code: "E0811",
                            message: "negative integer literal is outside the supported range"
                                .into(),
                            span,
                        });
                        return None;
                    };
                    let inferred = if magnitude <= (i32::MAX as u128) + 1 {
                        Type::I32
                    } else if magnitude <= (i64::MAX as u128) + 1 {
                        Type::I64
                    } else {
                        Type::I128
                    };
                    let ty = expected
                        .filter(|ty| ty.is_integer() && ty.is_signed_integer())
                        .unwrap_or(inferred);
                    let width = ty.integer_width(self.target.pointer_width).unwrap() as u32;
                    let abs_min = 1u128 << (width - 1);
                    if magnitude > abs_min {
                        self.errors.push(Diagnostic {
                            code: "E0811",
                            message: format!(
                                "negative integer literal is outside the range of `{ty}`"
                            ),
                            span,
                        });
                        return None;
                    }
                    let bits = (0u128.wrapping_sub(magnitude)) & integer_mask(width);
                    let constant = match ty {
                        Type::I32 if magnitude <= (i32::MAX as u128) + 1 => {
                            Constant::Integer((bits as u32 as i32) as i64)
                        }
                        Type::I64 if magnitude <= (i64::MAX as u128) + 1 => {
                            Constant::Integer(bits as i64)
                        }
                        _ => Constant::IntegerBits(bits),
                    };
                    return self.emit(InstructionKind::Constant(constant), Some(ty), span);
                }
                let Some(value) = self.expr_expected(value, expected) else {
                    return None;
                };
                if matches!(value.ty, Type::F32 | Type::F64) {
                    let ty = value.ty;
                    return self.emit(
                        InstructionKind::Unary {
                            op: UnaryOp::Negate,
                            value,
                        },
                        Some(ty),
                        span,
                    );
                }
                if !value.ty.is_integer() {
                    self.errors.push(Diagnostic {
                        code: "E0811",
                        message: "unary negation requires a numeric value".into(),
                        span,
                    });
                    return None;
                }
                let zero = self.emit(
                    InstructionKind::Constant(Constant::Integer(0)),
                    Some(value.ty),
                    span,
                )?;
                let ty = zero.ty;
                self.emit(
                    InstructionKind::Binary {
                        op: BinaryOp::Sub,
                        left: zero,
                        right: value,
                    },
                    Some(ty),
                    span,
                )
            }
            ExprKind::Unary { op: unary, value } if unary == "!" => {
                let value = self.expr_expected(value, Some(Type::Bool))?;
                let falsity = self.emit(
                    InstructionKind::Constant(Constant::Bool(false)),
                    Some(Type::Bool),
                    span,
                )?;
                self.emit(
                    InstructionKind::Binary {
                        op: BinaryOp::Equal,
                        left: value,
                        right: falsity,
                    },
                    Some(Type::Bool),
                    span,
                )
            }
            ExprKind::Unary { op: unary, .. } => {
                self.errors.push(Diagnostic {
                    code: "E0810",
                    message: format!("unary operator `{unary}` is not supported"),
                    span,
                });
                None
            }
        }
    }

    fn short_circuit(
        &mut self,
        left: &Expr,
        right: &Expr,
        is_and: bool,
        span: Span,
    ) -> Option<Value> {
        let lhs = self.expr_expected(left, Some(Type::Bool))?;
        let pre = self.current?;
        let rhs_id = self.block("logic.rhs".into(), span);
        let short_id = self.block("logic.short".into(), span);
        let merge_id = self.block("logic.merge".into(), span);
        self.current = Some(pre);
        self.terminate(Terminator::ConditionalBranch {
            condition: lhs,
            then_block: if is_and { rhs_id } else { short_id },
            else_block: if is_and { short_id } else { rhs_id },
        });
        self.current = Some(self.index(short_id));
        let short_value = self.emit(
            InstructionKind::Constant(Constant::Bool(!is_and)),
            Some(Type::Bool),
            span,
        )?;
        let short_end = BlockId(self.current? as u32);
        self.branch_to(merge_id);
        self.current = Some(self.index(rhs_id));
        let rhs_value = self.expr_expected(right, Some(Type::Bool))?;
        let rhs_end = BlockId(self.current? as u32);
        self.branch_to(merge_id);
        self.current = Some(self.index(merge_id));
        let result = self.value(Type::Bool, span);
        let merge_index = self.index(merge_id);
        self.blocks[merge_index].instructions.push(Instruction {
            result: Some(result.clone()),
            kind: InstructionKind::Phi {
                incoming: vec![(short_end, short_value), (rhs_end, rhs_value)],
            },
            span,
        });
        Some(result)
    }
}
