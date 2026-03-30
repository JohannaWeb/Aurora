use crate::dom::Node;
use std::fmt::{self, Display, Formatter};

#[derive(Debug, Clone, PartialEq)]
pub struct Program {
    body: Vec<Statement>,
}

#[derive(Debug, Clone, PartialEq)]
enum Statement {
    VarDecl {
        kind: VarKind,
        name: String,
        init: Option<Expr>,
    },
    FunctionDecl {
        name: String,
        params: Vec<String>,
        body: Vec<Statement>,
    },
    Return(Option<Expr>),
    If {
        condition: Expr,
        consequent: Vec<Statement>,
        alternate: Option<Vec<Statement>>,
    },
    ExprStmt(Expr),
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum VarKind {
    Let,
    Const,
    Var,
}

#[derive(Debug, Clone, PartialEq)]
enum Expr {
    Number(f64),
    StringLit(String),
    Bool(bool),
    Null,
    Ident(String),
    BinOp {
        op: String,
        lhs: Box<Expr>,
        rhs: Box<Expr>,
    },
    Unary {
        op: char,
        operand: Box<Expr>,
    },
    Call {
        callee: Box<Expr>,
        args: Vec<Expr>,
    },
    Member {
        object: Box<Expr>,
        property: String,
    },
    Assign {
        target: Box<Expr>,
        value: Box<Expr>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Token {
    Keyword(Keyword),
    Ident(String),
    Number(String),
    StringLit(String),
    Punct(char),
    EOF,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Keyword {
    Let,
    Const,
    Var,
    Function,
    Return,
    If,
    Else,
    True,
    False,
    Null,
}

impl Program {
    pub fn from_dom(document: &Node) -> Self {
        let mut source = String::new();
        collect_script_text(document, &mut source);
        Self::parse(&source)
    }

    fn parse(source: &str) -> Self {
        let tokens = tokenize(source);
        let mut parser = Parser::new(tokens);
        let body = parser.parse_program();
        Self { body }
    }

    fn fmt_with_indent(&self, f: &mut Formatter<'_>, depth: usize) -> fmt::Result {
        let indent = "  ".repeat(depth);
        writeln!(f, "{indent}#program")?;
        for statement in &self.body {
            statement.fmt_with_indent(f, depth + 1)?;
        }
        Ok(())
    }
}

impl Statement {
    fn fmt_with_indent(&self, f: &mut Formatter<'_>, depth: usize) -> fmt::Result {
        let indent = "  ".repeat(depth);
        match self {
            Statement::VarDecl { kind, name, init } => {
                write!(f, "{indent}var-decl({:?}): {}", kind, name)?;
                if let Some(expr) = init {
                    write!(f, " = {}", expr)?;
                }
                writeln!(f)?;
            }
            Statement::FunctionDecl { name, params, body } => {
                let params_str = params.join(", ");
                writeln!(f, "{indent}function: {}({})", name, params_str)?;
                for stmt in body {
                    stmt.fmt_with_indent(f, depth + 1)?;
                }
            }
            Statement::Return(expr) => {
                write!(f, "{indent}return")?;
                if let Some(e) = expr {
                    write!(f, " {}", e)?;
                }
                writeln!(f)?;
            }
            Statement::If {
                condition,
                consequent,
                alternate,
            } => {
                writeln!(f, "{indent}if")?;
                writeln!(f, "{}  condition: {}", indent, condition)?;
                writeln!(f, "{}  consequent:", indent)?;
                for stmt in consequent {
                    stmt.fmt_with_indent(f, depth + 2)?;
                }
                if let Some(alt) = alternate {
                    writeln!(f, "{}  alternate:", indent)?;
                    for stmt in alt {
                        stmt.fmt_with_indent(f, depth + 2)?;
                    }
                }
            }
            Statement::ExprStmt(expr) => {
                writeln!(f, "{indent}expr: {}", expr)?;
            }
        }
        Ok(())
    }
}

impl Display for Expr {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Expr::Number(n) => write!(f, "number: {}", n),
            Expr::StringLit(s) => write!(f, "string: \"{}\"", s),
            Expr::Bool(b) => write!(f, "bool: {}", b),
            Expr::Null => write!(f, "null"),
            Expr::Ident(name) => write!(f, "ident: {}", name),
            Expr::BinOp { op, lhs, rhs } => {
                write!(f, "{}(", op)?;
                write!(f, "{}, ", lhs)?;
                write!(f, "{})", rhs)
            }
            Expr::Unary { op, operand } => {
                write!(f, "{}({})", op, operand)
            }
            Expr::Call { callee, args } => {
                write!(f, "call: {}", callee)?;
                for arg in args {
                    write!(f, " {}", arg)?;
                }
                Ok(())
            }
            Expr::Member { object, property } => {
                write!(f, "member: {}.{}", object, property)
            }
            Expr::Assign { target, value } => {
                write!(f, "assign: {} = {}", target, value)
            }
        }
    }
}

impl Display for VarKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            VarKind::Let => write!(f, "let"),
            VarKind::Const => write!(f, "const"),
            VarKind::Var => write!(f, "var"),
        }
    }
}

