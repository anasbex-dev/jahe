use super::{
    control_flow, ssa, BinaryOp, BlockId, Constant, Diagnostic, ExternalFunction, Function,
    InstructionKind, Module, ParameterType, Signature, Terminator, Type, Value, ValueId,
};
use std::collections::{HashMap, HashSet, VecDeque};

fn integer_max(width: u32) -> u128 {
    if width == 128 {
        u128::MAX
    } else {
        (1u128 << width) - 1
    }
}
#[derive(Clone, Copy)]
struct Def {
    ty: Type,
    block: Option<BlockId>,
    index: usize,
}

pub fn validate(module: &Module) -> Result<(), Vec<Diagnostic>> {
    let mut errors = Vec::new();
    let mut sigs: HashMap<String, Signature> = HashMap::new();
    for external in &module.externals {
        if let Some(previous) = sigs.insert(external.name.clone(), external.signature.clone()) {
            let same = previous == external.signature;
            err(
                &mut errors,
                if same { "IR0040" } else { "IR0041" },
                format!(
                    "{} external declaration `{}`",
                    if same { "duplicate" } else { "conflicting" },
                    external.name
                ),
                external.span,
            );
        }
        for param in &external.signature.params {
            if matches!(param, ParameterType::Exact(Type::Void | Type::Never)) {
                err(
                    &mut errors,
                    "IR0042",
                    format!(
                        "external `{}` has a non-value parameter type",
                        external.name
                    ),
                    external.span,
                );
            }
        }
    }
    for function in &module.functions {
        if let Some(previous) = sigs.insert(
            function.name.clone(),
            Signature::exact(
                function.params.iter().map(|p| p.value.ty),
                function.return_type,
            ),
        ) {
            let code = if module.externals.iter().any(|e| e.name == function.name) {
                "IR0043"
            } else {
                "IR0001"
            };
            err(
                &mut errors,
                code,
                format!(
                    "function `{}` conflicts with an existing declaration",
                    function.name
                ),
                function.span,
            );
            let _ = previous;
        }
    }
    for function in &module.functions {
        validate_fn(function, &sigs, &mut errors);
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

pub fn validate_with_externals(
    module: &Module,
    externals: &HashMap<String, (Vec<Type>, Type)>,
) -> Result<(), Vec<Diagnostic>> {
    let mut combined = module.clone();
    let mut declarations = externals.iter().collect::<Vec<_>>();
    declarations.sort_by(|a, b| a.0.cmp(b.0));
    for (name, (params, return_type)) in declarations {
        let span = module
            .functions
            .first()
            .map(|f| f.span)
            .unwrap_or(crate::lexer::token::Span { line: 1, column: 1 });
        combined.externals.push(ExternalFunction::new(
            name,
            Signature::exact(params.iter().copied(), *return_type),
            span,
        ));
    }
    validate(&combined)
}
fn validate_fn(f: &Function, sigs: &HashMap<String, Signature>, e: &mut Vec<Diagnostic>) {
    if f.blocks.is_empty() {
        err(
            e,
            "IR0002",
            format!("function `{}` has no entry block", f.name),
            f.span,
        );
        return;
    }
    let mut blocks = HashMap::new();
    for b in &f.blocks {
        if blocks.insert(b.id, b).is_some() {
            err(e, "IR0003", "duplicate basic block id", b.span);
        }
    }
    if !blocks.contains_key(&BlockId(0)) {
        err(e, "IR0004", "entry block 0 is missing", f.span);
    }
    let mut defs: HashMap<ValueId, Def> = HashMap::new();
    let mut names = HashSet::new();
    for p in &f.params {
        if !names.insert(&p.name) {
            err(
                e,
                "IR0005",
                format!("duplicate parameter `{}`", p.name),
                p.value.span,
            );
        }
        if matches!(p.value.ty, Type::Void | Type::Never) {
            err(
                e,
                "IR0010",
                "parameter cannot have void or never type",
                p.value.span,
            );
        }
        insert_def(&mut defs, &p.value, None, 0, e);
    }
    for b in &f.blocks {
        for (i, inst) in b.instructions.iter().enumerate() {
            if let Some(v) = &inst.result {
                if matches!(v.ty, Type::Void | Type::Never) {
                    err(
                        e,
                        "IR0018",
                        "instruction result cannot have void or never type",
                        inst.span,
                    );
                }
                insert_def(&mut defs, v, Some(b.id), i, e);
            }
        }
    }
    let preds = control_flow::predecessors(f);
    let reachable = reachable(f);
    for b in &f.blocks {
        if !reachable.contains(&b.id) {
            err(
                e,
                "IR0022",
                format!("unreachable basic block `{}`", b.name),
                b.span,
            );
        }
    }
    let dom = dominators(f, &reachable, &preds);
    for b in &f.blocks {
        if b.terminator.is_none() {
            err(
                e,
                "IR0020",
                format!("basic block `{}` is missing a terminator", b.name),
                b.span,
            );
        }
        let mut non_phi = false;
        for (i, inst) in b.instructions.iter().enumerate() {
            let is_phi = matches!(inst.kind, InstructionKind::Phi { .. });
            if is_phi && non_phi {
                err(
                    e,
                    "IR0030",
                    "phi instructions must appear before non-phi instructions",
                    inst.span,
                );
            }
            if !is_phi {
                non_phi = true;
            }
            match &inst.kind {
                InstructionKind::Constant(c) => {
                    let valid = match (c, inst.result.as_ref().map(|v| v.ty)) {
                        (Constant::Integer(v), Some(Type::I32)) => i32::try_from(*v).is_ok(),
                        (Constant::Integer(_), Some(Type::I64)) => true,
                        (Constant::Integer(v), Some(ty)) if ty.is_integer() => {
                            let width = ty.integer_width(64).unwrap() as u32;
                            if ty.is_signed_integer() {
                                if width >= 64 {
                                    true
                                } else {
                                    let min = -(1i64 << (width - 1));
                                    let max = (1i64 << (width - 1)) - 1;
                                    (*v >= min) && (*v <= max)
                                }
                            } else {
                                *v >= 0 && (*v as u128) <= integer_max(width)
                            }
                        }
                        (Constant::IntegerBits(v), Some(ty)) if ty.is_integer() => {
                            let width = ty.integer_width(64).unwrap() as u32;
                            width == 128 || (*v >> width) == 0
                        }
                        (Constant::Float(v), Some(Type::F32)) => {
                            !v.is_finite() || v.abs() <= f32::MAX as f64
                        }
                        (Constant::Float(_), Some(Type::F64)) => true,
                        (Constant::String(_), Some(Type::Str)) => true,
                        (Constant::Bool(_), Some(Type::Bool)) => true,
                        (Constant::Character(c), Some(Type::Char)) => {
                            char::from_u32(*c as u32).is_some()
                        }
                        _ => false,
                    };
                    if !valid {
                        err(
                            e,
                            "IR0012",
                            "constant has invalid result type or value range",
                            inst.span,
                        );
                    }
                }
                InstructionKind::Binary { op, left, right } => {
                    check_use(left, b.id, i, &defs, &dom, e);
                    check_use(right, b.id, i, &defs, &dom, e);
                    let cmp = !matches!(
                        op,
                        BinaryOp::Add
                            | BinaryOp::Sub
                            | BinaryOp::Mul
                            | BinaryOp::Div
                            | BinaryOp::Rem
                            | BinaryOp::BitAnd
                            | BinaryOp::BitOr
                            | BinaryOp::BitXor
                            | BinaryOp::ShiftLeft
                            | BinaryOp::ShiftRight
                    );
                    let numeric = left.ty.is_integer() || matches!(left.ty, Type::F32 | Type::F64);
                    let comparable = matches!(
                        left.ty,
                        Type::I32
                            | Type::I64
                            | Type::F32
                            | Type::F64
                            | Type::Bool
                            | Type::Str
                            | Type::Char
                    );
                    let comparable = comparable || left.ty.is_integer();
                    let integer_only = matches!(
                        op,
                        BinaryOp::Rem
                            | BinaryOp::BitAnd
                            | BinaryOp::BitOr
                            | BinaryOp::BitXor
                            | BinaryOp::ShiftLeft
                            | BinaryOp::ShiftRight
                    );
                    let remainder_valid = !integer_only || left.ty.is_integer();
                    let valid = left.ty == right.ty
                        && if cmp {
                            comparable
                                && (left.ty != Type::Char
                                    || matches!(op, BinaryOp::Equal | BinaryOp::NotEqual))
                        } else {
                            numeric && remainder_valid
                        };
                    let result = if cmp { Type::Bool } else { left.ty };
                    if !valid || inst.result.as_ref().is_none_or(|v| v.ty != result) {
                        err(
                            e,
                            "IR0014",
                            "binary instruction has incompatible operands or result type",
                            inst.span,
                        );
                    }
                }
                InstructionKind::Unary { value, .. } => {
                    check_use(value, b.id, i, &defs, &dom, e);
                    if !matches!(value.ty, Type::F32 | Type::F64)
                        || inst.result.as_ref().is_none_or(|r| r.ty != value.ty)
                    {
                        err(
                            e,
                            "IR0045",
                            "floating negation has invalid operand or result type",
                            inst.span,
                        );
                    }
                }
                InstructionKind::Cast { value, target } => {
                    check_use(value, b.id, i, &defs, &dom, e);
                    let numeric = |ty: Type| ty.is_integer() || matches!(ty, Type::F32 | Type::F64);
                    if !numeric(value.ty)
                        || !numeric(*target)
                        || inst
                            .result
                            .as_ref()
                            .is_none_or(|result| result.ty != *target)
                    {
                        err(
                            e,
                            "IR0044",
                            "cast has invalid source or target type",
                            inst.span,
                        );
                    }
                }
                InstructionKind::Call { function, args } => {
                    for a in args {
                        check_use(a, b.id, i, &defs, &dom, e);
                    }
                    if let Some(signature) = sigs.get(function) {
                        let result = signature.return_type;
                        if args.len() != signature.params.len()
                            || args
                                .iter()
                                .zip(&signature.params)
                                .any(|(a, expected)| !expected.accepts(a.ty))
                            || (matches!(result, Type::Void | Type::Never)) != inst.result.is_none()
                            || inst.result.as_ref().is_some_and(|v| v.ty != result)
                        {
                            err(
                                e,
                                "IR0016",
                                format!(
                                    "call to `{function}` does not match declared signature {}",
                                    format_signature(signature)
                                ),
                                inst.span,
                            );
                        }
                    } else {
                        err(
                            e,
                            "IR0017",
                            format!("call references unknown function `{function}`"),
                            inst.span,
                        );
                    }
                }
                InstructionKind::Phi { incoming } => {
                    if inst.result.is_none() {
                        err(
                            e,
                            "IR0033",
                            "phi instruction must define a value",
                            inst.span,
                        );
                    }
                    let actual = preds.get(&b.id).cloned().unwrap_or_default();
                    if incoming.is_empty() || !ssa::phi_inputs_match_predecessors(incoming, &actual)
                    {
                        err(
                            e,
                            "IR0031",
                            "phi inputs must contain exactly one value for each predecessor",
                            inst.span,
                        );
                    }
                    let result_ty = inst.result.as_ref().map(|v| v.ty);
                    for (pred, v) in incoming {
                        if Some(v.ty) != result_ty {
                            err(
                                e,
                                "IR0032",
                                "phi incoming value type differs from phi result",
                                v.span,
                            );
                        }
                        check_phi_use(v, *pred, &defs, &dom, e);
                    }
                }
            }
        }
        match &b.terminator {
            Some(Terminator::Return(v)) => {
                if let Some(v) = v {
                    check_use(v, b.id, b.instructions.len(), &defs, &dom, e);
                }
                let ty = v.as_ref().map(|v| v.ty).unwrap_or(Type::Void);
                if ty != f.return_type {
                    err(
                        e,
                        "IR0021",
                        format!(
                            "return type mismatch: expected {}, found {ty}",
                            f.return_type
                        ),
                        v.as_ref().map(|x| x.span).unwrap_or(b.span),
                    );
                }
            }
            Some(Terminator::Branch(target)) => {
                if !blocks.contains_key(target) {
                    err(
                        e,
                        "IR0023",
                        format!("branch targets missing block {}", target.0),
                        b.span,
                    );
                }
            }
            Some(Terminator::ConditionalBranch {
                condition,
                then_block,
                else_block,
            }) => {
                check_use(condition, b.id, b.instructions.len(), &defs, &dom, e);
                if condition.ty != Type::Bool {
                    err(
                        e,
                        "IR0024",
                        "conditional branch condition must be bool",
                        condition.span,
                    );
                }
                for target in [then_block, else_block] {
                    if !blocks.contains_key(target) {
                        err(
                            e,
                            "IR0023",
                            format!("branch targets missing block {}", target.0),
                            b.span,
                        );
                    }
                }
            }
            Some(Terminator::Unreachable) => {}
            None => {}
        }
    }
}
fn format_signature(signature: &Signature) -> String {
    let params = signature
        .params
        .iter()
        .map(|p| match p {
            ParameterType::Exact(t) => t.to_string(),
            ParameterType::AnyValue => "any-value".into(),
        })
        .collect::<Vec<_>>()
        .join(", ");
    format!("({params}) -> {}", signature.return_type)
}
fn insert_def(
    m: &mut HashMap<ValueId, Def>,
    v: &Value,
    b: Option<BlockId>,
    i: usize,
    e: &mut Vec<Diagnostic>,
) {
    if m.insert(
        v.id,
        Def {
            ty: v.ty,
            block: b,
            index: i,
        },
    )
    .is_some()
    {
        err(
            e,
            "IR0011",
            format!("SSA value %{} has multiple definitions", v.id.0),
            v.span,
        );
    }
}
fn check_use(
    v: &Value,
    use_block: BlockId,
    use_index: usize,
    defs: &HashMap<ValueId, Def>,
    dom: &HashMap<BlockId, HashSet<BlockId>>,
    e: &mut Vec<Diagnostic>,
) {
    let Some(d) = defs.get(&v.id) else {
        err(
            e,
            "IR0019",
            format!("use of undefined SSA value %{}", v.id.0),
            v.span,
        );
        return;
    };
    if d.ty != v.ty {
        err(
            e,
            "IR0019",
            format!("SSA value %{} is used with the wrong type", v.id.0),
            v.span,
        );
    }
    if let Some(db) = d.block {
        if db == use_block {
            if d.index >= use_index {
                err(
                    e,
                    "IR0025",
                    format!("SSA value %{} is used before its definition", v.id.0),
                    v.span,
                );
            }
        } else if !dom.get(&use_block).is_some_and(|s| s.contains(&db)) {
            err(
                e,
                "IR0026",
                format!("definition of %{} does not dominate its use", v.id.0),
                v.span,
            );
        }
    }
}
fn check_phi_use(
    v: &Value,
    pred: BlockId,
    defs: &HashMap<ValueId, Def>,
    dom: &HashMap<BlockId, HashSet<BlockId>>,
    e: &mut Vec<Diagnostic>,
) {
    let Some(d) = defs.get(&v.id) else {
        err(
            e,
            "IR0019",
            format!("phi uses undefined value %{}", v.id.0),
            v.span,
        );
        return;
    };
    if d.ty != v.ty {
        err(e, "IR0019", "phi value used with inconsistent type", v.span);
    }
    if let Some(db) = d.block {
        if db != pred && !dom.get(&pred).is_some_and(|s| s.contains(&db)) {
            err(
                e,
                "IR0026",
                format!(
                    "phi input definition %{} does not dominate predecessor",
                    v.id.0
                ),
                v.span,
            );
        }
    }
}
fn reachable(f: &Function) -> HashSet<BlockId> {
    let mut seen = HashSet::new();
    let mut q = VecDeque::from([BlockId(0)]);
    while let Some(id) = q.pop_front() {
        if !seen.insert(id) {
            continue;
        }
        if let Some(b) = f.blocks.iter().find(|b| b.id == id) {
            match &b.terminator {
                Some(Terminator::Branch(t)) => q.push_back(*t),
                Some(Terminator::ConditionalBranch {
                    then_block,
                    else_block,
                    ..
                }) => {
                    q.push_back(*then_block);
                    q.push_back(*else_block);
                }
                _ => {}
            }
        }
    }
    seen
}
fn dominators(
    _f: &Function,
    reach: &HashSet<BlockId>,
    preds: &HashMap<BlockId, Vec<BlockId>>,
) -> HashMap<BlockId, HashSet<BlockId>> {
    let all = reach.clone();
    let mut d = HashMap::new();
    for b in reach {
        d.insert(
            *b,
            if *b == BlockId(0) {
                HashSet::from([*b])
            } else {
                all.clone()
            },
        );
    }
    loop {
        let mut changed = false;
        for b in reach.iter().filter(|b| **b != BlockId(0)) {
            let ps = preds
                .get(b)
                .into_iter()
                .flatten()
                .filter(|p| reach.contains(p))
                .collect::<Vec<_>>();
            let mut next = ps
                .first()
                .and_then(|p| d.get(p).cloned())
                .unwrap_or_default();
            for p in ps.iter().skip(1) {
                if let Some(x) = d.get(p) {
                    next = next.intersection(x).copied().collect();
                }
            }
            next.insert(*b);
            if d.get(b) != Some(&next) {
                d.insert(*b, next);
                changed = true;
            }
        }
        if !changed {
            break;
        }
    }
    d
}
fn err(
    e: &mut Vec<Diagnostic>,
    code: &'static str,
    message: impl Into<String>,
    span: crate::lexer::token::Span,
) {
    e.push(Diagnostic {
        code,
        message: message.into(),
        span,
    });
}
