// Parser: トークン列をASTに組み立てる。手法は「再帰下降構文解析」— 文法規則ひとつが
// 関数ひとつに対応する定番の書き方。TS版(src/parser.ts、1217行)からの移植だが、
// ast.rsのコメントに書いたとおり実用サブセットに絞っている。
//
// エラー復帰(パニックモード)の枠組みはTS版と1:1で移植した — これは今後どの文法を
// 追加するときも再利用する土台なので、最初にきちんと作っておく価値がある。
//
// TSの`throw`は、ここでも`Result<T, Box<CompileError>>` + `?`に対応する
// (`Box`で包む理由はlexer.rsのコメント参照 — clippy::result_large_err対策)。
// parseProgram()だけは例外で、TS版と同じく「回復してでも必ずProgramを返す」設計
// (エラーはself.errorsに溜め込むだけで、呼び出し元のparse()がまとめて返す)なので
// 戻り値の型にResultを持たない(=失敗しない関数、という型で表現している)

use crate::ast::{Block, ConstDecl, ElseClause, Expr, FnDecl, IfStmt, Param, Program, Stmt, TypeNode};
use crate::lexer::lex;
use crate::token::{CompileError, Fix, Pos, Range, Token, TokenType};

// エラー復帰が収集する構文エラー件数の上限。病的に壊れた入力でカスケードが
// 延々続くのを防ぐ安全弁(通常のMeshファイルの規模では実質当たらない)
const MAX_PARSE_ERRORS: usize = 50;

// 本番の検査パイプラインが使う入口: 構文エラーがあれば全部集めて返す(TS版は「1件なら
// 素のCompileError、2件以上ならMultiCompileError」という互換維持のための型分けをしていたが、
// Rustには合わせるべき既存呼び出し側が無いので、常に`Vec`で統一する簡略化をしている
pub fn parse(source: &str) -> Result<Program, Vec<CompileError>> {
    let lex_output = lex(source, None).map_err(|e| vec![*e])?;
    let mut parser = Parser::new(lex_output.tokens);
    let program = parser.parse_program();
    if parser.errors.is_empty() {
        Ok(program)
    } else {
        Err(parser.errors)
    }
}

// テスト・デバッグ専用: 構文エラーがあっても投げず、パニックモード復帰後の
// ベストエフォートASTをそのまま返す。呼び出しはテスト1回きりでホットパスではないので、
// 公開APIとしての素直さ(Boxを漏らさない)を優先してclippy::result_large_errを許容する
#[allow(clippy::result_large_err)]
pub fn parse_ignoring_errors(source: &str) -> Result<Program, CompileError> {
    let lex_output = lex(source, None).map_err(|e| *e)?;
    let mut parser = Parser::new(lex_output.tokens);
    Ok(parser.parse_program())
}

// parseProgram内の1宣言の結果。TS版はfns/typesなど複数の配列へ直接pushしていたが、
// Rustでは「関数から返した値を呼び出し元がどの配列に振り分けるか」を明示する方が
// 素直(かつクロージャで複数のVecを同時に可変借用する面倒を避けられる)
enum TopLevelItem {
    Fn(FnDecl),
    Const(ConstDecl),
}

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    errors: Vec<CompileError>,
}

impl Parser {
    fn new(tokens: Vec<Token>) -> Self {
        Parser { tokens, pos: 0, errors: Vec::new() }
    }

    // ---- エラー復帰(パニックモード) ----

    // 構文エラーを1件記録し、次の再開点まで読み飛ばす。sync関数は最低1トークンは
    // 必ず消費することを保証する(でなければ無限ループになる)
    fn record_and_recover(&mut self, e: CompileError, start_pos: usize, sync: fn(&mut Self, usize)) {
        self.errors.push(e);
        if self.errors.len() >= MAX_PARSE_ERRORS {
            self.pos = self.tokens.len() - 1; // eofまで飛んで打ち切る(安全弁)
            return;
        }
        sync(self, start_pos);
    }