impl Display for Program {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        self.fmt_with_indent(f, 0)
    }
}

struct Parser {
    tokens: Vec<Token>,
    position: usize,
}

impl Parser {
    fn new(tokens: Vec<Token>) -> Self {
        Self {
            tokens,
            position: 0,
        }
    }

    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.position)
    }

    fn advance(&mut self) -> Option<Token> {
        if self.position < self.tokens.len() {
            let token = self.tokens[self.position].clone();
            self.position += 1;
            Some(token)
        } else {
            None
        }
    }

    fn is_eof(&self) -> bool {
        self.position >= self.tokens.len()
            || matches!(self.peek(), Some(Token::EOF) | None)
    }

    fn parse_program(&mut self) -> Vec<Statement> {
        let mut statements = Vec::new();

        while !self.is_eof() {
            if let Some(stmt) = self.parse_statement() {
                statements.push(stmt);
            } else {
                self.advance();
            }
        }

        statements
    }

    fn parse_statement(&mut self) -> Option<Statement> {
        match self.peek()? {
            Token::Keyword(Keyword::Let) | Token::Keyword(Keyword::Const) | Token::Keyword(Keyword::Var) => {
                let kind = match self.advance()? {
                    Token::Keyword(Keyword::Let) => VarKind::Let,
                    Token::Keyword(Keyword::Const) => VarKind::Const,
                    Token::Keyword(Keyword::Var) => VarKind::Var,
                    _ => unreachable!(),
                };

                let name = match self.advance()? {
                    Token::Ident(n) => n,
                    _ => return None,
                };

                let init = if matches!(self.peek(), Some(Token::Punct('='))) {
                    self.advance();
                    Some(self.parse_expr()?)
                } else {
                    None
                };

                if matches!(self.peek(), Some(Token::Punct(';'))) {
                    self.advance();
                }

                Some(Statement::VarDecl { kind, name, init })
            }
            Token::Keyword(Keyword::Function) => {
                self.advance();
                let name = match self.advance()? {
                    Token::Ident(n) => n,
                    _ => return None,
                };

                if !matches!(self.peek(), Some(Token::Punct('('))) {
                    return None;
                }
                self.advance();

                let mut params = Vec::new();
                while !matches!(self.peek(), Some(Token::Punct(')'))) {
                    if let Token::Ident(param) = self.advance()? {
                        params.push(param);
                        if matches!(self.peek(), Some(Token::Punct(','))) {
                            self.advance();
                        }
                    } else {
                        return None;
                    }
                }

                if !matches!(self.peek(), Some(Token::Punct(')'))) {
                    return None;
                }
                self.advance();

                if !matches!(self.peek(), Some(Token::Punct('{'))) {
                    return None;
                }
                self.advance();

                let mut body = Vec::new();
                while !matches!(self.peek(), Some(Token::Punct('}'))) && !self.is_eof() {
                    if let Some(stmt) = self.parse_statement() {
                        body.push(stmt);
                    } else {
                        self.advance();
                    }
                }

                if !matches!(self.peek(), Some(Token::Punct('}'))) {
                    return None;
                }
                self.advance();

                Some(Statement::FunctionDecl { name, params, body })
            }
            Token::Keyword(Keyword::Return) => {
                self.advance();
                let expr = if !matches!(self.peek(), Some(Token::Punct(';'))) && !self.is_eof() {
                    Some(self.parse_expr()?)
                } else {
                    None
                };

                if matches!(self.peek(), Some(Token::Punct(';'))) {
                    self.advance();
                }

                Some(Statement::Return(expr))
            }
            Token::Keyword(Keyword::If) => {
                self.advance();

                if !matches!(self.peek(), Some(Token::Punct('('))) {
                    return None;
                }
                self.advance();

                let condition = self.parse_expr()?;

                if !matches!(self.peek(), Some(Token::Punct(')'))) {
                    return None;
                }
                self.advance();

                if !matches!(self.peek(), Some(Token::Punct('{'))) {
                    return None;
                }
                self.advance();

                let mut consequent = Vec::new();
                while !matches!(self.peek(), Some(Token::Punct('}'))) && !self.is_eof() {
                    if let Some(stmt) = self.parse_statement() {
                        consequent.push(stmt);
                    } else {
                        self.advance();
                    }
                }

                if !matches!(self.peek(), Some(Token::Punct('}'))) {
                    return None;
                }
                self.advance();

                let alternate = if matches!(self.peek(), Some(Token::Keyword(Keyword::Else))) {
                    self.advance();

                    if !matches!(self.peek(), Some(Token::Punct('{'))) {
                        return None;
                    }
                    self.advance();

                    let mut alt = Vec::new();
                    while !matches!(self.peek(), Some(Token::Punct('}'))) && !self.is_eof() {
                        if let Some(stmt) = self.parse_statement() {
                            alt.push(stmt);
                        } else {
                            self.advance();
                        }
                    }

                    if !matches!(self.peek(), Some(Token::Punct('}'))) {
                        return None;
                    }
                    self.advance();

                    Some(alt)
                } else {
                    None
                };

                Some(Statement::If {
                    condition,
                    consequent,
                    alternate,
                })
            }
            _ => {
                let expr = self.parse_expr()?;
                if matches!(self.peek(), Some(Token::Punct(';'))) {
                    self.advance();
                }
                Some(Statement::ExprStmt(expr))
            }
        }
    }

    fn parse_expr(&mut self) -> Option<Expr> {
        self.parse_assign()
    }

    fn parse_assign(&mut self) -> Option<Expr> {
        let expr = self.parse_equality()?;

        if matches!(self.peek(), Some(Token::Punct('='))) {
            self.advance();
            let value = self.parse_assign()?;
            Some(Expr::Assign {
                target: Box::new(expr),
                value: Box::new(value),
            })
        } else {
            Some(expr)
        }
    }

    fn parse_equality(&mut self) -> Option<Expr> {
        let mut expr = self.parse_additive()?;

        loop {
            let op = match self.peek() {
                Some(Token::Punct('=')) => {
                    if self.position + 1 < self.tokens.len() {
                        if let Some(Token::Punct('=')) = self.tokens.get(self.position + 1) {
                            "==".to_string()
                        } else {
                            break;
                        }
                    } else {
                        break;
                    }
                }
                Some(Token::Punct('!')) => {
                    if self.position + 1 < self.tokens.len() {
                        if let Some(Token::Punct('=')) = self.tokens.get(self.position + 1) {
                            "!=".to_string()
                        } else {
                            break;
                        }
                    } else {
                        break;
                    }
                }
                _ => break,
            };

            self.advance();
            self.advance();

            let rhs = self.parse_additive()?;
            expr = Expr::BinOp {
                op,
                lhs: Box::new(expr),
                rhs: Box::new(rhs),
            };
        }

        Some(expr)
    }

    fn parse_additive(&mut self) -> Option<Expr> {
        let mut expr = self.parse_unary()?;

        loop {
            let op = match self.peek() {
                Some(Token::Punct('+')) => "+".to_string(),
                Some(Token::Punct('-')) => "-".to_string(),
                _ => break,
            };

            self.advance();
            let rhs = self.parse_unary()?;
            expr = Expr::BinOp {
                op,
                lhs: Box::new(expr),
                rhs: Box::new(rhs),
            };
        }

        Some(expr)
    }

    fn parse_unary(&mut self) -> Option<Expr> {
        match self.peek() {
            Some(Token::Punct(op)) if *op == '!' || *op == '-' => {
                let op = *op;
                self.advance();
                let operand = self.parse_unary()?;
                Some(Expr::Unary {
                    op,
                    operand: Box::new(operand),
                })
            }
            _ => self.parse_call_or_member(),
        }
    }

    fn parse_call_or_member(&mut self) -> Option<Expr> {
        let mut expr = self.parse_primary()?;

        loop {
            match self.peek() {
                Some(Token::Punct('.')) => {
                    self.advance();
                    let property = match self.advance()? {
                        Token::Ident(p) => p,
                        _ => return None,
                    };
                    expr = Expr::Member {
                        object: Box::new(expr),
                        property,
                    };
                }
                Some(Token::Punct('(')) => {
                    self.advance();
                    let mut args = Vec::new();
                    while !matches!(self.peek(), Some(Token::Punct(')'))) && !self.is_eof() {
                        args.push(self.parse_expr()?);
                        if matches!(self.peek(), Some(Token::Punct(','))) {
                            self.advance();
                        }
                    }
                    if !matches!(self.peek(), Some(Token::Punct(')'))) {
                        return None;
                    }
                    self.advance();
                    expr = Expr::Call {
                        callee: Box::new(expr),
                        args,
                    };
                }
                _ => break,
            }
        }

        Some(expr)
    }

    fn parse_primary(&mut self) -> Option<Expr> {
        match self.advance()? {
            Token::Number(n) => {
                let val = n.parse::<f64>().ok()?;
                Some(Expr::Number(val))
            }
            Token::StringLit(s) => Some(Expr::StringLit(s)),
            Token::Ident(name) => Some(Expr::Ident(name)),
            Token::Keyword(Keyword::True) => Some(Expr::Bool(true)),
            Token::Keyword(Keyword::False) => Some(Expr::Bool(false)),
            Token::Keyword(Keyword::Null) => Some(Expr::Null),
            Token::Punct('(') => {
                let expr = self.parse_expr()?;
                if !matches!(self.peek(), Some(Token::Punct(')'))) {
                    return None;
                }
                self.advance();
                Some(expr)
            }
            _ => None,
        }
    }
}

