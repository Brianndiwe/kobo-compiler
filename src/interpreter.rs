use crate::ast::{Expr, Program, Resolved, Stmt};
use crate::environment::Environment;
use crate::token::{Token, TokenType};
use crate::value::Value;

pub struct RuntimeError {
    pub line: usize,
    pub message: String,
}

/// Why a statement stopped early: a runtime error unwinding to `main`, or a `return`
/// unwinding to its call.
enum Flow {
    Error(RuntimeError),
    Return(Value),
}

type Exec<T> = Result<T, Flow>;

/// Kobo's call stack is Rust's, and overflowing it aborts with no message, so calls are
/// capped (spec section 5.1). The measured limit here is near 1,500; 500 leaves room for
/// a deeply nested expression inside every frame.
pub const MAX_FRAMES: usize = 500;

pub fn interpret(program: &Program) -> Result<(), RuntimeError> {
    let mut env = Environment::new();
    for stmt in &program.stmts {
        match execute(stmt, &mut env, program) {
            Ok(()) => {}
            // `return` outside any function ends the program (spec section 3.1).
            Err(Flow::Return(_)) => return Ok(()),
            Err(Flow::Error(e)) => return Err(e),
        }
    }
    Ok(())
}

fn execute(stmt: &Stmt, env: &mut Environment, program: &Program) -> Exec<()> {
    match stmt {
        Stmt::Expression(e) => {
            evaluate(e, env, program)?;
            Ok(())
        }
        Stmt::Print(e) => {
            let v = evaluate(e, env, program)?;
            println!("{}", v);
            Ok(())
        }
        Stmt::Var {
            name,
            initialiser,
            ..
        } => {
            let value = match initialiser {
                Some(expr) => evaluate(expr, env, program)?,
                None => Value::Nil,
            };
            env.declare(&name.lexeme, value);
            Ok(())
        }
        Stmt::Block { body, .. } => {
            env.push_scope();
            for s in body {
                if let Err(flow) = execute(s, env, program) {
                    env.pop_scope();
                    return Err(flow);
                }
            }
            env.pop_scope();
            Ok(())
        }
        Stmt::If {
            condition,
            then_branch,
            else_branch,
        } => {
            let cond = evaluate(condition, env, program)?;
            if cond.is_truthy() {
                execute(then_branch, env, program)?;
            } else if let Some(else_stmt) = else_branch {
                execute(else_stmt, env, program)?;
            }
            Ok(())
        }
        Stmt::While { condition, body } => {
            while evaluate(condition, env, program)?.is_truthy() {
                execute(body, env, program)?;
            }
            Ok(())
        }
        Stmt::Function { index, name, .. } => {
            env.declare(
                name,
                Value::Function {
                    index: *index,
                    name: name.clone(),
                },
            );
            Ok(())
        }
        Stmt::Return(v) => {
            let value = match v {
                Some(expr) => evaluate(expr, env, program)?,
                None => Value::Nil,
            };
            Err(Flow::Return(value))
        }
    }
}