    // start_posからself.pos(エラーが投げられた時点)までの間に開いたまま閉じていない
    // { の深さを数える(エラー発生時点で構文的にまだ{の中にいることがあるため)
    fn brace_depth_since(&self, start_pos: usize) -> i32 {
        let mut depth = 0;
        for i in start_pos..self.pos {
            match self.tokens[i].kind {
                TokenType::LBrace => depth += 1,
                TokenType::RBrace => depth -= 1,
                _ => {}
            }
        }
        depth.max(0)
    }

    // トップレベル宣言の構文エラーから復帰: まず開いたままの{があれば閉じきり、
    // そのうえで次の宣言の先頭らしきトークンまで読み飛ばす
    fn sync_to_top_level(&mut self, start_pos: usize) {
        let mut depth = self.brace_depth_since(start_pos);
        while depth > 0 && !self.check(TokenType::Eof) {
            match self.peek().kind {
                TokenType::LBrace => depth += 1,
                TokenType::RBrace => depth -= 1,
                _ => {}
            }
            self.next();
        }
        while !self.check(TokenType::Eof) {
            if matches!(self.peek().kind, TokenType::Import | TokenType::Export | TokenType::Fn | TokenType::Struct | TokenType::Type)
                || (self.check(TokenType::Ident) && matches!(self.peek_at(1).kind, TokenType::ColonEq | TokenType::Colon))
            {
                break;
            }
            self.next();
        }
        if self.pos == start_pos {
            self.next(); // 前進保証
        }
    }

    // 文レベルの構文エラーから復帰: 次の文区切り(;)か、このブロックの終わり(})まで
    // 読み飛ばす。";"は消費して止まる。"}"は消費しない(囲むブロックの終了判定に譲る)
    fn sync_to_statement_boundary(&mut self, start_pos: usize) {
        let mut depth = self.brace_depth_since(start_pos);
        while !self.check(TokenType::Eof) {
            if depth == 0 && (self.check(TokenType::Semi) || self.check(TokenType::RBrace)) {
                break;
            }
            match self.peek().kind {
                TokenType::LBrace => depth += 1,
                TokenType::RBrace => depth -= 1,
                _ => {}
            }
            self.next();
        }
        self.eat(TokenType::Semi);
        if self.pos == start_pos {
            self.next();
        }
    }

    // ---- トークン操作ユーティリティ ----

    fn peek(&self) -> &Token {
        self.peek_at(0)
    }
    fn peek_at(&self, offset: usize) -> &Token {
        &self.tokens[(self.pos + offset).min(self.tokens.len() - 1)]
    }
    fn next(&mut self) -> Token {
        let t = self.peek().clone();
        if t.kind != TokenType::Eof {
            self.pos += 1;
        }
        t
    }
    fn check(&self, kind: TokenType) -> bool {
        self.peek().kind == kind
    }
    // TSの`match(type)`相当。Rustでは`match`が予約語なので`eat`という名前にした
    // (「一致したら食べて前進する」という定番の命名)
    fn eat(&mut self, kind: TokenType) -> bool {
        if self.check(kind) {
            self.next();
            true
        } else {
            false
        }
    }
    fn expect(&mut self, kind: TokenType, context: &str) -> Result<Token, Box<CompileError>> {
        if self.check(kind) {
            return Ok(self.next());
        }
        let t = self.peek().clone();
        let got = if t.kind == TokenType::Eof { "end of file".to_string() } else { format!("'{}'", t.value) };
        Err(self.error_at(t.pos, format!("expected '{kind}' {context}, but got {got}"), "syntax-error"))
    }
    fn skip_semis(&mut self) {
        while self.eat(TokenType::Semi) {}
    }
    fn error_at(&self, pos: Pos, message: impl Into<String>, code: &'static str) -> Box<CompileError> {
        Box::new(CompileError { message: message.into(), pos, code, fix: None })
    }
    fn error_here(&self, message: impl Into<String>, code: &'static str) -> Box<CompileError> {
        self.error_at(self.peek().pos, message, code)
    }

    // ---- 宣言 ----

    fn parse_program(&mut self) -> Program {
        let mut fns = Vec::new();
        let mut consts = Vec::new();
        self.skip_semis();
        while !self.check(TokenType::Eof) {
            let start_pos = self.pos;
            match self.parse_top_level_item() {
                Ok(TopLevelItem::Fn(f)) => fns.push(f),
                Ok(TopLevelItem::Const(c)) => consts.push(c),
                Err(e) => self.record_and_recover(*e, start_pos, Self::sync_to_top_level),
            }
            self.skip_semis();
        }
        Program { fns, consts }
    }

