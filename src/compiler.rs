use crate::ast::{Expr, Program, Resolved, Stmt};
use crate::chunk::{Bytecode, Chunk, Op};
use crate::token::TokenType;
use crate::value::Value;

/// AST to bytecode in one pass. Local slots come from `resolver.rs`, which has already
/// written them into the tree.
pub fn compile(program: &Program) -> Bytecode {
    let functions = program
        .functions
        .iter()
        .map(|f| {
            // Parameters are the frame's first locals, so argument `i` is already in
            // slot `i` when the body starts.
            let mut c = Compiler::new(Chunk::new(f.name.clone(), f.params.len()), program);
            c.in_function = true;
            c.statements(&f.body);
            c.finish(f.line)
        })
        .collect();

    let mut script = Compiler::new(Chunk::new("script".to_string(), 0), program);
    script.statements(&program.stmts);
    let line = script.chunk.last_line();
    Bytecode {
        script: script.finish(line),
        functions,
    }
}

struct Compiler<'a> {
    chunk: Chunk,
    program: &'a Program,
    /// A function body's declarations are locals even at its outermost level; the
    /// script's are globals.
    in_function: bool,
}

impl<'a> Compiler<'a> {
    fn new(chunk: Chunk, program: &'a Program) -> Self {
        Compiler {
            chunk,
            program,
            in_function: false,
        }
    }

    /// Every chunk ends by returning `nil`, so falling off the end of a function returns
    /// `nil` (spec section 4.3).
    fn finish(mut self, line: usize) -> Chunk {
        self.chunk.write(Op::Nil, line);
        self.chunk.write(Op::Return, line);
        self.chunk
    }

    fn statements(&mut self, stmts: &[Stmt]) {
        for stmt in stmts {
            self.statement(stmt);
        }
    }

    // --- statements -------------------------------------------------------------------

