# Q1
Fractional numbers

The scanner decides that a . begins a fractional part only when it is immediately followed by a digit. This decision is made in src/scanner.rs:84, where the condition checks both self.peek() == '.' and self.peek_next().is_ascii_digit(). This is important because a dot by itself is not a token in Kobo. For example, when the input is 5., number() first consumes the 5. The condition at src/scanner.rs:84 then fails because the dot is not followed by a digit. Therefore, the dot is not consumed as part of the number. The scanner adds 5 as a Number token and later processes the . separately. The _ arm at src/scanner.rs:66 reports the dot as an invalid character. This matches section 1.4 because a fractional number must contain digits after the decimal point. In contrast, 5.25 passes the condition, consumes the dot and 25, and becomes one Number token.

# Q2
 Line counting and EOF

The scanner changes the line counter in two places. A normal newline is handled at src/scanner.rs:45, where self.line += 1 is executed. Newlines inside strings are handled at src/scanner.rs:72, where the same increment occurs while the string is being scanned. These are the only places where the scanner changes its line counter. EOF does not simply use self.line. Instead, run() gets the line from the last real token at src/scanner.rs:20-22: self.tokens.last().map(|token| token.line).unwrap_or(1). Therefore, if a file ends with two blank lines, EOF carries the line number of the last real token rather than the final physical line of the file. Section 6.1 requires this because EOF represents the end of the scanned token stream, not an additional token located on every trailing blank line. Using self.line directly would incorrectly move EOF forward when the source ends with blank lines.

# Q3
A failed test and what I learned

One test I encountered during the implementation was a test involving number literals and decimal points. I initially had to pay particular attention to the 5. case because it is different from a normal decimal such as 5.25. I learned that the scanner must not consume the decimal point until it has confirmed that a digit follows it. The important line is src/scanner.rs:84, where both peek() and peek_next() are checked before the dot is consumed. This helped me understand that scanner implementations depend heavily on lookahead and on consuming characters only after the complete token rule has been confirmed.

I did not have Git commits from the earlier stages of my work, so I cannot truthfully provide two commit hashes or quote the exact old and new lines from my own Git history. I have not invented hashes because they would not exist in my repository. My main lesson from this task is that I should commit working stages as I develop the scanner so that my changes and mistakes can be traced accurately.