    fn parse_top_level_item(&mut self) -> Result<TopLevelItem, Box<CompileError>> {
        let exported = self.eat(TokenType::Export);
        // F-9c: トップレベル定数は常に不変(共有可変状態を作らないため)。'mut'はここでは使えない
        if self.check(TokenType::Mut) {
            return Err(self.error_here(
                "top-level bindings are always immutable — 'mut' is not allowed here \
                 (there are no mutable globals; pass mutable state as a parameter instead)",
                "top-level-mut-not-allowed",
            ));
        }
        if self.check(TokenType::Fn) {
            Ok(TopLevelItem::Fn(self.parse_fn_decl(exported)?))
        } else if self.check(TokenType::Ident) && matches!(self.peek_at(1).kind, TokenType::ColonEq | TokenType::Colon) {
            Ok(TopLevelItem::Const(self.parse_const_decl(exported)?))
        } else {
            let message = if exported {
                "'export' must be followed by a 'fn', 'struct', 'type' or constant (name := value) declaration"
            } else {
                "only 'fn', 'struct', 'type' declarations and top-level constants (name := value) are allowed at the top level"
            };
            Err(self.error_here(message, "invalid-top-level-declaration"))
        }
    }

    // F-9c: トップレベル定数。x := 10 / x: int = 10(常に不変)
    fn parse_const_decl(&mut self, exported: bool) -> Result<ConstDecl, Box<CompileError>> {
        let name_tok = self.expect(TokenType::Ident, "as constant name")?;
        if self.check(TokenType::Colon) {
            self.next();
            let type_node = self.parse_type()?;
            self.expect(TokenType::Eq, "in typed top-level constant ('name: T = value')")?;
            let value = self.parse_expr()?;
            return Ok(ConstDecl { name: name_tok.value, type_node: Some(type_node), value, exported, pos: name_tok.pos });
        }
        self.expect(TokenType::ColonEq, "after top-level constant name")?;
        let value = self.parse_expr()?;
        Ok(ConstDecl { name: name_tok.value, type_node: None, value, exported, pos: name_tok.pos })
    }

    fn parse_fn_decl(&mut self, exported: bool) -> Result<FnDecl, Box<CompileError>> {
        let start = self.expect(TokenType::Fn, "at start of function declaration")?;
        let name = self.expect(TokenType::Ident, "as function name")?.value;
        let params = self.parse_params()?;
        let ret = self.parse_return_type()?;
        let body = self.parse_block()?;
        Ok(FnDecl { name, params, ret, body, exported, pos: start.pos })
    }

    fn parse_params(&mut self) -> Result<Vec<Param>, Box<CompileError>> {
        self.expect(TokenType::LParen, "after function name")?;
        let mut params = Vec::new();
        while !self.check(TokenType::RParen) {
            let name_tok = self.expect(TokenType::Ident, "as parameter name")?;
            self.expect(TokenType::Colon, "after parameter name")?;
            let type_node = self.parse_type()?;
            params.push(Param { name: name_tok.value, type_node, pos: name_tok.pos });
            if !self.check(TokenType::RParen) {
                self.expect(TokenType::Comma, "between parameters")?;
            }
        }
        self.expect(TokenType::RParen, "after parameters")?;
        Ok(params)
    }

    // 戻り値の型: なし / 単一 `int` / union `int | error`(多値戻りは廃止)
    fn parse_return_type(&mut self) -> Result<Option<TypeNode>, Box<CompileError>> {
        if self.check(TokenType::LBrace) {
            return Ok(None);
        }
        if self.check(TokenType::LParen) {
            return Err(self.error_here(
                "multiple return values were removed — return one value (use a union type like 'int | error')",
                "multiple-return-values-removed",
            ));
        }
        Ok(Some(self.parse_type()?))
    }