fn tokenize(source: &str) -> Vec<Token> {
    let mut tokens = Vec::new();
    let mut chars = source.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch.is_whitespace() {
            continue;
        }

        if ch == '/' {
            if chars.peek() == Some(&'/') {
                chars.next();
                while let Some(c) = chars.peek() {
                    if *c == '\n' {
                        break;
                    }
                    chars.next();
                }
                continue;
            } else if chars.peek() == Some(&'*') {
                chars.next();
                let mut prev = ' ';
                while let Some(c) = chars.next() {
                    if prev == '*' && c == '/' {
                        break;
                    }
                    prev = c;
                }
                continue;
            }
        }

        if ch == '"' || ch == '\'' {
            let quote = ch;
            let mut string = String::new();
            while let Some(c) = chars.next() {
                if c == quote {
                    break;
                }
                if c == '\\' {
                    if let Some(escaped) = chars.next() {
                        match escaped {
                            'n' => string.push('\n'),
                            't' => string.push('\t'),
                            'r' => string.push('\r'),
                            '\\' => string.push('\\'),
                            '"' => string.push('"'),
                            '\'' => string.push('\''),
                            _ => {
                                string.push('\\');
                                string.push(escaped);
                            }
                        }
                    }
                } else {
                    string.push(c);
                }
            }
            tokens.push(Token::StringLit(string));
            continue;
        }

        if ch.is_ascii_digit() {
            let mut number = String::new();
            number.push(ch);
            while let Some(&c) = chars.peek() {
                if c.is_ascii_digit() || c == '.' {
                    number.push(c);
                    chars.next();
                } else {
                    break;
                }
            }
            tokens.push(Token::Number(number));
            continue;
        }

        if ch.is_ascii_alphabetic() || ch == '_' || ch == '$' {
            let mut ident = String::new();
            ident.push(ch);
            while let Some(&c) = chars.peek() {
                if c.is_ascii_alphanumeric() || c == '_' || c == '$' {
                    ident.push(c);
                    chars.next();
                } else {
                    break;
                }
            }

            let token = match ident.as_str() {
                "let" => Token::Keyword(Keyword::Let),
                "const" => Token::Keyword(Keyword::Const),
                "var" => Token::Keyword(Keyword::Var),
                "function" => Token::Keyword(Keyword::Function),
                "return" => Token::Keyword(Keyword::Return),
                "if" => Token::Keyword(Keyword::If),
                "else" => Token::Keyword(Keyword::Else),
                "true" => Token::Keyword(Keyword::True),
                "false" => Token::Keyword(Keyword::False),
                "null" => Token::Keyword(Keyword::Null),
                _ => Token::Ident(ident),
            };
            tokens.push(token);
            continue;
        }

        match ch {
            ';' | ',' | '(' | ')' | '{' | '}' | '[' | ']' | '.' | '=' | '+' | '-' | '*' | '/' | '%' | '!' | '<' | '>' | '&' | '|' => {
                tokens.push(Token::Punct(ch));
            }
            _ => {}
        }
    }

    tokens.push(Token::EOF);
    tokens
}