    fn statement(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::Expression(e) => {
                let line = self.expression(e);
                self.chunk.write(Op::Pop, line);
            }
            Stmt::Print(e) => {
                let line = self.expression(e);
                self.chunk.write(Op::Print, line);
            }
            Stmt::Var {
                name,
                initialiser,
                slot,
            } => {
                let line = match initialiser {
                    Some(e) => self.expression(e),
                    None => {
                        self.chunk.write(Op::Nil, name.line);
                        name.line
                    }
                };
                self.declare(&name.lexeme, *slot, line);
            }
            Stmt::Block { body, locals } => {
                self.statements(body);
                self.end_scope(*locals);
            }
            Stmt::Function { index, name, slot } => {
                let f = &self.program.functions[*index];
                let line = f.line;
                let k = self.chunk.constant(Value::Function {
                    index: *index,
                    name: name.clone(),
                });
                self.chunk.write(Op::Constant(k), line);
                self.declare(name, *slot, line);
            }
            Stmt::If {
                condition,
                then_branch,
                else_branch,
            } => {
                let cond_line = self.expression(condition);
                let jif = self.chunk.write(Op::JumpIfFalse(0), cond_line);
                self.chunk.write(Op::Pop, cond_line);

                self.statement(then_branch);

                let jmp = self.chunk.write(Op::Jump(0), cond_line);
                self.patch(jif);
                self.chunk.write(Op::Pop, cond_line);

                if let Some(else_stmt) = else_branch {
                    self.statement(else_stmt);
                }

                self.patch(jmp);
            }
            Stmt::While { condition, body } => {
                let start = self.chunk.code.len();
                let cond_line = self.expression(condition);
                let exit = self.chunk.write(Op::JumpIfFalse(0), cond_line);
                self.chunk.write(Op::Pop, cond_line);

                self.statement(body);

                let back_line = self.chunk.last_line();
                self.chunk.write(Op::Jump(start), back_line);
                self.patch(exit);
                self.chunk.write(Op::Pop, cond_line);
            }
            Stmt::Return(v) => {
                let line = match v {
                    Some(e) => self.expression(e),
                    None => self.chunk.last_line(),
                };
                if v.is_none() {
                    self.chunk.write(Op::Nil, line);
                }
                self.chunk.write(Op::Return, line);
            }
        }
    }

    // --- expressions ------------------------------------------------------------------

    /// Emit `expr`, and return the line for the caller's next instruction.
    fn expression(&mut self, expr: &Expr) -> usize {
        match expr {
            Expr::Literal { value, line } => {
                match value {
                    Value::Nil => self.chunk.write(Op::Nil, *line),
                    Value::Bool(true) => self.chunk.write(Op::True, *line),
                    Value::Bool(false) => self.chunk.write(Op::False, *line),
                    _ => {
                        let k = self.chunk.constant(value.clone());
                        self.chunk.write(Op::Constant(k), *line)
                    }
                };
                *line
            }
            Expr::Grouping(inner) => self.expression(inner),
            Expr::Variable { name, at } => {
                let line = name.line;
                match at {
                    Resolved::Local { slot, .. } => {
                        self.chunk.write(Op::GetLocal(*slot), line);
                    }
                    Resolved::Global => {
                        let k = self.name_constant(&name.lexeme);
                        self.chunk.write(Op::GetGlobal(k), line);
                    }
                }
                line
            }
            Expr::Assign { name, value, at } => {
                self.expression(value);
                let line = name.line;
                match at {
                    Resolved::Local { slot, .. } => {
                        self.chunk.write(Op::SetLocal(*slot), line);
                    }
                    Resolved::Global => {
                        let k = self.name_constant(&name.lexeme);
                        self.chunk.write(Op::SetGlobal(k), line);
                    }
                }
                line
            }
            Expr::Unary { op, right } => {
                self.expression(right);
                let line = op.line;
                match op.kind {
                    TokenType::Minus => {
                        self.chunk.write(Op::Negate, line);
                    }
                    TokenType::Bang => {
                        self.chunk.write(Op::Not, line);
                    }
                    _ => unreachable!(),
                }
                line
            }
            Expr::Binary { left, op, right } => {
                self.expression(left);
                self.expression(right);
                let line = op.line;
                let instr = match op.kind {
                    TokenType::Plus => Op::Add,
                    TokenType::Minus => Op::Subtract,
                    TokenType::Star => Op::Multiply,
                    TokenType::Slash => Op::Divide,
                    TokenType::Greater => Op::Greater,
                    TokenType::GreaterEqual => Op::GreaterEqual,
                    TokenType::Less => Op::Less,
                    TokenType::LessEqual => Op::LessEqual,
                    TokenType::EqualEqual => Op::Equal,
                    TokenType::BangEqual => Op::NotEqual,
                    _ => unreachable!(),
                };
                self.chunk.write(instr, line);
                line
            }
            Expr::Logical { left, op, right } => {
                let line = self.expression(left);
                match op.kind {
                    TokenType::Or => {
                        let jif = self.chunk.write(Op::JumpIfFalse(0), line);
                        let end = self.chunk.write(Op::Jump(0), line);
                        self.patch(jif);
                        self.chunk.write(Op::Pop, line);
                        self.expression(right);
                        self.patch(end);
                    }
                    TokenType::And => {
                        let end = self.chunk.write(Op::JumpIfFalse(0), line);
                        self.chunk.write(Op::Pop, line);
                        self.expression(right);
                        self.patch(end);
                    }
                    _ => unreachable!(),
                }
                line
            }
            Expr::Call {
                callee,
                paren,
                args,
            } => {
                self.expression(callee);
                for arg in args {
                    self.expression(arg);
                }
                let line = paren.line;
                self.chunk.write(Op::Call(args.len()), line);
                line
            }
        }
    }

    // --- names ------------------------------------------------------------------------

    /// A global is written into the globals map. A local is already on the stack in the
    /// slot the resolver gave it, so declaring one emits nothing.
    fn declare(&mut self, name: &str, slot: Option<usize>, line: usize) {
        if slot.is_none() {
            let k = self.name_constant(name);
            self.chunk.write(Op::DefineGlobal(k), line);
        }
    }

    fn name_constant(&mut self, name: &str) -> usize {
        self.chunk.constant(Value::Str(name.to_string()))
    }

    /// Leaving a scope pops its locals, one `Pop` each; the count is on the `Block`.
    fn end_scope(&mut self, locals: usize) {
        let line = self.chunk.last_line();
        for _ in 0..locals {
            self.chunk.write(Op::Pop, line);
        }
    }

    fn patch(&mut self, at: usize) {
        let target = self.chunk.code.len();
        match &mut self.chunk.code[at] {
            Op::Jump(dst) | Op::JumpIfFalse(dst) => *dst = target,
            _ => unreachable!(),
        }
    }
}