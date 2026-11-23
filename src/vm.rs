use std::collections::HashMap;

use crate::chunk::{Bytecode, Chunk, Op};
// Shared with the tree-walker: both engines must print the same thing (spec section 6.1).
use crate::interpreter::{RuntimeError, MAX_FRAMES};
use crate::value::Value;

/// One active call: its chunk, how far through it is, and where its slice of the stack
/// begins. Nothing else, because Kobo has no closures.
struct Frame {
    /// An index into `Bytecode::functions`, or `None` for the script itself.
    function: Option<usize>,
    ip: usize,
    /// Stack index of local slot 0. The callee's own value sits just below it.
    base: usize,
}

/// Run compiled bytecode. The VM takes ownership; nothing needs the chunks afterwards.
pub fn run(bytecode: Bytecode) -> Result<(), RuntimeError> {
    let mut vm = Vm {
        bytecode,
        stack: Vec::new(),
        globals: HashMap::new(),
        frames: vec![Frame {
            function: None,
            ip: 0,
            base: 0,
        }],
    };
    vm.run()
}

struct Vm {
    bytecode: Bytecode,
    /// Values in flight and the locals of every active call, in one vector.
    stack: Vec<Value>,
    globals: HashMap<String, Value>,
    frames: Vec<Frame>,
}

impl Vm {
    fn run(&mut self) -> Result<(), RuntimeError> {
        // TODO(you): the dispatch loop: one match over Op. Spec 6.1 requires it to print
        //            exactly what the interpreter prints, down to the exit code.
        loop {
            let function = self.frame().function;
            let ip = self.frame().ip;
            let (op, line) = {
                let chunk = self.chunk(function);
                (chunk.code[ip], chunk.lines[ip])
            };
            self.frame_mut().ip += 1;

            match op {
                Op::Constant(i) => {
                    let val = self.chunk(function).constants[i].clone();
                    self.stack.push(val);
                }
                Op::Nil => self.stack.push(Value::Nil),
                Op::True => self.stack.push(Value::Bool(true)),
                Op::False => self.stack.push(Value::Bool(false)),
                Op::Pop => {
                    self.pop();
                }
                Op::DefineGlobal(i) => {
                    let name = self.name(function, i);
                    let value = self.pop();
                    self.globals.insert(name, value);
                }
                Op::GetGlobal(i) => {
                    let name = self.name(function, i);
                    match self.globals.get(&name) {
                        Some(val) => self.stack.push(val.clone()),
                        None => return Err(undefined(line, &name)),
                    }
                }
                Op::SetGlobal(i) => {
                    let name = self.name(function, i);
                    let val = self.peek().clone();
                    if self.globals.contains_key(&name) {
                        self.globals.insert(name, val);
                    } else {
                        return Err(undefined(line, &name));
                    }
                }
                Op::GetLocal(s) => {
                    let base = self.frame().base;
                    let val = self.stack[base + s].clone();
                    self.stack.push(val);
                }
                Op::SetLocal(s) => {
                    let base = self.frame().base;
                    let val = self.peek().clone();
                    self.stack[base + s] = val;
                }
                Op::Equal => self.binary_value(|a, b| Value::Bool(a == b)),
                Op::NotEqual => self.binary_value(|a, b| Value::Bool(a != b)),
                Op::Greater => self.compare(line, |a, b| a > b)?,
                Op::GreaterEqual => self.compare(line, |a, b| a >= b)?,
                Op::Less => self.compare(line, |a, b| a < b)?,
                Op::LessEqual => self.compare(line, |a, b| a <= b)?,
                Op::Add => {
                    let (a, b) = self.pop_two();
                    match (a, b) {
                        (Value::Number(x), Value::Number(y)) => {
                            self.stack.push(Value::Number(x + y));
                        }
                        (Value::Str(x), Value::Str(y)) => {
                            self.stack.push(Value::Str(format!("{}{}", x, y)));
                        }
                        _ => {
                            return Err(error(
                                line,
                                "Operands must be two numbers or two strings.",
                            ));
                        }
                    }
                }
                Op::Subtract => self.arith(line, |a, b| a - b)?,
                Op::Multiply => self.arith(line, |a, b| a * b)?,
                Op::Divide => {
                    let (a, b) = self.numbers(line)?;
                    if b == 0.0 {
                        return Err(error(line, "Division by zero."));
                    }
                    self.stack.push(Value::Number(a / b));
                }
                Op::Not => {
                    let truthy = self.pop().is_truthy();
                    self.stack.push(Value::Bool(!truthy));
                }
                Op::Negate => {
                    let val = self.pop();
                    match val {
                        Value::Number(n) => self.stack.push(Value::Number(-n)),
                        _ => return Err(error(line, "Operand must be a number.")),
                    }
                }
                Op::Print => {
                    let val = self.pop();
                    println!("{}", val);
                }
                Op::Jump(t) => {
                    self.frame_mut().ip = t;
                }
                Op::JumpIfFalse(t) => {
                    if !self.peek().is_truthy() {
                        self.frame_mut().ip = t;
                    }
                }
                Op::Call(n) => {
                    self.call(n, line)?;
                }
                Op::Return => {
                    let result = self.pop();
                    if self.frames.len() == 1 {
                        return Ok(());
                    }
                    let base = self.frame().base;
                    self.frames.pop();
                    self.stack.truncate(base - 1);
                    self.stack.push(result);
                }
            }
        }
    }