fn collect_script_text(node: &Node, output: &mut String) {
    match node {
        Node::Document { children } => {
            for child in children {
                collect_script_text(child, output);
            }
        }
        Node::Element(element) => {
            if element.tag_name == "script" {
                for child in &element.children {
                    if let Node::Text(text) = child {
                        output.push_str(text);
                        output.push('\n');
                    }
                }
            }

            for child in &element.children {
                collect_script_text(child, output);
            }
        }
        Node::Text(_) => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_variable_declarations() {
        let program = Program::parse("let x = 42;");
        assert_eq!(program.body.len(), 1);
        match &program.body[0] {
            Statement::VarDecl { kind, name, init } => {
                assert_eq!(kind, &VarKind::Let);
                assert_eq!(name, "x");
                assert!(init.is_some());
            }
            _ => panic!("Expected VarDecl"),
        }
    }

    #[test]
    fn parses_const_and_var() {
        let const_prog = Program::parse("const y = 10;");
        let var_prog = Program::parse("var z;");

        match &const_prog.body[0] {
            Statement::VarDecl { kind, .. } => assert_eq!(kind, &VarKind::Const),
            _ => panic!(),
        }

        match &var_prog.body[0] {
            Statement::VarDecl { kind, init, .. } => {
                assert_eq!(kind, &VarKind::Var);
                assert!(init.is_none());
            }
            _ => panic!(),
        }
    }

    #[test]
    fn parses_function_declaration() {
        let program = Program::parse("function greet(name) { return name; }");
        assert_eq!(program.body.len(), 1);

        match &program.body[0] {
            Statement::FunctionDecl { name, params, body } => {
                assert_eq!(name, "greet");
                assert_eq!(params, &vec!["name".to_string()]);
                assert_eq!(body.len(), 1);
            }
            _ => panic!("Expected FunctionDecl"),
        }
    }

    #[test]
    fn parses_if_statement() {
        let program = Program::parse("if (x == 5) { let y = 1; }");
        assert_eq!(program.body.len(), 1);

        match &program.body[0] {
            Statement::If {
                condition,
                consequent,
                alternate,
            } => {
                assert!(matches!(condition, Expr::BinOp { .. }));
                assert_eq!(consequent.len(), 1);
                assert!(alternate.is_none());
            }
            _ => panic!("Expected If"),
        }
    }

    #[test]
    fn parses_if_else() {
        let program = Program::parse("if (true) { let a = 1; } else { let b = 2; }");
        match &program.body[0] {
            Statement::If { alternate, .. } => {
                assert!(alternate.is_some());
                assert_eq!(alternate.as_ref().unwrap().len(), 1);
            }
            _ => panic!(),
        }
    }

    #[test]
    fn parses_expression_statements() {
        let program = Program::parse("foo(); bar.baz;");
        assert_eq!(program.body.len(), 2);

        match &program.body[0] {
            Statement::ExprStmt(Expr::Call { .. }) => {}
            _ => panic!("Expected call expression"),
        }

        match &program.body[1] {
            Statement::ExprStmt(Expr::Member { .. }) => {}
            _ => panic!("Expected member expression"),
        }
    }

    #[test]
    fn parses_binary_and_unary_operators() {
        let program = Program::parse("let x = -5; let y = !true;");
        assert_eq!(program.body.len(), 2);

        match &program.body[0] {
            Statement::VarDecl {
                init: Some(Expr::Unary { op, .. }),
                ..
            } => {
                assert_eq!(op, &'-');
            }
            _ => panic!(),
        }

        match &program.body[1] {
            Statement::VarDecl {
                init: Some(Expr::Unary { op, .. }),
                ..
            } => {
                assert_eq!(op, &'!');
            }
            _ => panic!(),
        }
    }

    #[test]
    fn parses_string_and_number_literals() {
        let program = Program::parse(r#"let s = "hello"; let n = 3.14;"#);
        assert_eq!(program.body.len(), 2);

        match &program.body[0] {
            Statement::VarDecl {
                init: Some(Expr::StringLit(s)),
                ..
            } => assert_eq!(s, "hello"),
            _ => panic!(),
        }

        match &program.body[1] {
            Statement::VarDecl {
                init: Some(Expr::Number(n)),
                ..
            } => assert!((*n - 3.14).abs() < 0.001),
            _ => panic!(),
        }
    }

    #[test]
    fn parses_boolean_and_null_literals() {
        let program = Program::parse("let t = true; let f = false; let n = null;");
        assert_eq!(program.body.len(), 3);

        match &program.body[0] {
            Statement::VarDecl {
                init: Some(Expr::Bool(b)),
                ..
            } => assert!(b),
            _ => panic!(),
        }

        match &program.body[2] {
            Statement::VarDecl {
                init: Some(Expr::Null),
                ..
            } => {}
            _ => panic!(),
        }
    }

    #[test]
    fn extracts_scripts_from_dom() {
        let dom = Node::document(vec![Node::element(
            "body",
            vec![Node::element(
                "script",
                vec![Node::text("let x = 10;")],
            )],
        )]);

        let program = Program::from_dom(&dom);
        assert_eq!(program.body.len(), 1);

        match &program.body[0] {
            Statement::VarDecl { name, .. } => assert_eq!(name, "x"),
            _ => panic!(),
        }
    }

    #[test]
    fn displays_program_as_tree() {
        let program = Program::parse("let x = 42;");
        let output = program.to_string();

        assert!(output.contains("#program"));
        assert!(output.contains("var-decl"));
        assert!(output.contains("x"));
    }

    #[test]
    fn handles_comments() {
        let program = Program::parse(
            r#"
            // This is a comment
            let x = 5; /* block comment */ let y = 10;
            "#,
        );
        assert_eq!(program.body.len(), 2);
    }
}