    // 型 = 単一型を "|" でつないだもの: User | none | error
    fn parse_type(&mut self) -> Result<TypeNode, Box<CompileError>> {
        let first = self.parse_single_type()?;
        if !self.check(TokenType::Pipe) {
            return Ok(first);
        }
        let pos = first.pos();
        let mut members = vec![first];
        while self.eat(TokenType::Pipe) {
            members.push(self.parse_single_type()?);
        }
        Ok(TypeNode::Union { members, pos })
    }

    fn parse_single_type(&mut self) -> Result<TypeNode, Box<CompileError>> {
        self.parse_type_atom()
    }

    // 今回のスコープはname型(int, string, math.User)とnoneのみ。array/chan/map/fnType/
    // structType/literalは次回以降のPRで追加する
    fn parse_type_atom(&mut self) -> Result<TypeNode, Box<CompileError>> {
        if self.check(TokenType::NoneKw) {
            let t = self.next();
            return Ok(TypeNode::Name { name: "none".into(), pkg: None, pos: t.pos });
        }
        let name_tok = self.expect(TokenType::Ident, "as type name")?;
        // math.User — パッケージ修飾された型名
        if self.check(TokenType::Dot) && self.peek_at(1).kind == TokenType::Ident {
            self.next();
            let type_name = self.next();
            return Ok(TypeNode::Name { name: type_name.value, pkg: Some(name_tok.value), pos: name_tok.pos });
        }
        Ok(TypeNode::Name { name: name_tok.value, pkg: None, pos: name_tok.pos })
    }

    // ---- 文 ----

    fn parse_block(&mut self) -> Result<Block, Box<CompileError>> {
        self.expect(TokenType::LBrace, "at start of block")?;
        let mut stmts = Vec::new();
        self.skip_semis();
        while !self.check(TokenType::RBrace) && !self.check(TokenType::Eof) {
            let start_pos = self.pos;
            match self.parse_statement() {
                Ok(s) => stmts.push(s),
                Err(e) => self.record_and_recover(*e, start_pos, Self::sync_to_statement_boundary),
            }
            self.skip_semis();
        }
        self.expect(TokenType::RBrace, "at end of block")?;
        Ok(Block { stmts })
    }

    fn parse_statement(&mut self) -> Result<Stmt, Box<CompileError>> {
        let t = self.peek().clone();
        match t.kind {
            TokenType::Return => {
                self.next();
                let mut value = None;
                if !self.check(TokenType::Semi) && !self.check(TokenType::RBrace) {
                    value = Some(self.parse_expr()?);
                    if self.check(TokenType::Comma) {
                        return Err(self.error_here(
                            "multiple return values were removed — return one value (use a union type like 'int | error')",
                            "multiple-return-values-removed",
                        ));
                    }
                }
                Ok(Stmt::Return { value, pos: t.pos })
            }
            TokenType::If => self.parse_if().map(Stmt::If),
            TokenType::For => self.parse_for(),
            TokenType::Break => {
                self.next();
                Ok(Stmt::Break { pos: t.pos })
            }
            TokenType::Continue => {
                self.next();
                Ok(Stmt::Continue { pos: t.pos })
            }
            _ => self.parse_simple_stmt(),
        }
    }

