# Reflection - CA2: The Parser

## 1. Dangling Else Resolution
The dangling `else` in an `if` statement is resolved in `src/parser.rs:164` inside the `if_statement` method:
```rust
let else_branch = if self.matches(&[TokenType::Else]) {
    Some(Box::new(self.statement()?))
} else {
    None
};
```

This behavior is determined entirely by recursive descent rather than explicit grammar rules. As soon as if_statement finishes parsing the then_branch statement, it immediately checks if the next token is else. Because of this greedy matching order, an else naturally binds to the closest preceding unmatched if.   For the input if (false) if (true) print "a"; else print "b";, our parser produces:(if false (if true (print "a") (print "b")))
If the else bound to the outer if instead, it would print:(if false (if true (print "a")) (print "b"))Commit where it was incorrect/missing: c23ed27 (if stmt impl) where todo!("if_statement") was still present:todo!("if_statement")Commit where it was fixed: c23ed27 (or updated in ae396b3):let else_branch = if self.matches(&[TokenType::Else]) { Some(Box::new(self.statement()?)) } else { None };

## 2. Assignment Expression Verification
Assignment is checked in src/parser.rs:215 within the assignment method:

```rust
if let Expr::Variable { name, at } = expr {
    return Ok(Expr::Assign { name, value: Box::new(value), at, });
}
self.error_at(&equals, "Cannot assign to this expression.");
return Err(());
```

This check cannot happen before parsing the left side because assignment syntax in Kobo is not LL(1)—an expression like a or a.b or f() starts identically during token lookahead. The parser must first parse the left side as an ordinary expression, and only when the = token is encountered does it look back to verify if the left-hand side is a valid assignable target (a bare variable).   For the input 1 = 2;, this produces the exact section 5.1 error message:
[line 1] Error at '=': Cannot assign to this expression.   Commit where it was incorrect/missing: 3692e47 (assignment impl) initial stub:self.or()Commit where it was fixed: 3692e47 (updated in ae396b3):if let Expr::Variable { name, at } = expr { return Ok(Expr::Assign { name, value: Box::new(value), at, }); }

## 3. Error Synchronisation
The synchronisation logic is implemented in src/parser.rs:420 inside synchronise:

```rust
fn synchronise(&mut self) {
    self.depth = 0;
    if self.at_end() { return; }
    self.current += 1;
    while !self.at_end() {
        if self.previous().kind == TokenType::Semicolon { return; }
        match self.peek().kind {
            TokenType::Fun | TokenType::Var | TokenType::If | TokenType::While | TokenType::Print | TokenType::Return => return,
            _ => {}
        }
        self.current += 1;
    }
}
```
synchronise stops on either a semicolon (immediately consuming past it) or on statement-boundary keywords (fun, var, if, while, print, return). It always steps over at least one token first to guarantee progress and prevent infinite loops on invalid input.   If you delete the call to synchronise() in parse(), a syntax error (such as a missing semicolon or malformed statement) will fail to recover, causing cascading parse errors where a single typo generates dozens of error messages instead of one. For instance, in an invalid test file with multiple statements following a syntax error, the output balloons into numerous redundant "Expect expression." or "Expect ';'" errors.   Commit where it was incorrect/missing: b64f41d (synchronise impl) initial simple loop:while !self.at_end() { self.current += 1; if self.previous().kind == TokenType::Semicolon { return; } }Commit where it was fixed: 740eaee (real synchronise impl replaced 'ffn' with fn):fn synchronise(&mut self) {