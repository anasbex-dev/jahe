use crate::{
    lexer::token::Span,
    parser::ast::{Expr, ExprKind, Function, Module, Statement, Type},
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
    fn mismatch(span: Span, expected: &str, actual: &str) -> Self {
        Self {
            code: "E0301",
            message: format!("type mismatch: expected `{expected}`, found `{actual}`"),
            span,
            expected: Some(expected.into()),
            actual: Some(actual.into()),
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
    let mut c = Checker {
        functions: HashMap::new(),
        errors: Vec::new(),
    };
    for f in &module.functions {
        if c.functions.contains_key(&f.name) {
            c.errors.push(SemanticDiagnostic::new(
                "E0201",
                format!("duplicate function `{}`", f.name),
                f.span,
            ));
            continue;
        }
        let params = f
            .params
            .iter()
            .map(|p| c.resolve_type(&p.ty, p.span))
            .collect();
        let result = f
            .return_type
            .as_ref()
            .map(|t| c.resolve_type(t, f.span))
            .unwrap_or(Ty::Void);
        c.functions
            .insert(f.name.clone(), Signature { params, result });
    }
    for f in &module.functions {
        c.check_function(f);
    }
    c.errors
}
struct Checker {
    functions: HashMap<String, Signature>,
    errors: Vec<SemanticDiagnostic>,
}
impl Checker {
    fn resolve_type(&mut self, t: &Type, span: Span) -> Ty {
        Ty::parse(t).unwrap_or_else(|| {
            self.errors.push(SemanticDiagnostic::new(
                "E0101",
                format!("unknown type `{}`", t.0),
                span,
            ));
            Ty::Void
        })
    }
    fn check_function(&mut self, f: &Function) {
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
        for s in &f.body {
            self.statement(s, &mut scopes, sig.result);
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
    fn statement(&mut self, s: &Statement, scopes: &mut Vec<HashMap<String, Variable>>, ret: Ty) {
        match s {
            Statement::Let {
                name,
                ty,
                value,
                mutable,
                span,
            } => {
                let declared = self.resolve_type(ty, *span);
                if declared == Ty::Void {
                    self.errors.push(SemanticDiagnostic::new(
                        "E0102",
                        "variable cannot have type `void`",
                        *span,
                    ));
                }
                let actual = self.expression(value, scopes);
                self.expect(&declared, &actual, value.span);
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
                            ty: declared,
                            mutable: *mutable,
                        },
                    );
                }
            }
            Statement::Assign { name, value, span } => {
                let actual = self.expression(value, scopes);
                let found = scopes.iter().rev().find_map(|s| s.get(name)).cloned();
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
                        self.expect(&v.ty, &actual, value.span);
                    }
                }
            }
            Statement::Return(value, span) => {
                let actual = value
                    .as_ref()
                    .map(|v| self.expression(v, scopes))
                    .unwrap_or(Ty::Void);
                self.expect(&ret, &actual, *span);
            }
            Statement::Expression(e, _) => {
                self.expression(e, scopes);
            }
            Statement::Block(body, _) => {
                self.block(body, scopes, ret);
            }
            Statement::If {
                condition,
                then_body,
                else_body,
                ..
            } => {
                let ty = self.expression(condition, scopes);
                self.expect(&Ty::Bool, &ty, condition.span);
                self.block(then_body, scopes, ret);
                self.block(else_body, scopes, ret);
            }
            Statement::While {
                condition, body, ..
            } => {
                let ty = self.expression(condition, scopes);
                self.expect(&Ty::Bool, &ty, condition.span);
                self.block(body, scopes, ret);
            }
        }
    }
    fn block(&mut self, body: &[Statement], scopes: &mut Vec<HashMap<String, Variable>>, ret: Ty) {
        scopes.push(HashMap::new());
        for s in body {
            self.statement(s, scopes, ret);
        }
        scopes.pop();
    }
    fn expect(&mut self, e: &Ty, a: &Ty, span: Span) {
        if e != a {
            self.errors.push(SemanticDiagnostic::mismatch(
                span,
                &e.to_string(),
                &a.to_string(),
            ));
        }
    }
    fn expression(&mut self, e: &Expr, scopes: &[HashMap<String, Variable>]) -> Ty {
        match &e.kind {
            ExprKind::Name(name) => scopes
                .iter()
                .rev()
                .find_map(|s| s.get(name).map(|v| v.ty))
                .unwrap_or_else(|| {
                    self.errors.push(SemanticDiagnostic::new(
                        "E0401",
                        format!("undefined variable `{name}`"),
                        e.span,
                    ));
                    Ty::Void
                }),
            ExprKind::Integer(v) => {
                if i32::try_from(*v).is_ok() {
                    Ty::I32
                } else {
                    Ty::I64
                }
            }
            ExprKind::Float(_) => Ty::F64,
            ExprKind::String(_) => Ty::Str,
            ExprKind::Bool(_) => Ty::Bool,
            ExprKind::Call { callee, args } => {
                if callee == "println" {
                    if args.len() != 1 {
                        self.errors.push(SemanticDiagnostic::new(
                            "E0502",
                            "`println` expects exactly one argument",
                            e.span,
                        ));
                    }
                    for a in args {
                        self.expression(a, scopes);
                    }
                    return Ty::Void;
                }
                let Some(sig) = self.functions.get(callee).cloned() else {
                    self.errors.push(SemanticDiagnostic::new(
                        "E0402",
                        format!("undefined function `{callee}`"),
                        e.span,
                    ));
                    for a in args {
                        self.expression(a, scopes);
                    }
                    return Ty::Void;
                };
                if args.len() != sig.params.len() {
                    self.errors.push(SemanticDiagnostic::new(
                        "E0501",
                        format!(
                            "function `{callee}` expects {} argument(s), found {}",
                            sig.params.len(),
                            args.len()
                        ),
                        e.span,
                    ));
                }
                for (a, expected) in args.iter().zip(&sig.params) {
                    let actual = self.expression(a, scopes);
                    self.expect(expected, &actual, a.span);
                }
                sig.result
            }
            ExprKind::Binary { left, op, right } => {
                let l = self.expression(left, scopes);
                let r = self.expression(right, scopes);
                if ["+", "-", "*", "/"].contains(&op.as_str()) {
                    if !matches!(l, Ty::I32 | Ty::I64 | Ty::F32 | Ty::F64) {
                        self.errors.push(SemanticDiagnostic::new(
                            "E0302",
                            format!("operator `{op}` requires numeric operands"),
                            e.span,
                        ));
                    }
                    self.expect(&l, &r, right.span);
                    l
                } else {
                    self.expect(&l, &r, right.span);
                    if !matches!(
                        l,
                        Ty::I32 | Ty::I64 | Ty::F32 | Ty::F64 | Ty::Bool | Ty::Str
                    ) {
                        self.errors.push(SemanticDiagnostic::new(
                            "E0303",
                            "comparison requires comparable operands",
                            e.span,
                        ));
                    }
                    Ty::Bool
                }
            }
        }
    }
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