    // 「単純文」: 代入 / 短縮変数宣言 / インクリメント / 式文。forのヘッダにも現れるので
    // 独立した関数にしている
    fn parse_simple_stmt(&mut self) -> Result<Stmt, Box<CompileError>> {
        let start = self.peek().clone();
        let mutable = self.eat(TokenType::Mut);

        let first = self.parse_expr()?;

        // x := ... / x, y := ... / x = ... / x, y = f()
        if self.check(TokenType::Comma) || self.check(TokenType::ColonEq) || self.check(TokenType::Eq) {
            let mut targets = vec![first];
            while self.eat(TokenType::Comma) {
                targets.push(self.parse_expr()?);
            }

            if self.eat(TokenType::ColonEq) {
                let mut names = Vec::new();
                for e in &targets {
                    match e {
                        Expr::Ident { name, .. } => names.push(name.clone()),
                        _ => return Err(self.error_at(e.pos(), "left side of ':=' must be a name", "invalid-assignment-target")),
                    }
                }
                let mut values = vec![self.parse_expr()?];
                while self.eat(TokenType::Comma) {
                    values.push(self.parse_expr()?);
                }
                return Ok(Stmt::ShortVarDecl { names, values, mutable, pos: start.pos });
            }
            if mutable {
                return Err(self.error_at(start.pos, "'mut' can only be used with a ':=' declaration", "misplaced-mut"));
            }

            self.expect(TokenType::Eq, "in assignment")?;
            for e in &targets {
                if !matches!(e, Expr::Ident { .. }) {
                    return Err(self.error_at(e.pos(), "invalid assignment target", "invalid-assignment-target"));
                }
            }
            let mut values = vec![self.parse_expr()?];
            while self.eat(TokenType::Comma) {
                values.push(self.parse_expr()?);
            }
            return Ok(Stmt::Assign { targets, values, compound_op: None, pos: start.pos });
        }

        // F-9b: 複合代入 x += 1(常に単一target/value)
        if matches!(self.peek().kind, TokenType::PlusEq | TokenType::MinusEq | TokenType::StarEq | TokenType::SlashEq | TokenType::PercentEq) {
            if mutable {
                return Err(self.error_at(start.pos, "'mut' can only be used with a ':=' declaration", "misplaced-mut"));
            }
            if !matches!(first, Expr::Ident { .. }) {
                return Err(self.error_at(first.pos(), "invalid assignment target", "invalid-assignment-target"));
            }
            let op_tok = self.next();
            // code review(PR #42): TS版は`opTok.type.slice(0, -1)`で末尾の"="を落とし、
            // 基本演算子("+"等)だけを持たせている(checker/codegen/formatterがそちらを
            // 前提にしている)。ここも同じく複合トークン(PlusEq等)ではなく基本演算子
            // (Plus等)に変換してから持たせる必要がある — 見落とすとテストでは気づけないまま
            // 将来のcodegen移植時に`(x += rhs)`のような不正なJSを吐く実質的なバグになる
            let base_op = match op_tok.kind {
                TokenType::PlusEq => TokenType::Plus,
                TokenType::MinusEq => TokenType::Minus,
                TokenType::StarEq => TokenType::Star,
                TokenType::SlashEq => TokenType::Slash,
                TokenType::PercentEq => TokenType::Percent,
                _ => unreachable!("guarded by the matches! check above"),
            };
            let value = self.parse_expr()?;
            return Ok(Stmt::Assign { targets: vec![first], values: vec![value], compound_op: Some(base_op), pos: start.pos });
        }

        if mutable {
            return Err(self.error_at(start.pos, "'mut' can only be used with a ':=' declaration", "misplaced-mut"));
        }

        // i++ / i--
        if matches!(self.peek().kind, TokenType::PlusPlus | TokenType::MinusMinus) {
            let op = self.next().kind;
            return Ok(Stmt::IncDec { target: first, op, pos: start.pos });
        }

        Ok(Stmt::ExprStmt { expr: first, pos: start.pos })
    }

    fn parse_if(&mut self) -> Result<IfStmt, Box<CompileError>> {
        let start = self.expect(TokenType::If, "at start of if statement")?;
        let cond = self.parse_expr()?; // Goと同じく条件に丸括弧は不要
        let then = self.parse_block()?;
        let else_ = if self.eat(TokenType::Else) {
            if self.check(TokenType::If) {
                Some(Box::new(ElseClause::If(self.parse_if()?)))
            } else {
                Some(Box::new(ElseClause::Block(self.parse_block()?)))
            }
        } else {
            None
        };
        Ok(IfStmt { cond, then, else_, pos: start.pos })
    }

    // forの3形態: `for { }` / `for cond { }` / `for init; cond; post { }`
    fn parse_for(&mut self) -> Result<Stmt, Box<CompileError>> {
        let start = self.expect(TokenType::For, "at start of for statement")?;

        if self.check(TokenType::LBrace) {
            return Ok(Stmt::For { init: None, cond: None, post: None, body: self.parse_block()?, pos: start.pos });
        }

        let first = self.parse_simple_stmt()?;

        if self.check(TokenType::LBrace) {
            let Stmt::ExprStmt { expr, .. } = first else {
                return Err(self.error_at(start.pos, "for condition must be an expression", "syntax-error"));
            };
            return Ok(Stmt::For { init: None, cond: Some(expr), post: None, body: self.parse_block()?, pos: start.pos });
        }

        self.expect(TokenType::Semi, "after for init statement")?;
        let cond = if self.check(TokenType::Semi) { None } else { Some(self.parse_expr()?) };
        self.expect(TokenType::Semi, "after for condition")?;
        let post = if self.check(TokenType::LBrace) { None } else { Some(Box::new(self.parse_simple_stmt()?)) };
        Ok(Stmt::For { init: Some(Box::new(first)), cond, post, body: self.parse_block()?, pos: start.pos })
    }