    /// The callee sits under its arguments, and the arguments are already where the new
    /// frame's slots need them. Nothing is moved.
    fn call(&mut self, argc: usize, line: usize) -> Result<(), RuntimeError> {
        // TODO(you): spec 4.3 again, on a stack instead of a tree: the arguments are already
        //            lying where the callee's slots need them.
        let callee = self.stack[self.stack.len() - 1 - argc].clone();
        let index = match callee {
            Value::Function { index, .. } => index,
            _ => return Err(error(line, "This value is not a function.")),
        };
        let arity = self.bytecode.functions[index].arity;
        if argc != arity {
            return Err(error(
                line,
                &format!("Expected {} arguments but got {}.", arity, argc),
            ));
        }
        let active_calls = self.frames.len() - 1;
        if active_calls >= MAX_FRAMES {
            return Err(error(line, "Stack overflow."));
        }
        let base = self.stack.len() - argc;
        self.frames.push(Frame {
            function: Some(index),
            ip: 0,
            base,
        });
        Ok(())
    }

    // --- primitives -------------------------------------------------------------------

    fn chunk(&self, function: Option<usize>) -> &Chunk {
        match function {
            None => &self.bytecode.script,
            Some(i) => &self.bytecode.functions[i],
        }
    }

    fn name(&self, function: Option<usize>, index: usize) -> String {
        self.chunk(function).constants[index].to_string()
    }

    fn frame(&self) -> &Frame {
        self.frames.last().expect("a frame is always running")
    }

    fn frame_mut(&mut self) -> &mut Frame {
        self.frames.last_mut().expect("a frame is always running")
    }

    fn pop(&mut self) -> Value {
        self.stack.pop().expect("the compiler balances the stack")
    }

    fn peek(&self) -> &Value {
        self.stack.last().expect("the compiler balances the stack")
    }

    fn pop_two(&mut self) -> (Value, Value) {
        let b = self.pop();
        let a = self.pop();
        (a, b)
    }

    fn binary_value(&mut self, f: fn(Value, Value) -> Value) {
        let (a, b) = self.pop_two();
        self.stack.push(f(a, b));
    }

    fn numbers(&mut self, line: usize) -> Result<(f64, f64), RuntimeError> {
        match self.pop_two() {
            (Value::Number(a), Value::Number(b)) => Ok((a, b)),
            _ => Err(error(line, "Operands must be numbers.")),
        }
    }

    fn arith(&mut self, line: usize, f: fn(f64, f64) -> f64) -> Result<(), RuntimeError> {
        let (a, b) = self.numbers(line)?;
        self.stack.push(Value::Number(f(a, b)));
        Ok(())
    }

    fn compare(&mut self, line: usize, f: fn(f64, f64) -> bool) -> Result<(), RuntimeError> {
        let (a, b) = self.numbers(line)?;
        self.stack.push(Value::Bool(f(a, b)));
        Ok(())
    }
}

fn undefined(line: usize, name: &str) -> RuntimeError {
    error(line, &format!("Variable '{}' is not defined.", name))
}

fn error(line: usize, message: &str) -> RuntimeError {
    RuntimeError {
        line,
        message: message.to_string(),
    }
}