use crate::{
    lexer::token::Span,
    parser::ast::{Constant, Expr, ExprKind, Function, Module, Statement, Type},
};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SemanticDiagnostic {
    pub code: &'static str,
    pub message: String,
    pub span: Span,
    pub expected: Option<String>,
    pub actual: Option<String>,
}
impl SemanticDiagnostic {
    fn new(code: &'static str, message: impl Into<String>, span: Span) -> Self {
        Self {
            code,
            message: message.into(),
            span,
            expected: None,
            actual: None,
        }
    }
    fn mismatch(span: Span, expected: Ty, actual: Ty) -> Self {
        Self {
            code: "E0301",
            message: format!("type mismatch: expected `{expected}`, found `{actual}`"),
            span,
            expected: Some(expected.to_string()),
            actual: Some(actual.to_string()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Ty {
    I8,
    I16,
    I32,
    I64,
    I128,
    ISize,
    U8,
    U16,
    U32,
    U64,
    U128,
    USize,
    F32,
    F64,
    Bool,
    Char,
    Str,
    Void,
    Never,
}
impl Ty {
    fn parse(t: &Type) -> Option<Self> {
        match t.0.as_str() {
            "i8" => Some(Self::I8),
            "i16" => Some(Self::I16),
            "i32" => Some(Self::I32),
            "i64" => Some(Self::I64),
            "i128" => Some(Self::I128),
            "isize" => Some(Self::ISize),
            "u8" => Some(Self::U8),
            "u16" => Some(Self::U16),
            "u32" => Some(Self::U32),
            "u64" => Some(Self::U64),
            "u128" => Some(Self::U128),
            "usize" => Some(Self::USize),
            "f32" => Some(Self::F32),
            "f64" => Some(Self::F64),
            "bool" => Some(Self::Bool),
            "char" => Some(Self::Char),
            "str" => Some(Self::Str),
            "void" => Some(Self::Void),
            "never" => Some(Self::Never),
            _ => None,
        }
    }
}
impl std::fmt::Display for Ty {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::I8 => "i8",
            Self::I16 => "i16",
            Self::I32 => "i32",
            Self::I64 => "i64",
            Self::I128 => "i128",
            Self::ISize => "isize",
            Self::U8 => "u8",
            Self::U16 => "u16",
            Self::U32 => "u32",
            Self::U64 => "u64",
            Self::U128 => "u128",
            Self::USize => "usize",
            Self::F32 => "f32",
            Self::F64 => "f64",
            Self::Bool => "bool",
            Self::Char => "char",
            Self::Str => "str",
            Self::Void => "void",
            Self::Never => "never",
        })
    }
}
impl Ty {
    fn is_integer(self) -> bool {
        matches!(
            self,
            Self::I8
                | Self::I16
                | Self::I32
                | Self::I64
                | Self::I128
                | Self::ISize
                | Self::U8
                | Self::U16
                | Self::U32
                | Self::U64
                | Self::U128
                | Self::USize
        )
    }
    fn is_signed_integer(self) -> bool {
        matches!(
            self,
            Self::I8 | Self::I16 | Self::I32 | Self::I64 | Self::I128 | Self::ISize
        )
    }
    fn integer_width(self, pointer_width: u8) -> Option<u32> {
        Some(match self {
            Self::I8 | Self::U8 => 8,
            Self::I16 | Self::U16 => 16,
            Self::I32 | Self::U32 => 32,
            Self::I64 | Self::U64 => 64,
            Self::ISize | Self::USize => u32::from(pointer_width),
            Self::I128 | Self::U128 => 128,
            _ => return None,
        })
    }
}
#[derive(Clone)]
struct Signature {
    params: Vec<Ty>,
    result: Ty,
}
#[derive(Clone)]
struct Variable {
    ty: Ty,
    mutable: bool,
    constant: Option<crate::ir::Constant>,
}

pub fn analyze(module: &Module) -> Vec<SemanticDiagnostic> {
    let mut copy = module.clone();
    resolve(&mut copy)
}

/// Resolves inferred local types in-place before lowering. This keeps type decisions out of the parser and IR backend.
pub fn resolve(module: &mut Module) -> Vec<SemanticDiagnostic> {
    resolve_for_target(module, crate::target::TargetSpec::WINDOWS_X86_64_MSVC)
}

pub fn resolve_for_target(
    module: &mut Module,
    target: crate::target::TargetSpec,
) -> Vec<SemanticDiagnostic> {
    let mut checker = Checker {
        functions: HashMap::new(),
        constants: HashMap::new(),
        constant_values: HashMap::new(),
        errors: Vec::new(),
        target,
    };
    for constant in &module.constants {
        if checker.constants.contains_key(&constant.name) {
            checker.errors.push(SemanticDiagnostic::new(
                "E0205",
                format!("duplicate constant `{}`", constant.name),
                constant.span,
            ));
            continue;
        }
        let ty = checker.resolve_type(&constant.ty, constant.span);
        checker.constants.insert(constant.name.clone(), ty);
    }
    for function in &module.functions {
        if checker.functions.contains_key(&function.name) {
            checker.errors.push(SemanticDiagnostic::new(
                "E0201",
                format!("duplicate function `{}`", function.name),
                function.span,
            ));
            continue;
        }
        let params = function
            .params
            .iter()
            .map(|p| checker.resolve_type(&p.ty, p.span))
            .collect();
        let result = function
            .return_type
            .as_ref()
            .map(|t| checker.resolve_type(t, function.span))
            .unwrap_or(Ty::Void);
        checker
            .functions
            .insert(function.name.clone(), Signature { params, result });
    }
    let cycles = constant_cycles(&module.constants);
    for constant in &module.constants {
        let expected = Ty::parse(&constant.ty).unwrap_or(Ty::Void);
        let actual = checker.expression(&constant.value, &[], Some(expected));
        checker.expect(expected, actual, constant.value.span);
        if cycles.contains(&constant.name) {
            checker.errors.push(SemanticDiagnostic::new(
                "E0702",
                format!(
                    "constant `{}` participates in a dependency cycle",
                    constant.name
                ),
                constant.span,
            ));
        } else if let Some(value) =
            eval_const_expression_with(&constant.value, &checker.constant_values)
        {
            check_const_value(&value, expected, constant.value.span, &mut checker.errors);
            checker.constant_values.insert(constant.name.clone(), value);
        } else {
            checker.errors.push(SemanticDiagnostic::new(
                "E0701",
                "constant initializer could not be evaluated; it may contain an unsupported operation or a forward reference",
                constant.value.span,
            ));
        }
    }
    for function in &mut module.functions {
        checker.check_function(function);
    }
    checker.errors
}

struct Checker {
    functions: HashMap<String, Signature>,
    constants: HashMap<String, Ty>,
    constant_values: HashMap<String, crate::ir::Constant>,
    errors: Vec<SemanticDiagnostic>,
    target: crate::target::TargetSpec,
}
impl Checker {
    fn resolve_type(&mut self, t: &Type, span: Span) -> Ty {
        Ty::parse(t).unwrap_or_else(|| {
            self.errors.push(SemanticDiagnostic::new(
                "E0101",
                format!("unsupported or unknown type `{}`", t.0),
                span,
            ));
            Ty::Void
        })
    }
    fn check_function(&mut self, f: &mut Function) {
        let Some(sig) = self.functions.get(&f.name).cloned() else {
            return;
        };
        let mut scopes = vec![HashMap::new()];
        for (p, ty) in f.params.iter().zip(sig.params) {
            if ty == Ty::Void {
                self.errors.push(SemanticDiagnostic::new(
                    "E0102",
                    "parameter cannot have type `void`",
                    p.span,
                ));
            }
            if ty == Ty::Never {
                self.errors.push(SemanticDiagnostic::new(
                    "E0102",
                    "parameter cannot have type `never`",
                    p.span,
                ));
            }
            if scopes[0]
                .insert(
                    p.name.clone(),
                    Variable {
                        ty,
                        mutable: false,
                        constant: None,
                    },
                )
                .is_some()
            {
                self.errors.push(SemanticDiagnostic::new(
                    "E0202",
                    format!("duplicate parameter `{}`", p.name),
                    p.span,
                ));
            }
        }
        for stmt in &mut f.body {
            self.statement(stmt, &mut scopes, sig.result);
        }
        if sig.result != Ty::Void && !self.block_terminates(&f.body) {
            self.errors.push(SemanticDiagnostic::new(
                "E0601",
                format!(
                    "function `{}` may finish without returning `{}`",
                    f.name, sig.result
                ),
                f.span,
            ));
        }
    }
    fn statement(
        &mut self,
        s: &mut Statement,
        scopes: &mut Vec<HashMap<String, Variable>>,
        ret: Ty,
    ) {
        match s {
            Statement::Let {
                name,
                ty,
                value,
                mutable,
                span,
            } => {
                let expected = ty.as_ref().map(|t| self.resolve_type(t, *span));
                if expected == Some(Ty::Void) {
                    self.errors.push(SemanticDiagnostic::new(
                        "E0102",
                        "variable cannot have type `void`",
                        *span,
                    ));
                }
                if expected == Some(Ty::Never) {
                    self.errors.push(SemanticDiagnostic::new(
                        "E0102",
                        "variable cannot have type `never`",
                        *span,
                    ));
                }
                let actual = self.expression(value, scopes, expected);
                if actual == Ty::Never && expected.is_none() {
                    self.errors.push(SemanticDiagnostic::new(
                        "E0102",
                        "cannot infer a stored value from a diverging expression",
                        value.span,
                    ));
                }
                if let Some(expected) = expected {
                    self.expect(expected, actual, value.span);
                } else {
                    *ty = Some(Type(actual.to_string()));
                }
                let scope = scopes.last_mut().unwrap();
                if scope.contains_key(name) {
                    self.errors.push(SemanticDiagnostic::new(
                        "E0203",
                        format!("duplicate declaration `{name}` in this scope"),
                        *span,
                    ));
                } else {
                    scope.insert(
                        name.clone(),
                        Variable {
                            ty: actual,
                            mutable: *mutable,
                            constant: None,
                        },
                    );
                }
            }
            Statement::Const(c) => self.check_local_const(c, scopes),
            Statement::Assign { name, value, span } => {
                let found = scopes.iter().rev().find_map(|s| s.get(name)).cloned();
                let actual = self.expression(value, scopes, found.as_ref().map(|v| v.ty));
                match found {
                    None => self.errors.push(SemanticDiagnostic::new(
                        "E0401",
                        format!("undefined variable `{name}`"),
                        *span,
                    )),
                    Some(v) => {
                        if !v.mutable {
                            self.errors.push(SemanticDiagnostic::new(
                                "E0204",
                                format!("cannot assign to immutable variable `{name}`"),
                                *span,
                            ));
                        }
                        self.expect(v.ty, actual, value.span);
                    }
                }
            }
            Statement::Return(value, span) => {
                let actual = value
                    .as_ref()
                    .map(|v| self.expression(v, scopes, Some(ret)))
                    .unwrap_or(Ty::Void);
                self.expect(ret, actual, *span);
            }
            Statement::Expression(e, _) => {
                self.expression(e, scopes, None);
            }
            Statement::Block(body, _) => self.block(body, scopes, ret),
            Statement::If {
                condition,
                then_body,
                else_body,
                ..
            } => {
                let ty = self.expression(condition, scopes, Some(Ty::Bool));
                self.expect(Ty::Bool, ty, condition.span);
                self.block(then_body, scopes, ret);
                self.block(else_body, scopes, ret);
            }
            Statement::While {
                condition, body, ..
            } => {
                let ty = self.expression(condition, scopes, Some(Ty::Bool));
                self.expect(Ty::Bool, ty, condition.span);
                self.block(body, scopes, ret);
            }
        }
    }
    fn check_local_const(&mut self, c: &Constant, scopes: &mut Vec<HashMap<String, Variable>>) {
        let expected = self.resolve_type(&c.ty, c.span);
        let actual = self.expression(&c.value, scopes, Some(expected));
        self.expect(expected, actual, c.value.span);
        let mut const_env = self.constant_values.clone();
        for scope in scopes.iter() {
            for (name, variable) in scope {
                if let Some(value) = &variable.constant {
                    const_env.insert(name.clone(), value.clone());
                }
            }
        }
        let evaluated = eval_const_expression_with(&c.value, &const_env);
        if let Some(value) = &evaluated {
            check_const_value(value, expected, c.value.span, &mut self.errors);
        } else {
            self.errors.push(SemanticDiagnostic::new(
                "E0701",
                "constant initializer could not be evaluated at compile time",
                c.value.span,
            ));
        }
        let scope = scopes.last_mut().unwrap();
        if scope.contains_key(&c.name) {
            self.errors.push(SemanticDiagnostic::new(
                "E0203",
                format!("duplicate declaration `{}` in this scope", c.name),
                c.span,
            ));
        } else {
            scope.insert(
                c.name.clone(),
                Variable {
                    ty: expected,
                    mutable: false,
                    constant: evaluated,
                },
            );
        }
    }
    fn block(
        &mut self,
        body: &mut [Statement],
        scopes: &mut Vec<HashMap<String, Variable>>,
        ret: Ty,
    ) {
        scopes.push(HashMap::new());
        for s in body {
            self.statement(s, scopes, ret);
        }
        scopes.pop();
    }
    fn expect(&mut self, e: Ty, a: Ty, span: Span) {
        if e != a && a != Ty::Never {
            self.errors.push(SemanticDiagnostic::mismatch(span, e, a));
        }
    }
    fn expression_terminates(&self, expr: &Expr) -> bool {
        match &expr.kind {
            ExprKind::Call { callee, args } => {
                args.iter().any(|arg| self.expression_terminates(arg))
                    || match &callee.kind {
                        ExprKind::Name(name) => self
                            .functions
                            .get(name)
                            .is_some_and(|sig| sig.result == Ty::Never),
                        _ => self.expression_terminates(callee),
                    }
            }
            ExprKind::Cast { value, .. } | ExprKind::Unary { value, .. } => {
                self.expression_terminates(value)
            }
            ExprKind::Binary { left, op, right } if op == "&&" || op == "||" => {
                self.expression_terminates(left)
            }
            ExprKind::Binary { left, right, .. } => {
                self.expression_terminates(left) || self.expression_terminates(right)
            }
            _ => false,
        }
    }
    fn block_terminates(&self, body: &[Statement]) -> bool {
        body.iter().any(|statement| match statement {
            Statement::Return(_, _) => true,
            Statement::Block(nested, _) => self.block_terminates(nested),
            Statement::If {
                then_body,
                else_body,
                ..
            } => {
                !else_body.is_empty()
                    && self.block_terminates(then_body)
                    && self.block_terminates(else_body)
            }
            Statement::While {
                condition:
                    Expr {
                        kind: ExprKind::Bool(true),
                        ..
                    },
                ..
            } => true,
            Statement::Let { value, .. } | Statement::Expression(value, _) => {
                self.expression_terminates(value)
            }
            Statement::Assign { value, .. } => self.expression_terminates(value),
            Statement::Const(c) => self.expression_terminates(&c.value),
            Statement::While { .. } => false,
        })
    }
    fn expression(
        &mut self,
        e: &Expr,
        scopes: &[HashMap<String, Variable>],
        expected: Option<Ty>,
    ) -> Ty {
        match &e.kind {
            ExprKind::Name(name) => scopes
                .iter()
                .rev()
                .find_map(|s| s.get(name).map(|v| v.ty))
                .or_else(|| self.constants.get(name).copied())
                .unwrap_or_else(|| {
                    self.errors.push(SemanticDiagnostic::new(
                        "E0401",
                        format!("undefined value `{name}`"),
                        e.span,
                    ));
                    Ty::Void
                }),
            ExprKind::Integer(v) => {
                let Some(magnitude) = v.magnitude() else {
                    self.errors.push(SemanticDiagnostic::new(
                        "E0305",
                        format!("integer literal `{}` is outside the supported range", v.raw),
                        e.span,
                    ));
                    return Ty::I64;
                };
                let inferred = if magnitude <= i32::MAX as u128 {
                    Ty::I32
                } else if magnitude <= i64::MAX as u128 {
                    Ty::I64
                } else if magnitude <= i128::MAX as u128 {
                    Ty::I128
                } else {
                    Ty::U128
                };
                if let Some(expected) = expected.filter(|t| t.is_integer()) {
                    let width = expected
                        .integer_width(self.target.pointer_width)
                        .unwrap_or(64);
                    let max = integer_positive_max(expected.is_signed_integer(), width);
                    if magnitude > max {
                        self.errors.push(SemanticDiagnostic::new(
                            "E0305",
                            format!(
                                "integer literal `{}` is outside the range of `{expected}`",
                                v.raw
                            ),
                            e.span,
                        ));
                    }
                    expected
                } else if expected.is_some() {
                    let fallback = if magnitude <= i32::MAX as u128 {
                        Ty::I32
                    } else {
                        inferred
                    };
                    self.errors.push(SemanticDiagnostic::mismatch(
                        e.span,
                        expected.unwrap(),
                        fallback,
                    ));
                    fallback
                } else if inferred == Ty::U128 {
                    self.errors.push(SemanticDiagnostic::new(
                        "E0305",
                        format!(
                            "integer literal `{}` requires an explicit unsigned type",
                            v.raw
                        ),
                        e.span,
                    ));
                    Ty::U128
                } else {
                    inferred
                }
            }
            ExprKind::Float(_) => match expected {
                Some(t @ (Ty::F32 | Ty::F64)) => t,
                Some(t) => {
                    self.errors
                        .push(SemanticDiagnostic::mismatch(e.span, t, Ty::F64));
                    Ty::F64
                }
                None => Ty::F64,
            },
            ExprKind::String(_) => {
                if let Some(t) = expected {
                    self.expect(t, Ty::Str, e.span);
                }
                Ty::Str
            }
            ExprKind::Bool(_) => {
                if let Some(t) = expected {
                    self.expect(t, Ty::Bool, e.span);
                }
                Ty::Bool
            }
            ExprKind::Character(_) => {
                if let Some(t) = expected {
                    self.expect(t, Ty::Char, e.span);
                }
                Ty::Char
            }
            ExprKind::Call { callee, args } => {
                let Some(name) = (match &callee.kind {
                    ExprKind::Name(n) => Some(n.as_str()),
                    _ => None,
                }) else {
                    self.errors.push(SemanticDiagnostic::new(
                        "E0410",
                        "member call cannot be resolved in compiler 0.8 scalar scope",
                        callee.span,
                    ));
                    for a in args {
                        self.expression(a, scopes, None);
                    }
                    return Ty::Void;
                };
                if name == "println" {
                    if args.len() != 1 {
                        self.errors.push(SemanticDiagnostic::new(
                            "E0502",
                            "`println` expects exactly one argument",
                            e.span,
                        ));
                    }
                    for a in args {
                        self.expression(a, scopes, None);
                    }
                    return Ty::Void;
                }
                let Some(sig) = self.functions.get(name).cloned() else {
                    self.errors.push(SemanticDiagnostic::new(
                        "E0402",
                        format!("undefined function `{name}`"),
                        e.span,
                    ));
                    for a in args {
                        self.expression(a, scopes, None);
                    }
                    return Ty::Void;
                };
                if args.len() != sig.params.len() {
                    self.errors.push(SemanticDiagnostic::new(
                        "E0501",
                        format!(
                            "function `{name}` expects {} argument(s), found {}",
                            sig.params.len(),
                            args.len()
                        ),
                        e.span,
                    ));
                }
                for (a, expected) in args.iter().zip(&sig.params) {
                    let actual = self.expression(a, scopes, Some(*expected));
                    self.expect(*expected, actual, a.span);
                }
                sig.result
            }
            ExprKind::Binary { left, op, right } => {
                if op == "&&" || op == "||" {
                    let lhs = self.expression(left, scopes, Some(Ty::Bool));
                    let rhs = self.expression(right, scopes, Some(Ty::Bool));
                    self.expect(Ty::Bool, lhs, left.span);
                    self.expect(Ty::Bool, rhs, right.span);
                    if let Some(t) = expected {
                        self.expect(t, Ty::Bool, e.span);
                    }
                    return Ty::Bool;
                }
                let comparison = ["==", "!=", "<", "<=", ">", ">="].contains(&op.as_str());
                let bitwise = ["&", "|", "^", "<<", ">>"].contains(&op.as_str());
                let l = self.expression(left, scopes, if comparison { None } else { expected });
                let r = self.expression(right, scopes, Some(l));
                if comparison {
                    self.expect(l, r, right.span);
                    let valid = match op.as_str() {
                        "==" | "!=" => {
                            l.is_integer()
                                || matches!(l, Ty::F32 | Ty::F64 | Ty::Bool | Ty::Str | Ty::Char)
                        }
                        _ => l.is_integer() || matches!(l, Ty::F32 | Ty::F64),
                    };
                    if !valid {
                        self.errors.push(SemanticDiagnostic::new(
                            "E0303",
                            "comparison is not defined for these operands",
                            e.span,
                        ));
                    }
                    Ty::Bool
                } else {
                    self.expect(l, r, right.span);
                    if (!bitwise && !is_numeric(l)) || ((op == "%" || bitwise) && !l.is_integer()) {
                        self.errors.push(SemanticDiagnostic::new(
                            "E0302",
                            format!("operator `{op}` requires compatible integer operands"),
                            e.span,
                        ));
                    }
                    l
                }
            }
            ExprKind::Member { base, member } => {
                self.expression(base, scopes, None);
                self.errors.push(SemanticDiagnostic::new(
                    "E0410",
                    format!("member `{member}` cannot be resolved in compiler 0.8 scalar scope"),
                    e.span,
                ));
                Ty::Void
            }
            ExprKind::TupleIndex { base, index } => {
                self.expression(base, scopes, None);
                self.errors.push(SemanticDiagnostic::new("E0410", format!("tuple projection .{index} is parsed but tuple values are not implemented in compiler 0.8"), e.span));
                Ty::Void
            }
            ExprKind::Cast { value, target } => {
                let src = self.expression(value, scopes, None);
                let target_ty = self.resolve_type(target, e.span);
                if !is_numeric(src) || !is_numeric(target_ty) {
                    self.errors.push(SemanticDiagnostic::new(
                        "E0304",
                        format!("cannot cast `{src}` to `{target_ty}`"),
                        e.span,
                    ));
                } else if is_const_expr(value)
                    && const_cast(eval_const_expression(value).unwrap(), target_ty).is_none()
                {
                    self.errors.push(SemanticDiagnostic::new(
                        "E0304",
                        format!("constant value cannot be represented as `{target_ty}`"),
                        e.span,
                    ));
                }
                target_ty
            }
            ExprKind::Unary { op, value }
                if op == "-" && matches!(value.kind, ExprKind::Integer(_)) =>
            {
                let ExprKind::Integer(magnitude) = &value.kind else {
                    unreachable!()
                };
                let Some(magnitude) = magnitude.magnitude() else {
                    self.errors.push(SemanticDiagnostic::new(
                        "E0305",
                        "negative integer literal is outside the supported range",
                        e.span,
                    ));
                    return Ty::I64;
                };
                let inferred = if magnitude <= (i32::MAX as u128) + 1 {
                    Ty::I32
                } else if magnitude <= (i64::MAX as u128) + 1 {
                    Ty::I64
                } else if magnitude <= (i128::MAX as u128) + 1 {
                    Ty::I128
                } else {
                    self.errors.push(SemanticDiagnostic::new(
                        "E0305",
                        "negative integer literal is outside the range of every signed integer type",
                        e.span,
                    ));
                    return Ty::I128;
                };
                let inferred_width = inferred.integer_width(self.target.pointer_width).unwrap();
                let max_magnitude = integer_positive_max(true, inferred_width) + 1;
                if magnitude > max_magnitude {
                    self.errors.push(SemanticDiagnostic::new(
                        "E0305",
                        "negative integer literal is outside the supported range",
                        e.span,
                    ));
                    return inferred;
                }
                match expected {
                    Some(t) if t.is_signed_integer() => {
                        let expected_max = integer_positive_max(
                            true,
                            t.integer_width(self.target.pointer_width).unwrap(),
                        ) + 1;
                        if magnitude > expected_max {
                            self.errors.push(SemanticDiagnostic::new(
                                "E0305",
                                format!("negative integer literal is outside the range of `{t}`"),
                                e.span,
                            ));
                        }
                        t
                    }
                    Some(t) => {
                        self.errors
                            .push(SemanticDiagnostic::mismatch(e.span, t, inferred));
                        inferred
                    }
                    None => inferred,
                }
            }
            ExprKind::Unary { op, value } => {
                let ty = self.expression(value, scopes, expected);
                if op == "-" && !is_numeric(ty) {
                    self.errors.push(SemanticDiagnostic::new(
                        "E0302",
                        "unary `-` requires a numeric operand",
                        e.span,
                    ));
                } else if op == "!" && ty != Ty::Bool {
                    self.errors.push(SemanticDiagnostic::new(
                        "E0302",
                        "unary `!` requires a bool operand",
                        e.span,
                    ));
                } else if op != "-" && op != "!" {
                    self.errors.push(SemanticDiagnostic::new(
                        "E0302",
                        format!("unary operator `{op}` is not supported"),
                        e.span,
                    ));
                }
                if op == "!" {
                    Ty::Bool
                } else {
                    ty
                }
            }
        }
    }
}
fn is_numeric(t: Ty) -> bool {
    t.is_integer() || matches!(t, Ty::F32 | Ty::F64)
}

fn integer_positive_max(signed: bool, width: u32) -> u128 {
    if signed {
        if width == 128 {
            i128::MAX as u128
        } else {
            (1u128 << (width - 1)) - 1
        }
    } else if width == 128 {
        u128::MAX
    } else {
        (1u128 << width) - 1
    }
}
fn integer_mask(width: u32) -> u128 {
    if width == 128 {
        u128::MAX
    } else {
        (1u128 << width) - 1
    }
}
fn constant_cycles(constants: &[Constant]) -> HashSet<String> {
    fn refs(expr: &Expr, out: &mut HashSet<String>) {
        match &expr.kind {
            ExprKind::Name(name) => {
                out.insert(name.clone());
            }
            ExprKind::Call { callee, args } => {
                refs(callee, out);
                for arg in args {
                    refs(arg, out);
                }
            }
            ExprKind::Member { base, .. }
            | ExprKind::TupleIndex { base, .. }
            | ExprKind::Cast { value: base, .. }
            | ExprKind::Unary { value: base, .. } => refs(base, out),
            ExprKind::Binary { left, right, .. } => {
                refs(left, out);
                refs(right, out);
            }
            _ => {}
        }
    }
    fn visit(
        name: &str,
        graph: &HashMap<String, HashSet<String>>,
        state: &mut HashMap<String, u8>,
        stack: &mut Vec<String>,
        cycles: &mut HashSet<String>,
    ) {
        state.insert(name.to_string(), 1);
        stack.push(name.to_string());
        if let Some(deps) = graph.get(name) {
            for dep in deps {
                match state.get(dep).copied().unwrap_or(0) {
                    0 => visit(dep, graph, state, stack, cycles),
                    1 => {
                        if let Some(start) = stack.iter().position(|item| item == dep) {
                            cycles.extend(stack[start..].iter().cloned());
                        }
                    }
                    _ => {}
                }
            }
        }
        stack.pop();
        state.insert(name.to_string(), 2);
    }
    let names = constants
        .iter()
        .map(|c| c.name.clone())
        .collect::<HashSet<_>>();
    let graph = constants
        .iter()
        .map(|c| {
            let mut deps = HashSet::new();
            refs(&c.value, &mut deps);
            deps.retain(|name| names.contains(name));
            (c.name.clone(), deps)
        })
        .collect::<HashMap<_, _>>();
    let mut state = HashMap::new();
    let mut cycles = HashSet::new();
    for name in names {
        if state.get(&name).copied().unwrap_or(0) == 0 {
            visit(&name, &graph, &mut state, &mut Vec::new(), &mut cycles);
        }
    }
    cycles
}
fn is_const_expr(e: &Expr) -> bool {
    eval_const_expression(e).is_some()
}
fn check_const_value(
    value: &crate::ir::Constant,
    ty: Ty,
    span: Span,
    errors: &mut Vec<SemanticDiagnostic>,
) {
    let bits = match value {
        crate::ir::Constant::Integer(v) => *v as u128,
        crate::ir::Constant::IntegerBits(v) => *v,
        _ => return,
    };
    let width = ty.integer_width(64).unwrap();
    let valid = if ty.is_signed_integer() {
        let signed = if width == 128 {
            bits as i128
        } else {
            let mask = (1u128 << width) - 1;
            let value = bits & mask;
            if value & (1u128 << (width - 1)) == 0 {
                value as i128
            } else {
                (value | !mask) as i128
            }
        };
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
        signed >= min && signed <= max
    } else {
        width == 128 || bits < (1u128 << width)
    };
    if !valid {
        errors.push(SemanticDiagnostic::new(
            "E0305",
            format!("constant value is outside the range of `{ty}`"),
            span,
        ));
    }
}
pub(crate) fn eval_const_expression(e: &Expr) -> Option<crate::ir::Constant> {
    eval_const_expression_with(e, &HashMap::new())
}
pub(crate) fn eval_const_expression_with(
    e: &Expr,
    constants: &HashMap<String, crate::ir::Constant>,
) -> Option<crate::ir::Constant> {
    match &e.kind {
        ExprKind::Name(name) => constants.get(name).cloned(),
        ExprKind::Integer(v) => {
            let magnitude = v.magnitude()?;
            Some(if magnitude <= i64::MAX as u128 {
                crate::ir::Constant::Integer(magnitude as i64)
            } else {
                crate::ir::Constant::IntegerBits(magnitude)
            })
        }
        ExprKind::Float(v) => Some(crate::ir::Constant::Float(*v)),
        ExprKind::Bool(v) => Some(crate::ir::Constant::Bool(*v)),
        ExprKind::Character(v) => Some(crate::ir::Constant::Character(*v)),
        ExprKind::String(v) => Some(crate::ir::Constant::String(v.clone())),
        ExprKind::Unary { op, value } if op == "!" => {
            match eval_const_expression_with(value, constants)? {
                crate::ir::Constant::Bool(v) => Some(crate::ir::Constant::Bool(!v)),
                _ => None,
            }
        }
        ExprKind::Unary { op, value } if op == "-" => {
            match eval_const_expression_with(value, constants)? {
                crate::ir::Constant::Integer(v) => {
                    Some(crate::ir::Constant::Integer(v.checked_neg()?))
                }
                crate::ir::Constant::IntegerBits(v) => {
                    Some(crate::ir::Constant::IntegerBits(0u128.wrapping_sub(v)))
                }
                crate::ir::Constant::Float(v) => Some(crate::ir::Constant::Float(-v)),
                _ => None,
            }
        }
        ExprKind::Cast { value, target } => {
            let target = Ty::parse(target)?;
            const_cast(eval_const_expression_with(value, constants)?, target)
        }
        ExprKind::Binary { left, op, right } => {
            let a = eval_const_expression_with(left, constants)?;
            if op == "&&" {
                let crate::ir::Constant::Bool(a) = a else {
                    return None;
                };
                if !a {
                    return Some(crate::ir::Constant::Bool(false));
                }
                let crate::ir::Constant::Bool(b) = eval_const_expression_with(right, constants)?
                else {
                    return None;
                };
                return Some(crate::ir::Constant::Bool(b));
            }
            if op == "||" {
                let crate::ir::Constant::Bool(a) = a else {
                    return None;
                };
                if a {
                    return Some(crate::ir::Constant::Bool(true));
                }
                let crate::ir::Constant::Bool(b) = eval_const_expression_with(right, constants)?
                else {
                    return None;
                };
                return Some(crate::ir::Constant::Bool(b));
            }
            let b = eval_const_expression_with(right, constants)?;
            match (a, b) {
                (crate::ir::Constant::Integer(a), crate::ir::Constant::Integer(b)) => {
                    match op.as_str() {
                        "+" => Some(crate::ir::Constant::Integer(a.checked_add(b)?)),
                        "-" => Some(crate::ir::Constant::Integer(a.checked_sub(b)?)),
                        "*" => Some(crate::ir::Constant::Integer(a.checked_mul(b)?)),
                        "/" => Some(crate::ir::Constant::Integer(a.checked_div(b)?)),
                        "%" => Some(crate::ir::Constant::Integer(a.checked_rem(b)?)),
                        "&" => Some(crate::ir::Constant::Integer(a & b)),
                        "|" => Some(crate::ir::Constant::Integer(a | b)),
                        "^" => Some(crate::ir::Constant::Integer(a ^ b)),
                        "<<" => checked_const_shift(a, b, 64).map(crate::ir::Constant::Integer),
                        ">>" if (0..64).contains(&b) => Some(crate::ir::Constant::Integer(a >> b)),
                        "==" => Some(crate::ir::Constant::Bool(a == b)),
                        "!=" => Some(crate::ir::Constant::Bool(a != b)),
                        "<" => Some(crate::ir::Constant::Bool(a < b)),
                        "<=" => Some(crate::ir::Constant::Bool(a <= b)),
                        ">" => Some(crate::ir::Constant::Bool(a > b)),
                        ">=" => Some(crate::ir::Constant::Bool(a >= b)),
                        _ => None,
                    }
                }
                (crate::ir::Constant::Float(a), crate::ir::Constant::Float(b)) => match op.as_str()
                {
                    "+" => Some(crate::ir::Constant::Float(a + b)),
                    "-" => Some(crate::ir::Constant::Float(a - b)),
                    "*" => Some(crate::ir::Constant::Float(a * b)),
                    "/" => Some(crate::ir::Constant::Float(a / b)),
                    "==" => Some(crate::ir::Constant::Bool(a == b)),
                    "!=" => Some(crate::ir::Constant::Bool(a != b)),
                    "<" => Some(crate::ir::Constant::Bool(a < b)),
                    "<=" => Some(crate::ir::Constant::Bool(a <= b)),
                    ">" => Some(crate::ir::Constant::Bool(a > b)),
                    ">=" => Some(crate::ir::Constant::Bool(a >= b)),
                    _ => None,
                },
                (crate::ir::Constant::Bool(a), crate::ir::Constant::Bool(b)) => match op.as_str() {
                    "==" => Some(crate::ir::Constant::Bool(a == b)),
                    "!=" => Some(crate::ir::Constant::Bool(a != b)),
                    _ => None,
                },
                (crate::ir::Constant::String(a), crate::ir::Constant::String(b)) => {
                    match op.as_str() {
                        "==" => Some(crate::ir::Constant::Bool(a == b)),
                        "!=" => Some(crate::ir::Constant::Bool(a != b)),
                        _ => None,
                    }
                }
                _ => None,
            }
        }
        _ => None,
    }
}
fn const_cast(value: crate::ir::Constant, target: Ty) -> Option<crate::ir::Constant> {
    use crate::ir::Constant as C;
    match (value, target) {
        (C::Integer(v), t) if t.is_integer() => {
            let width = t.integer_width(64)?;
            let raw = if t.is_signed_integer() {
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
                let value = i128::from(v);
                if value < min || value > max {
                    return None;
                }
                (value as u128) & integer_mask(width)
            } else {
                if v < 0 || (v as u128) > integer_positive_max(false, width) {
                    return None;
                }
                (v as u128) & integer_mask(width)
            };
            Some(if matches!(t, Ty::I32 | Ty::I64) {
                C::Integer(v)
            } else {
                C::IntegerBits(raw)
            })
        }
        (C::IntegerBits(v), t) if t.is_integer() => {
            let width = t.integer_width(64)?;
            if t.is_signed_integer() && v > integer_positive_max(true, width) {
                return None;
            }
            if !t.is_signed_integer() && v > integer_positive_max(false, width) {
                return None;
            }
            let bits = v & integer_mask(width);
            Some(if matches!(t, Ty::I32 | Ty::I64) {
                C::Integer(v as i64)
            } else {
                C::IntegerBits(bits)
            })
        }
        (C::Integer(v), Ty::F32) => Some(C::Float((v as f32) as f64)),
        (C::Integer(v), Ty::F64) => Some(C::Float(v as f64)),
        (C::IntegerBits(v), Ty::F32) => Some(C::Float((v as f32) as f64)),
        (C::IntegerBits(v), Ty::F64) => Some(C::Float(v as f64)),
        (C::Float(v), t) if t.is_integer() => {
            let width = t.integer_width(64)?;
            let (lower, upper) = if t.is_signed_integer() {
                (
                    -2f64.powi((width - 1) as i32),
                    2f64.powi((width - 1) as i32),
                )
            } else {
                (0.0, 2f64.powi(width as i32))
            };
            if !v.is_finite() || v < lower || v >= upper {
                return None;
            }
            let bits = if t.is_signed_integer() {
                (v.trunc() as i128) as u128
            } else {
                v.trunc() as u128
            };
            let bits = bits & integer_mask(width);
            Some(if matches!(t, Ty::I32 | Ty::I64) {
                C::Integer(bits as i64)
            } else {
                C::IntegerBits(bits)
            })
        }
        (C::Float(v), Ty::I32) if v.is_finite() && v >= i32::MIN as f64 && v < 2_147_483_648.0 => {
            Some(C::Integer(i64::from(v.trunc() as i32)))
        }
        (C::Float(v), Ty::I64)
            if v.is_finite() && v >= i64::MIN as f64 && v < 9_223_372_036_854_775_808.0 =>
        {
            Some(C::Integer(v.trunc() as i64))
        }
        (C::Float(v), Ty::F32) => Some(C::Float((v as f32) as f64)),
        (C::Float(v), Ty::F64) => Some(C::Float(v)),
        _ => None,
    }
}
fn checked_const_shift(value: i64, count: i64, width: u32) -> Option<i64> {
    if count < 0 || count >= i64::from(width) {
        return None;
    }
    let count = count as u32;
    let shifted = value.checked_shl(count)?;
    ((shifted >> count) == value).then_some(shifted)
}