    // ---- 式(優先順位法 / Pratt parsing) ----

    fn parse_expr(&mut self) -> Result<Expr, Box<CompileError>> {
        self.parse_binary(1)
    }

    // 二項演算子の優先順位(大きいほど強く結合する)。今回のスコープでは`or`/`is`を
    // 対象外にしている(束縛形・型ターゲットという特別扱いが要るため、次回以降で追加)
    fn precedence(kind: TokenType) -> Option<u8> {
        use TokenType::*;
        Some(match kind {
            OrOr => 2,
            AndAnd => 3,
            EqEq | NotEq => 4,
            Lt | Le | Gt | Ge => 5,
            Plus | Minus => 6,
            Star | Slash | Percent => 7,
            _ => return None,
        })
    }

    fn parse_binary(&mut self, min_prec: u8) -> Result<Expr, Box<CompileError>> {
        let mut left = self.parse_unary()?;
        loop {
            let op = self.peek().kind;
            let Some(prec) = Self::precedence(op) else { return Ok(left) };
            if prec < min_prec {
                return Ok(left);
            }
            let op_tok = self.next();
            let right = self.parse_binary(prec + 1)?; // 左結合
            left = Expr::Binary { op: op_tok.kind, left: Box::new(left), right: Box::new(right), pos: op_tok.pos };
        }
    }

    fn parse_unary(&mut self) -> Result<Expr, Box<CompileError>> {
        let t = self.peek().clone();
        if matches!(t.kind, TokenType::Bang | TokenType::Minus) {
            self.next();
            let operand = self.parse_unary()?;
            return Ok(Expr::Unary { op: t.kind, operand: Box::new(operand), pos: t.pos });
        }
        let primary = self.parse_primary()?;
        self.parse_postfix(primary)
    }

    // 呼び出しは後置で連鎖する: f(x)(y)。index/member/prop(`?`)/struct literalは次回以降
    fn parse_postfix(&mut self, mut expr: Expr) -> Result<Expr, Box<CompileError>> {
        loop {
            if self.check(TokenType::Bang) {
                // 旧記法(2026-07-19に?へ改名)。負の転移対策の誘導エラー
                let bang_pos = self.peek().pos;
                return Err(Box::new(CompileError {
                    message: "postfix '!' was renamed — use '?' to propagate none/error to the caller".into(),
                    pos: bang_pos,
                    code: "postfix-bang-renamed",
                    fix: Some(Fix {
                        description: "replace '!' with '?'".into(),
                        range: Range { start: bang_pos, end: Pos { line: bang_pos.line, col: bang_pos.col + 1 } },
                        replacement: "?".into(),
                    }),
                }));
            }
            if self.check(TokenType::LParen) {
                self.next();
                let mut args = Vec::new();
                while !self.check(TokenType::RParen) {
                    args.push(self.parse_expr()?);
                    if !self.check(TokenType::RParen) {
                        self.expect(TokenType::Comma, "between arguments")?;
                    }
                }
                self.expect(TokenType::RParen, "after arguments")?;
                let call_pos = expr.pos();
                expr = Expr::Call { callee: Box::new(expr), args, pos: call_pos };
            } else {
                return Ok(expr);
            }
        }
    }

