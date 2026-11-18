use crate::{
    lexer::token::Span,
    parser::ast::{Constant, Expr, ExprKind, Function, Module, Statement, Type},
};
use std::collections::HashMap;

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
    I32,
    I64,
    F32,
    F64,
    Bool,
    Str,
    Void,
}
impl Ty {
    fn parse(t: &Type) -> Option<Self> {
        match t.0.as_str() {
            "i32" => Some(Self::I32),
            "i64" => Some(Self::I64),
            "f32" => Some(Self::F32),
            "f64" => Some(Self::F64),
            "bool" => Some(Self::Bool),
            "str" => Some(Self::Str),
            "void" => Some(Self::Void),
            _ => None,
        }
    }
}
impl std::fmt::Display for Ty {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::I32 => "i32",
            Self::I64 => "i64",
            Self::F32 => "f32",
            Self::F64 => "f64",
            Self::Bool => "bool",
            Self::Str => "str",
            Self::Void => "void",
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
}

pub fn analyze(module: &Module) -> Vec<SemanticDiagnostic> {
    let mut copy = module.clone();
    resolve(&mut copy)
}

/// Resolves inferred local types in-place before lowering. This keeps type decisions out of the parser and IR backend.
pub fn resolve(module: &mut Module) -> Vec<SemanticDiagnostic> {
    let mut checker = Checker {
        functions: HashMap::new(),
        constants: HashMap::new(),
        errors: Vec::new(),
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
    for constant in &module.constants {
        let expected = Ty::parse(&constant.ty).unwrap_or(Ty::Void);
        let actual = checker.expression(&constant.value, &[], Some(expected));
        checker.expect(expected, actual, constant.value.span);
        if !is_const_expr(&constant.value) {
            checker.errors.push(SemanticDiagnostic::new(
                "E0701",
                "constant initializer must be a compile-time constant expression",
                constant.value.span,
            ));
        } else {
            check_const_range(&constant.value, expected, &mut checker.errors);
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
    errors: Vec<SemanticDiagnostic>,
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
            if scopes[0]
                .insert(p.name.clone(), Variable { ty, mutable: false })
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
        if sig.result != Ty::Void && !returns_on_all_paths(&f.body) {
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
                let actual = self.expression(value, scopes, expected);
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
        if !is_const_expr(&c.value) {
            self.errors.push(SemanticDiagnostic::new(
                "E0701",
                "constant initializer must be a compile-time constant expression",
                c.value.span,
            ));
        } else {
            check_const_range(&c.value, expected, &mut self.errors);
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
        if e != a {
            self.errors.push(SemanticDiagnostic::mismatch(span, e, a));
        }
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
                let inferred = if i32::try_from(*v).is_ok() {
                    Ty::I32
                } else {
                    Ty::I64
                };
                match expected {
                    Some(Ty::I32) if i32::try_from(*v).is_err() => {
                        self.errors.push(SemanticDiagnostic::new(
                            "E0305",
                            format!("integer literal `{v}` is outside the range of `i32`"),
                            e.span,
                        ));
                        Ty::I32
                    }
                    Some(t @ (Ty::I32 | Ty::I64)) => t,
                    Some(t) => {
                        self.errors
                            .push(SemanticDiagnostic::mismatch(e.span, t, inferred));
                        inferred
                    }
                    None => inferred,
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
                let comparison = ["==", "!=", "<", "<=", ">", ">="].contains(&op.as_str());
                let bitwise = ["&", "|", "^", "<<", ">>"].contains(&op.as_str());
                let l = self.expression(left, scopes, if comparison { None } else { expected });
                let r = self.expression(right, scopes, Some(l));
                if comparison {
                    self.expect(l, r, right.span);
                    let valid = match op.as_str() {
                        "==" | "!=" => matches!(
                            l,
                            Ty::I32 | Ty::I64 | Ty::F32 | Ty::F64 | Ty::Bool | Ty::Str
                        ),
                        _ => matches!(l, Ty::I32 | Ty::I64 | Ty::F32 | Ty::F64),
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
                    if (!bitwise && !matches!(l, Ty::I32 | Ty::I64 | Ty::F32 | Ty::F64))
                        || ((op == "%" || bitwise) && !matches!(l, Ty::I32 | Ty::I64))
                    {
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
                let ExprKind::Integer(magnitude) = value.kind else {
                    unreachable!()
                };
                let Some(negative) = magnitude.checked_neg() else {
                    self.errors.push(SemanticDiagnostic::new(
                        "E0305",
                        "negative integer literal is outside the supported range",
                        e.span,
                    ));
                    return Ty::I64;
                };
                let inferred = if i32::try_from(negative).is_ok() {
                    Ty::I32
                } else {
                    Ty::I64
                };
                match expected {
                    Some(t @ (Ty::I32 | Ty::I64)) => {
                        if t == Ty::I32 && i32::try_from(negative).is_err() {
                            self.errors.push(SemanticDiagnostic::new(
                                "E0305",
                                "negative integer literal is outside the range of `i32`",
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
                }
                ty
            }
        }
    }
}
fn is_numeric(t: Ty) -> bool {
    matches!(t, Ty::I32 | Ty::I64 | Ty::F32 | Ty::F64)
}
fn is_const_expr(e: &Expr) -> bool {
    eval_const_expression(e).is_some()
}
fn check_const_range(e: &Expr, ty: Ty, errors: &mut Vec<SemanticDiagnostic>) {
    if ty == Ty::I32
        && matches!(eval_const_expression(e), Some(crate::ir::Constant::Integer(v)) if i32::try_from(v).is_err())
    {
        errors.push(SemanticDiagnostic::new(
            "E0305",
            "constant value is outside the range of `i32`",
            e.span,
        ));
    }
}
pub(crate) fn eval_const_expression(e: &Expr) -> Option<crate::ir::Constant> {
    match &e.kind {
        ExprKind::Integer(v) => Some(crate::ir::Constant::Integer(*v)),
        ExprKind::Float(v) => Some(crate::ir::Constant::Float(*v)),
        ExprKind::Bool(v) => Some(crate::ir::Constant::Bool(*v)),
        ExprKind::String(v) => Some(crate::ir::Constant::String(v.clone())),
        ExprKind::Unary { op, value } if op == "-" => match eval_const_expression(value)? {
            crate::ir::Constant::Integer(v) => Some(crate::ir::Constant::Integer(v.checked_neg()?)),
            crate::ir::Constant::Float(v) => Some(crate::ir::Constant::Float(-v)),
            _ => None,
        },
        ExprKind::Cast { value, target } => {
            let target = Ty::parse(target)?;
            const_cast(eval_const_expression(value)?, target)
        }
        ExprKind::Binary { left, op, right } => {
            let a = eval_const_expression(left)?;
            let b = eval_const_expression(right)?;
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
        (C::Integer(v), Ty::I32) => Some(C::Integer(i64::from(i32::try_from(v).ok()?))),
        (C::Integer(v), Ty::I64) => Some(C::Integer(v)),
        (C::Integer(v), Ty::F32) => Some(C::Float((v as f32) as f64)),
        (C::Integer(v), Ty::F64) => Some(C::Float(v as f64)),
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
fn returns_on_all_paths(body: &[Statement]) -> bool {
    for statement in body {
        let returns = match statement {
            Statement::Return(_, _) => true,
            Statement::Block(nested, _) => returns_on_all_paths(nested),
            Statement::If {
                then_body,
                else_body,
                ..
            } => {
                !else_body.is_empty()
                    && returns_on_all_paths(then_body)
                    && returns_on_all_paths(else_body)
            }
            Statement::Let { .. }
            | Statement::Const(_)
            | Statement::Assign { .. }
            | Statement::Expression(_, _)
            | Statement::While { .. } => false,
        };
        if returns {
            return true;
        }
    }
    false
}