fn evaluate(expr: &Expr, env: &mut Environment, program: &Program) -> Exec<Value> {
    match expr {
        Expr::Literal { value, .. } => Ok(value.clone()),
        Expr::Grouping(inner) => evaluate(inner, env, program),
        Expr::Variable { name, at } => {
            let val = match at {
                Resolved::Local { depth, .. } => env.get_at(*depth, &name.lexeme),
                Resolved::Global => env.get_global(&name.lexeme),
            };
            val.cloned().ok_or_else(|| undefined(name))
        }
        Expr::Assign { name, value, at } => {
            let v = evaluate(value, env, program)?;
            let assigned = match at {
                Resolved::Local { depth, .. } => {
                    env.assign_at(*depth, &name.lexeme, v.clone())
                }
                Resolved::Global => env.assign_global(&name.lexeme, v.clone()),
            };
            if !assigned {
                return Err(undefined(name));
            }
            Ok(v)
        }
        Expr::Logical { left, op, right } => {
            let l = evaluate(left, env, program)?;
            match op.kind {
                TokenType::Or => {
                    if l.is_truthy() {
                        Ok(l)
                    } else {
                        evaluate(right, env, program)
                    }
                }
                TokenType::And => {
                    if !l.is_truthy() {
                        Ok(l)
                    } else {
                        evaluate(right, env, program)
                    }
                }
                _ => unreachable!(),
            }
        }
        Expr::Unary { op, right } => {
            let r = evaluate(right, env, program)?;
            match op.kind {
                TokenType::Minus => match r {
                    Value::Number(n) => Ok(Value::Number(-n)),
                    _ => Err(err(op.line, "Operand must be a number.")),
                },
                TokenType::Bang => Ok(Value::Bool(!r.is_truthy())),
                _ => unreachable!(),
            }
        }
        Expr::Binary { left, op, right } => {
            let l = evaluate(left, env, program)?;
            let r = evaluate(right, env, program)?;
            match op.kind {
                TokenType::Plus => match (l, r) {
                    (Value::Number(a), Value::Number(b)) => Ok(Value::Number(a + b)),
                    (Value::Str(a), Value::Str(b)) => Ok(Value::Str(format!("{}{}", a, b))),
                    _ => Err(err(
                        op.line,
                        "Operands must be two numbers or two strings.",
                    )),
                },
                TokenType::Minus => arith(l, r, op.line, |a, b| a - b),
                TokenType::Star => arith(l, r, op.line, |a, b| a * b),
                TokenType::Slash => {
                    let (a, b) = numbers(l, r, op.line)?;
                    if b == 0.0 {
                        Err(err(op.line, "Division by zero."))
                    } else {
                        Ok(Value::Number(a / b))
                    }
                }
                TokenType::Caret => arith(l, r, op.line, f64::powf),
                TokenType::Greater => compare(l, r, op.line, |a, b| a > b),
                TokenType::GreaterEqual => compare(l, r, op.line, |a, b| a >= b),
                TokenType::Less => compare(l, r, op.line, |a, b| a < b),
                TokenType::LessEqual => compare(l, r, op.line, |a, b| a <= b),
                TokenType::EqualEqual => Ok(Value::Bool(l == r)),
                TokenType::BangEqual => Ok(Value::Bool(l != r)),
                _ => unreachable!(),
            }
        }
        Expr::Call {
            callee,
            paren,
            args,
        } => {
            let f = evaluate(callee, env, program)?;
            let mut eval_args = Vec::with_capacity(args.len());
            for arg in args {
                eval_args.push(evaluate(arg, env, program)?);
            }
            match f {
                Value::Function { index, .. } => call(index, eval_args, paren.line, env, program),
                _ => Err(err(paren.line, "This value is not a function.")),
            }
        }
    }
}

/// Run a function body in a fresh frame: parameters in the frame's own scope, the
/// globals beyond it, and the frame popped on every way out.
fn call(
    index: usize,
    args: Vec<Value>,
    line: usize,
    env: &mut Environment,
    program: &Program,
) -> Exec<Value> {
    let f = &program.functions[index];
    if args.len() != f.params.len() {
        return Err(err(
            line,
            &format!(
                "Expected {} arguments but got {}.",
                f.params.len(),
                args.len()
            ),
        ));
    }
    if env.frame_depth() >= MAX_FRAMES {
        return Err(err(line, "Stack overflow."));
    }

    env.push_frame();
    for (p, a) in f.params.iter().zip(args) {
        env.declare(p, a);
    }

    let mut result = Ok(Value::Nil);
    for stmt in &f.body {
        match execute(stmt, env, program) {
            Ok(()) => {}
            Err(Flow::Return(v)) => {
                result = Ok(v);
                break;
            }
            Err(e) => {
                result = Err(e);
                break;
            }
        }
    }

    env.pop_frame();
    result
}

fn numbers(l: Value, r: Value, line: usize) -> Exec<(f64, f64)> {
    match (l, r) {
        (Value::Number(a), Value::Number(b)) => Ok((a, b)),
        _ => Err(err(line, "Operands must be numbers.")),
    }
}

fn arith(l: Value, r: Value, line: usize, f: fn(f64, f64) -> f64) -> Exec<Value> {
    let (a, b) = numbers(l, r, line)?;
    Ok(Value::Number(f(a, b)))
}

fn compare(l: Value, r: Value, line: usize, f: fn(f64, f64) -> bool) -> Exec<Value> {
    let (a, b) = numbers(l, r, line)?;
    Ok(Value::Bool(f(a, b)))
}

fn undefined(name: &Token) -> Flow {
    err(name.line, &format!("Variable '{}' is not defined.", name.lexeme))
}

fn err(line: usize, message: &str) -> Flow {
    Flow::Error(RuntimeError {
        line,
        message: message.to_string(),
    })
}