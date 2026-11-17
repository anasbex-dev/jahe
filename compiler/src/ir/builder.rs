use super::{
    BasicBlock, BlockId, Diagnostic, Function, Instruction, InstructionKind, Module, Parameter,
    Terminator, Type, Value, ValueId,
};
use crate::lexer::token::Span;

pub struct ModuleBuilder {
    name: String,
    functions: Vec<Function>,
}
impl ModuleBuilder {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            functions: Vec::new(),
        }
    }
    pub fn add_function(&mut self, function: Function) -> Result<(), Diagnostic> {
        if self.functions.iter().any(|f| f.name == function.name) {
            return Err(Diagnostic {
                code: "IRB006",
                message: format!("duplicate function `{}`", function.name),
                span: function.span,
            });
        }
        self.functions.push(function);
        Ok(())
    }
    pub fn finish(self) -> Result<Module, Vec<Diagnostic>> {
        let module = Module {
            name: self.name,
            functions: self.functions,
        };
        super::validate::validate(&module)?;
        Ok(module)
    }
}

pub struct FunctionBuilder {
    pub function: Function,
    next_value: u32,
    current: Option<usize>,
}
impl FunctionBuilder {
    pub fn new(name: impl Into<String>, return_type: Type, span: Span) -> Self {
        let entry = BasicBlock {
            id: BlockId(0),
            name: "entry".into(),
            instructions: Vec::new(),
            terminator: None,
            span,
        };
        Self {
            function: Function {
                name: name.into(),
                params: Vec::new(),
                return_type,
                blocks: vec![entry],
                span,
            },
            next_value: 0,
            current: Some(0),
        }
    }
    pub fn create_value(&mut self, ty: Type, span: Span) -> Value {
        let v = Value {
            id: ValueId(self.next_value),
            ty,
            span,
        };
        self.next_value += 1;
        v
    }
    pub fn add_parameter(
        &mut self,
        name: impl Into<String>,
        ty: Type,
        span: Span,
    ) -> Result<Value, Diagnostic> {
        let name = name.into();
        if self.function.params.iter().any(|p| p.name == name) {
            return Err(Diagnostic {
                code: "IRB007",
                message: format!("duplicate parameter `{name}`"),
                span,
            });
        }
        let value = self.create_value(ty, span);
        self.function.params.push(Parameter {
            name,
            value: value.clone(),
        });
        Ok(value)
    }
    pub fn create_block(&mut self, name: impl Into<String>, span: Span) -> BlockId {
        let id = BlockId(self.function.blocks.len() as u32);
        self.function.blocks.push(BasicBlock {
            id,
            name: name.into(),
            instructions: Vec::new(),
            terminator: None,
            span,
        });
        id
    }
    pub fn position_at(&mut self, id: BlockId) -> Result<(), Diagnostic> {
        if let Some(i) = self.function.blocks.iter().position(|b| b.id == id) {
            self.current = Some(i);
            Ok(())
        } else {
            Err(Diagnostic {
                code: "IRB002",
                message: format!("unknown block {}", id.0),
                span: self.function.span,
            })
        }
    }
    pub fn emit(
        &mut self,
        kind: InstructionKind,
        result_type: Option<Type>,
        span: Span,
    ) -> Result<Option<Value>, Diagnostic> {
        let Some(i) = self.current else {
            return Err(Diagnostic {
                code: "IRB003",
                message: "no insertion block selected".into(),
                span,
            });
        };
        if self.function.blocks[i].terminator.is_some() {
            return Err(Diagnostic {
                code: "IRB004",
                message: "cannot emit instruction after terminator".into(),
                span,
            });
        }
        let result = result_type.map(|ty| self.create_value(ty, span));
        self.function.blocks[i].instructions.push(Instruction {
            result: result.clone(),
            kind,
            span,
        });
        Ok(result)
    }
    pub fn emit_phi(
        &mut self,
        ty: Type,
        incoming: Vec<(BlockId, Value)>,
        span: Span,
    ) -> Result<Value, Diagnostic> {
        self.emit(InstructionKind::Phi { incoming }, Some(ty), span)?
            .ok_or_else(|| Diagnostic {
                code: "IRB008",
                message: "phi did not produce a value".into(),
                span,
            })
    }
    pub fn terminate(&mut self, terminator: Terminator, span: Span) -> Result<(), Diagnostic> {
        let Some(i) = self.current else {
            return Err(Diagnostic {
                code: "IRB003",
                message: "no insertion block selected".into(),
                span,
            });
        };
        if self.function.blocks[i].terminator.is_some() {
            return Err(Diagnostic {
                code: "IRB005",
                message: "basic block already has a terminator".into(),
                span,
            });
        }
        self.function.blocks[i].terminator = Some(terminator);
        Ok(())
    }
    pub fn return_value(&mut self, value: Option<Value>, span: Span) -> Result<(), Diagnostic> {
        self.terminate(Terminator::Return(value), span)
    }
    pub fn branch(&mut self, target: BlockId, span: Span) -> Result<(), Diagnostic> {
        self.terminate(Terminator::Branch(target), span)
    }
    pub fn conditional_branch(
        &mut self,
        condition: Value,
        then_block: BlockId,
        else_block: BlockId,
        span: Span,
    ) -> Result<(), Diagnostic> {
        self.terminate(
            Terminator::ConditionalBranch {
                condition,
                then_block,
                else_block,
            },
            span,
        )
    }
    pub fn finish(self) -> Function {
        self.function
    }
}