    fn parse_primary(&mut self) -> Result<Expr, Box<CompileError>> {
        let t = self.peek().clone();
        match t.kind {
            TokenType::Int => {
                self.next();
                Ok(Expr::Int { value: t.value, pos: t.pos })
            }
            TokenType::Float => {
                self.next();
                Ok(Expr::Float { value: t.value, pos: t.pos })
            }
            TokenType::Str => {
                self.next();
                if t.parts.is_some() {
                    // 文字列補間は次回以降(再字句解析が絡み複雑なため)。誠実に「未対応」と言う
                    return Err(self.error_at(
                        t.pos,
                        "string interpolation isn't supported by the Rust parser yet",
                        "unsupported-construct",
                    ));
                }
                Ok(Expr::String { value: t.value, pos: t.pos })
            }
            TokenType::True | TokenType::False => {
                self.next();
                Ok(Expr::Bool { value: t.kind == TokenType::True, pos: t.pos })
            }
            TokenType::Ident => {
                self.next();
                Ok(Expr::Ident { name: t.value, pos: t.pos })
            }
            TokenType::LParen => {
                self.next();
                let expr = self.parse_expr()?;
                self.expect(TokenType::RParen, "after expression")?;
                Ok(expr)
            }
            _ => {
                let text = if t.value.is_empty() { t.kind.to_string() } else { t.value.clone() };
                Err(self.error_at(t.pos, format!("unexpected '{text}'"), "syntax-error"))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::token::CompileError;

    // TS版のparser.test.tsにある`parseBody`ヘルパーと同じ意図: fn main(){...}に包んで
    // 中の文だけを取り出す
    fn parse_body(body: &str) -> Vec<Stmt> {
        parse(&format!("fn main() {{\n{body}\n}}")).unwrap().fns[0].body.stmts.clone()
    }

    #[test]
    fn 関数宣言_名前_引数_union戻り値() {
        let program = parse("fn divide(a: int, b: int) int | error { return a }").unwrap();
        let f = &program.fns[0];
        assert_eq!(f.name, "divide");
        assert_eq!(f.params.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(), vec!["a", "b"]);
        assert!(matches!(f.ret, Some(TypeNode::Union { .. })));
    }

    #[test]
    fn union型_3メンバー() {
        let program = parse("fn f() int | none | error { return 1 }").unwrap();
        let Some(TypeNode::Union { members, .. }) = &program.fns[0].ret else { panic!("expected union") };
        assert_eq!(members.len(), 3);
    }

    #[test]
    fn 多値戻りはエラーになる() {
        assert!(parse("fn f() (int, error) { return 1 }").unwrap_err()[0].message.contains("multiple return values"));
        assert!(parse("fn main() { return 1, 2 }").unwrap_err()[0].message.contains("multiple return values"));
    }

    #[test]
    fn 短縮変数宣言() {
        let stmts = parse_body("x := 1 + 2");
        let Stmt::ShortVarDecl { names, values, .. } = &stmts[0] else { panic!("expected shortVarDecl") };
        assert_eq!(names, &["x"]);
        assert!(matches!(values[0], Expr::Binary { .. }));
    }

    #[test]
    fn code_review_複合代入は末尾のequalを落とした基本演算子を持つ() {
        // TS版は opTok.type.slice(0, -1) で"+="→"+"に変換して持たせる(checker/codegen/
        // formatterがそちらを前提にしている)。ここが複合トークンのまま(PlusEq等)だと
        // 将来のcodegen移植時に不正なJSを吐く実質的なバグになる — code review(PR #42)で発見
        for (src, expected) in [
            ("x += 1", TokenType::Plus),
            ("x -= 1", TokenType::Minus),
            ("x *= 1", TokenType::Star),
            ("x /= 1", TokenType::Slash),
            ("x %= 1", TokenType::Percent),
        ] {
            let stmts = parse_body(src);
            let Stmt::Assign { compound_op, .. } = &stmts[0] else { panic!("expected assign for {src}") };
            assert_eq!(*compound_op, Some(expected), "for {src}");
        }
    }

    #[test]
    fn 多値の受け取り() {
        let stmts = parse_body("v, err := f()");
        let Stmt::ShortVarDecl { names, .. } = &stmts[0] else { panic!("expected shortVarDecl") };
        assert_eq!(names, &["v", "err"]);
    }

    #[test]
    fn 演算子の優先順位_1_plus_2_times_3() {
        let stmts = parse_body("x := 1 + 2 * 3");
        let Stmt::ShortVarDecl { values, .. } = &stmts[0] else { panic!("expected shortVarDecl") };
        let Expr::Binary { op, right, .. } = &values[0] else { panic!("expected binary") };
        assert_eq!(*op, TokenType::Plus);
        assert!(matches!(**right, Expr::Binary { .. }));
    }

    #[test]
    fn forの3形態() {
        let stmts = parse_body("for i := 0; i < 10; i++ {\n}");
        let Stmt::For { init, post, .. } = &stmts[0] else { panic!("expected for") };
        assert!(matches!(init.as_deref(), Some(Stmt::ShortVarDecl { .. })));
        assert!(matches!(post.as_deref(), Some(Stmt::IncDec { .. })));

        let stmts = parse_body("for x < 10 {\n}");
        let Stmt::For { init, cond, .. } = &stmts[0] else { panic!("expected for") };
        assert!(init.is_none());
        assert!(matches!(cond, Some(Expr::Binary { .. })));

        let stmts = parse_body("for {\nbreak\n}");
        let Stmt::For { cond, .. } = &stmts[0] else { panic!("expected for") };
        assert!(cond.is_none());
    }

    #[test]
    fn トップレベルはfn定数のみ() {
        assert!(parse("print(1)").is_err());
        assert!(parse("if true {}").is_err());
    }

    #[test]
    fn f9c_トップレベル定数() {
        let program = parse("x := 1\ny: string = \"a\"\nexport z := true\nfn main() {}").unwrap();
        assert_eq!(program.consts.len(), 3);
        assert_eq!(program.consts[0].name, "x");
        assert!(program.consts[0].type_node.is_none());
        assert!(!program.consts[0].exported);
        assert_eq!(program.consts[1].name, "y");
        assert!(program.consts[1].type_node.is_some());
        assert_eq!(program.consts[2].name, "z");
        assert!(program.consts[2].exported);
    }

    #[test]
    fn f9c_トップレベルのmutは使えない() {
        assert!(parse("mut x := 1").is_err());
    }

    #[test]
    fn 旧記法の後置bangはpostfix_bang_renamedとfixを持つ() {
        let errors = parse("fn f() int | error { return 1 }\nfn main() { x := f()!\nprint(x) }").unwrap_err();
        let e = &errors[0];
        assert_eq!(e.code, "postfix-bang-renamed");
        let fix = e.fix.as_ref().expect("expected a fix");
        assert_eq!(fix.replacement, "?");
        assert_eq!(fix.range.start, Pos { line: 2, col: 21 });
        assert_eq!(fix.range.end, Pos { line: 2, col: 22 });
    }

    #[test]
    fn 構文エラーの一般形はsyntax_errorコードを持つ() {
        let errors = parse("fn main() { x := }").unwrap_err();
        assert_eq!(errors[0].code, "syntax-error");
    }

    #[test]
    fn 退行防止_構文エラーが1件だけならその1件だけを返す() {
        let errors = parse("fn main() { x := }").unwrap_err();
        assert_eq!(errors.len(), 1);
    }

    #[test]
    fn 同じ関数内の複数の文にある独立した構文エラーを両方報告する() {
        let errors = parse("fn main() {\n\tx := 1 + * 2\n\ty := 3 + * 4\n\tprint(x, y)\n}").unwrap_err();
        assert_eq!(errors.len(), 2);
        assert!(errors[0].message.contains("unexpected '*'"));
        assert!(errors[1].message.contains("unexpected '*'"));
    }

    #[test]
    fn エラー文を読み飛ばしても直後の別の関数宣言を誤認しない() {
        let program = parse_ignoring_errors("fn main() {\n\tx := 1 + * 2\n}\nfn other() { print(2) }").unwrap();
        assert_eq!(program.fns.iter().map(|f| f.name.as_str()).collect::<Vec<_>>(), vec!["main", "other"]);
    }

    #[test]
    fn compile_errorを両方の入口から取得できることの確認() {
        // parse_ignoring_errorsはlexエラーだけCompileErrorをそのまま返す(recoveryはしない)ことの確認
        let err: CompileError = parse_ignoring_errors("fn main() { x := \"unterminated\n}").unwrap_err();
        assert_eq!(err.code, "unterminated-string");
    }
}
