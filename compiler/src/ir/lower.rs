use super::{
    BasicBlock, BinaryOp, BlockId, Constant, Diagnostic, ExternalFunction, Function, Instruction,
    InstructionKind, Module, Parameter, ParameterType, Signature, Terminator, Type, Value, ValueId,
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
    let sem = semantic::analyze(ast);
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
    let functions = ast
        .functions
        .iter()
        .map(|f| FunctionLowerer::new(f, &sigs, &mut errs).lower())
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
        "i32" => Type::I32,
        "i64" => Type::I64,
        "f32" => Type::F32,
        "f64" => Type::F64,
        "bool" => Type::Bool,
        "str" => Type::Str,
        "void" => Type::Void,
        _ => unreachable!("semantic analysis accepts only known types"),
    }
}
fn op(s: &str) -> BinaryOp {
    match s {
        "+" => BinaryOp::Add,
        "-" => BinaryOp::Sub,
        "*" => BinaryOp::Mul,
        "/" => BinaryOp::Div,
        "==" => BinaryOp::Equal,
        "!=" => BinaryOp::NotEqual,
        "<" => BinaryOp::Less,
        "<=" => BinaryOp::LessEqual,
        ">" => BinaryOp::Greater,
        ">=" => BinaryOp::GreaterEqual,
        _ => unreachable!(),
    }
}
#[derive(Clone)]
struct Binding {
    value: Value,
    mutable: bool,
}
struct FunctionLowerer<'a> {
    ast: &'a AstFunction,
    signatures: &'a [(String, Vec<Type>, Type)],
    errors: &'a mut Vec<Diagnostic>,
    blocks: Vec<BasicBlock>,
    current: Option<usize>,
    next_value: u32,
    scopes: Vec<HashMap<String, Binding>>,
    ret: Type,
}
impl<'a> FunctionLowerer<'a> {
    fn new(
        ast: &'a AstFunction,
        sigs: &'a [(String, Vec<Type>, Type)],
        errors: &'a mut Vec<Diagnostic>,
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
            errors,
            blocks: vec![entry],
            current: Some(0),
            next_value: 0,
            scopes: vec![HashMap::new()],
            ret,
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
            } => {
                if let Some(v) = self.expr(value) {
                    let ty = map_ty(&ty.0);
                    let b = Binding {
                        value: Value { ty, ..v },
                        mutable: *mutable,
                    };
                    self.scopes.last_mut().unwrap().insert(name.clone(), b);
                    let _ = span;
                }
            }
            AstStatement::Assign { name, value, .. } => {
                let rhs = self.expr(value);
                if let (Some(rhs), Some(binding)) = (rhs, self.binding(name)) {
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
                let v = e.as_ref().and_then(|e| self.expr(e));
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
        let Some(cond) = self.expr(condition) else {
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
        let header = self.block("while.header".into(), span);
        let header_index = self.index(header);
        let body_id = self.block("while.body".into(), span);
        let exit = self.block("while.exit".into(), span);
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
        let Some(cond) = self.expr(condition) else {
            return;
        };
        let cond_block = self.current.unwrap();
        self.terminate(Terminator::ConditionalBranch {
            condition: cond,
            then_block: body_id,
            else_block: exit,
        });
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
        let _ = cond_block;
        self.current = Some(self.index(exit));
        self.scopes = header_env;
    }
    fn expr(&mut self, e: &Expr) -> Option<Value> {
        let span = e.span;
        match &e.kind {
            ExprKind::Name(n) => self.binding(n).map(|b| b.value),
            ExprKind::Integer(v) => {
                let t = if i32::try_from(*v).is_ok() {
                    Type::I32
                } else {
                    Type::I64
                };
                self.emit(
                    InstructionKind::Constant(Constant::Integer(*v)),
                    Some(t),
                    span,
                )
            }
            ExprKind::Float(v) => self.emit(
                InstructionKind::Constant(Constant::Float(*v)),
                Some(Type::F64),
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
            ExprKind::Binary {
                left,
                op: oper,
                right,
            } => {
                let l = self.expr(left)?;
                let r = self.expr(right)?;
                let ty = if ["==", "!=", "<", "<=", ">", ">="].contains(&oper.as_str()) {
                    Type::Bool
                } else {
                    l.ty
                };
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
                let values = args
                    .iter()
                    .map(|a| self.expr(a))
                    .collect::<Option<Vec<_>>>()?;
                let ret = self
                    .signatures
                    .iter()
                    .find(|s| s.0 == *callee)
                    .map(|s| s.2)
                    .unwrap_or(Type::Void);
                self.emit(
                    InstructionKind::Call {
                        function: callee.clone(),
                        args: values,
                    },
                    if ret == Type::Void { None } else { Some(ret) },
                    span,
                )
            }
        }
    }
}
