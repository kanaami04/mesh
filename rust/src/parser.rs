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

use crate::ast::{
    Block, ConstDecl, ElseClause, Expr, FnDecl, IfStmt, ImportDecl, InterpSegment, MatchArm, MatchPattern, Param,
    Program, SelectArm, Stmt, StructFieldNode, StructLitField, TypeDecl, TypeNode,
};
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
    Type(TypeDecl),
}

// 文字列補間のネスト(`"${"${"${...}"}"}"`のような形)は、1段ごとにlex()→新しいParser→
// parse_expr()→...→parse_primary()という実フレームを積むため、上限が無いと本物の
// スタックオーバーフローでプロセスごと落ちる(code reviewで指摘・実機で再現)。
// TS版は同じ再帰設計だがJSの呼び出しスタック超過は捕捉可能な例外(RangeError)であり、
// 実際`src/cli.ts`がparse()呼び出しをtry/catchで包んでいるため実害が無い。Rustの
// スタックオーバーフローはResult/?/panic!のいずれでも捕捉できないため、この差は
// 移植によって新たに生まれた深刻度の格上げであり、放置できない。
// 実際のMeshコードが補間をこの深さまでネストすることは実質無い、という前提の余裕を
// 持った上限(TS側にも無い制限だが、Rustではプロセスクラッシュとの引き換えになるため
// Rust版だけが持つ安全弁として導入する)
const MAX_INTERP_DEPTH: usize = 64;

struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    errors: Vec<CompileError>,
    // if/forのヘッダでは`User{...}`を禁止する(ブロック開始の`{`と曖昧になるため。Goと同じ規則)。
    // struct literalを扱うようになったので今回から必要になった
    allow_struct_lit: bool,
    // 文字列補間のネスト深さ。新しいParserを作るたびに増える(new()自体は常に0から
    // 始まるので、補間の再帰呼び出し側が生成直後に明示的に設定する)
    interp_depth: usize,
}

impl Parser {
    fn new(tokens: Vec<Token>) -> Self {
        Parser { tokens, pos: 0, errors: Vec::new(), allow_struct_lit: true, interp_depth: 0 }
    }

    // allow_struct_litを一時的にvalueにしてfを実行し、終わったら(?で早期returnした場合も
    // 含めて)必ず元に戻す。fが`Result`を返す形にしているのはこのため——ループの後で
    // 素朴に戻すと、ループ内の`?`が復元をすり抜ける罠になる(code review, PR #43で指摘)
    fn with_struct_lit_flag<T>(
        &mut self,
        value: bool,
        f: impl FnOnce(&mut Self) -> Result<T, Box<CompileError>>,
    ) -> Result<T, Box<CompileError>> {
        let saved = self.allow_struct_lit;
        self.allow_struct_lit = value;
        let result = f(self);
        self.allow_struct_lit = saved;
        result
    }

    // TSの`withoutStructLit`相当
    fn with_no_struct_lit<T>(&mut self, f: impl FnOnce(&mut Self) -> Result<T, Box<CompileError>>) -> Result<T, Box<CompileError>> {
        self.with_struct_lit_flag(false, f)
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
        Err(self.error_at(t.pos, format!("expected '{kind}' {context}, but got {}", Self::describe_token(&t)), "syntax-error"))
    }
    // トークンをエラーメッセージ用に人が読める形にする。EOFは"end of file"(引用符なし)。
    // 補間つき文字列トークンは`value`が空文字列のまま(lexer.rs参照)なので、それを
    // そのまま引用符で囲むと`unexpected ''`のような空の表示になってしまう —
    // その場合は種別名(例: "string")にフォールバックする。判定は`value.is_empty()`ではなく
    // `parts.is_some()`で行う(素の空文字列リテラル`""`もvalueが空文字列になるため、
    // value基準だと誤って種別名にフォールバックしてしまう — code reviewでの指摘)
    fn describe_token(t: &Token) -> String {
        if t.kind == TokenType::Eof {
            return "end of file".to_string();
        }
        let text = if t.parts.is_some() { t.kind.to_string() } else { t.value.clone() };
        // 値に`'`が含まれると引用符の対応が崩れて表示が壊れるため(例: `unexpected 'it's a test'`)、
        // エスケープしてから引用符で囲む(code reviewでの指摘)
        format!("'{}'", text.replace('\'', "\\'"))
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
        let mut imports = Vec::new();
        let mut fns = Vec::new();
        let mut consts = Vec::new();
        let mut types = Vec::new();
        self.skip_semis();
        // importはファイル先頭にまとめる(宣言が始まったら以後のimportはエラー)
        while self.check(TokenType::Import) {
            let start_pos = self.pos;
            match self.parse_import_decl() {
                Ok(i) => imports.push(i),
                Err(e) => self.record_and_recover(*e, start_pos, Self::sync_to_top_level),
            }
            self.skip_semis();
        }
        while !self.check(TokenType::Eof) {
            let start_pos = self.pos;
            match self.parse_top_level_item() {
                Ok(TopLevelItem::Fn(f)) => fns.push(f),
                Ok(TopLevelItem::Const(c)) => consts.push(c),
                Ok(TopLevelItem::Type(t)) => types.push(t),
                Err(e) => self.record_and_recover(*e, start_pos, Self::sync_to_top_level),
            }
            self.skip_semis();
        }
        Program { imports, fns, consts, types }
    }

    // import "math" — v1制限: パスは単一セグメントのみ(examples/mathutil相当のパッケージ名を
    // そのまま指す)。パスの最終セグメントが修飾名(alias)になる
    fn parse_import_decl(&mut self) -> Result<ImportDecl, Box<CompileError>> {
        let start = self.expect(TokenType::Import, "at start of import")?;
        let path_tok = self.expect(TokenType::Str, "as import path (like: import \"math\")")?;
        if path_tok.parts.is_some() {
            return Err(self.error_at(path_tok.pos, "import path cannot use string interpolation", "invalid-import-path"));
        }
        let path = path_tok.value;
        // TS版と同じく`path.split("/").pop()`相当(pathが空でもsplitは必ず1要素返すため
        // 実質到達しないフォールバックだが、TS版の`?? path`に忠実に合わせてある)
        let alias = path.split('/').next_back().unwrap_or(path.as_str()).to_string();
        if alias.is_empty() {
            return Err(self.error_at(path_tok.pos, "import path cannot be empty", "invalid-import-path"));
        }
        Ok(ImportDecl { path, alias, pos: start.pos })
    }

    fn parse_top_level_item(&mut self) -> Result<TopLevelItem, Box<CompileError>> {
        if self.check(TokenType::Import) {
            return Err(self.error_here("imports must come before all declarations", "import-order"));
        }
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
        } else if self.check(TokenType::Struct) {
            Ok(TopLevelItem::Type(self.parse_struct_decl(exported)?))
        } else if self.check(TokenType::Type) {
            Ok(TopLevelItem::Type(self.parse_type_decl(exported)?))
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

    // struct User { name: string  age: int } — 意味的には型への名付け(typeと同じ)なので
    // TypeDecl として登録する。フィールドは改行区切り。error/jsonマーカーは次回以降
    fn parse_struct_decl(&mut self, exported: bool) -> Result<TypeDecl, Box<CompileError>> {
        let start = self.expect(TokenType::Struct, "at start of struct declaration")?;
        let name = self.expect(TokenType::Ident, "as struct name")?.value;
        self.expect(TokenType::LBrace, "after struct name")?;
        self.skip_semis();
        let mut fields = Vec::new();
        while !self.check(TokenType::RBrace) && !self.check(TokenType::Eof) {
            let fname = self.expect(TokenType::Ident, "as field name")?;
            self.expect(TokenType::Colon, "after field name")?;
            let type_node = self.parse_type()?;
            fields.push(StructFieldNode { name: fname.value, type_node, pos: fname.pos });
            self.skip_semis();
        }
        self.expect(TokenType::RBrace, "at end of struct declaration")?;
        Ok(TypeDecl { name, node: TypeNode::StructType { fields, pos: start.pos }, exported, pos: start.pos })
    }

    // type Status = "active" | "banned" / type X = { kind: "ok" } | { kind: "notFound" }(判別可能union)。
    // 長いunionは複数行に折れる — 行末`|`と行頭`|`(複数行の`;`越しの継続)の両方を許す
    fn parse_type_decl(&mut self, exported: bool) -> Result<TypeDecl, Box<CompileError>> {
        let start = self.expect(TokenType::Type, "at start of type declaration")?;
        let name = self.expect(TokenType::Ident, "as type name")?.value;
        let eq = self.expect(TokenType::Eq, "after type name")?;
        let first = self.parse_union_member()?;
        let mut members = vec![first];
        while self.match_union_continuation() {
            members.push(self.parse_union_member()?);
        }
        if members.len() == 1 {
            let first = members.into_iter().next().unwrap();
            if let TypeNode::StructType { ref fields, pos } = first {
                // fix: `type Name =`を`struct Name`に置き換えれば、続く`{ ... }`はそのまま使える —
                // ただしこれが安全なのはフィールドが1行1つ(改行区切り)のときだけ
                let one_field_per_line =
                    fields.iter().enumerate().all(|(i, f)| i == 0 || f.pos.line != fields[i - 1].pos.line);
                let fix = if one_field_per_line {
                    Some(Fix {
                        description: format!("replace 'type {name} =' with 'struct {name}'"),
                        range: Range { start: start.pos, end: Pos { line: eq.pos.line, col: eq.pos.col + 1 } },
                        replacement: format!("struct {name}"),
                    })
                } else {
                    None
                };
                return Err(Box::new(CompileError {
                    message: format!("use 'struct {name} {{ ... }}' to define a data shape ('{{...}}' alone is only allowed inside a union)"),
                    pos,
                    code: "bare-struct-shape",
                    fix,
                }));
            }
            return Ok(TypeDecl { name, node: first, exported, pos: start.pos });
        }
        let pos = members[0].pos();
        Ok(TypeDecl { name, node: TypeNode::Union { members, pos }, exported, pos: start.pos })
    }

    // unionの継続`|`を読む。行頭`|`スタイルでは直前の改行がASIで';'になっているので、
    // ';'の並びの先に`|`があればそこまでまとめて消費して継続とみなす
    fn match_union_continuation(&mut self) -> bool {
        let mut i = 0;
        while self.peek_at(i).kind == TokenType::Semi {
            i += 1;
        }
        if self.peek_at(i).kind != TokenType::Pipe {
            return false;
        }
        for _ in 0..=i {
            self.next();
        }
        true
    }

    // unionの1メンバー: 無名struct型{...}(判別可能union用)か、通常の単一型
    fn parse_union_member(&mut self) -> Result<TypeNode, Box<CompileError>> {
        if self.check(TokenType::LBrace) {
            self.parse_inline_struct_type()
        } else {
            self.parse_single_type()
        }
    }

    // { kind: "ok", user: User } — 無名の構造体型リテラル。フィールドはカンマまたは改行区切り
    fn parse_inline_struct_type(&mut self) -> Result<TypeNode, Box<CompileError>> {
        let start = self.expect(TokenType::LBrace, "at start of inline struct type")?;
        self.skip_semis();
        let mut fields = Vec::new();
        while !self.check(TokenType::RBrace) && !self.check(TokenType::Eof) {
            let fname = self.expect(TokenType::Ident, "as field name")?;
            self.expect(TokenType::Colon, "after field name")?;
            let type_node = self.parse_type()?;
            fields.push(StructFieldNode { name: fname.value, type_node, pos: fname.pos });
            self.eat(TokenType::Comma);
            self.skip_semis();
        }
        self.expect(TokenType::RBrace, "at end of inline struct type")?;
        Ok(TypeNode::StructType { fields, pos: start.pos })
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

    // 今回のスコープはname型(int, string, math.User)・文字列リテラル型・none・chan<T>のみ。
    // array/map/fnTypeは次回以降のPRで追加する
    fn parse_type_atom(&mut self) -> Result<TypeNode, Box<CompileError>> {
        if self.check(TokenType::NoneKw) {
            let t = self.next();
            return Ok(TypeNode::Name { name: "none".into(), pkg: None, pos: t.pos });
        }
        // 文字列リテラル型: "active"(判別可能unionのタグに必須)
        if self.check(TokenType::Str) {
            let t = self.next();
            if t.parts.is_some() {
                return Err(self.error_at(t.pos, "interpolation cannot be used in a type", "interpolation-in-type"));
            }
            return Ok(TypeNode::Literal { value: t.value, pos: t.pos });
        }
        if self.check(TokenType::Chan) {
            let start = self.next();
            self.expect(TokenType::Lt, "after 'chan'")?;
            let elem = self.parse_type()?;
            self.expect(TokenType::Gt, "after channel element type")?;
            return Ok(TypeNode::Chan { elem: Box::new(elem), pos: start.pos });
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
            TokenType::Wait => {
                self.next();
                let body = self.parse_block()?;
                Ok(Stmt::Wait { body, pos: t.pos })
            }
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

        // 型注釈つき宣言: x: T = v  /  mut best: string | none = none
        if self.check(TokenType::Ident) && self.peek_at(1).kind == TokenType::Colon {
            let name_tok = self.next();
            self.next(); // :
            let type_node = self.parse_type()?;
            self.expect(TokenType::Eq, "in typed declaration ('name: T = value')")?;
            let value = self.parse_expr()?;
            return Ok(Stmt::TypedVarDecl { name: name_tok.value, type_node, value, mutable, pos: start.pos });
        }

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

        // ch <- v
        if self.eat(TokenType::Arrow) {
            let value = self.parse_expr()?;
            return Ok(Stmt::Send { channel: first, value, pos: start.pos });
        }

        Ok(Stmt::ExprStmt { expr: first, pos: start.pos })
    }

    fn parse_if(&mut self) -> Result<IfStmt, Box<CompileError>> {
        let start = self.expect(TokenType::If, "at start of if statement")?;
        // Goと同じく条件に丸括弧は不要。`if User{...} {`のような曖昧さを避けるため、
        // 条件式の中ではstruct literalを禁止する(ブロック開始の`{`と区別できないため)
        let cond = self.with_no_struct_lit(|p| p.parse_expr())?;
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

        let first = self.with_no_struct_lit(|p| p.parse_simple_stmt())?;

        if self.check(TokenType::LBrace) {
            let Stmt::ExprStmt { expr, .. } = first else {
                return Err(self.error_at(start.pos, "for condition must be an expression", "syntax-error"));
            };
            return Ok(Stmt::For { init: None, cond: Some(expr), post: None, body: self.parse_block()?, pos: start.pos });
        }

        self.expect(TokenType::Semi, "after for init statement")?;
        let cond = if self.check(TokenType::Semi) { None } else { Some(self.with_no_struct_lit(|p| p.parse_expr())?) };
        self.expect(TokenType::Semi, "after for condition")?;
        let post = if self.check(TokenType::LBrace) {
            None
        } else {
            Some(Box::new(self.with_no_struct_lit(|p| p.parse_simple_stmt())?))
        };
        Ok(Stmt::For { init: Some(Box::new(first)), cond, post, body: self.parse_block()?, pos: start.pos })
    }

    // ---- 式(優先順位法 / Pratt parsing) ----

    fn parse_expr(&mut self) -> Result<Expr, Box<CompileError>> {
        self.parse_binary(1)
    }

    // 文字列補間の中身のように「式1つで完結する」入力をパースする(TS版と同じ)
    fn parse_standalone_expr(&mut self) -> Result<Expr, Box<CompileError>> {
        let expr = self.parse_expr()?;
        self.skip_semis();
        if !self.check(TokenType::Eof) {
            let t = self.peek().clone();
            return Err(self.error_at(t.pos, format!("unexpected {} in string interpolation", Self::describe_token(&t)), "syntax-error"));
        }
        Ok(expr)
    }

    // 二項演算子の優先順位(大きいほど強く結合する)
    fn precedence(kind: TokenType) -> Option<u8> {
        use TokenType::*;
        Some(match kind {
            Or => 1, // f() or fallback は最も弱く結合する
            OrOr => 2,
            AndAnd => 3,
            EqEq | NotEq | Is => 4, // x is none
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
            // x is none — 右辺は式ではなく型(matchのパターンと同じ: 型名 or 部分構造{...})
            if op == TokenType::Is {
                let target = if self.check(TokenType::LBrace) { self.parse_inline_struct_type()? } else { self.parse_single_type()? };
                left = Expr::Is { operand: Box::new(left), target, pos: op_tok.pos };
                continue;
            }
            // f() or fallback(noneのみ) / f() or e => fallback(失敗値を束縛。errorを含むなら必須)
            if op == TokenType::Or {
                let mut binding = None;
                if self.check(TokenType::Ident) && self.peek_at(1).kind == TokenType::FatArrow {
                    binding = Some(self.next().value);
                    self.next(); // =>
                }
                let right = self.parse_binary(prec + 1)?;
                left = Expr::OrElse { left: Box::new(left), right: Box::new(right), binding, pos: op_tok.pos };
                continue;
            }
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
        // <-ch — チャネル受信
        if t.kind == TokenType::Arrow {
            self.next();
            let channel = self.parse_unary()?;
            return Ok(Expr::Recv { channel: Box::new(channel), pos: t.pos });
        }
        // spawn f(x) / detach f(x) — 並行起動して受取口を返す式。
        // spawnは今の関数が所有(関数を抜けるとき暗黙wait)、detachはプログラムが所有
        if matches!(t.kind, TokenType::Spawn | TokenType::Detach) {
            self.next();
            let call = self.parse_unary()?;
            if !matches!(call, Expr::Call { .. }) {
                return Err(self.error_at(t.pos, format!("'{}' must be followed by a function call", t.kind), "invalid-spawn-target"));
            }
            return Ok(Expr::Spawn { call: Box::new(call), detached: t.kind == TokenType::Detach, pos: t.pos });
        }
        let primary = self.parse_primary()?;
        self.parse_postfix(primary)
    }

    // structリテラルの中身`{ field: value, ... }`を読む(名前の直後の`{`から)。
    // pkgはパッケージ修飾(math.Point{...})のときだけSome
    fn parse_struct_lit_body(&mut self, pkg: Option<String>, name: String, pos: Pos) -> Result<Expr, Box<CompileError>> {
        self.next(); // {
        self.skip_semis();
        // フィールド値の中では再びstruct literalを許可する(ネストしたliteral用)
        let fields = self.with_struct_lit_flag(true, |p| {
            let mut fields = Vec::new();
            while !p.check(TokenType::RBrace) && !p.check(TokenType::Eof) {
                let fname = p.expect(TokenType::Ident, "as field name")?;
                p.expect(TokenType::Colon, "after field name")?;
                let value = p.parse_expr()?;
                fields.push(StructLitField { name: fname.value, value, pos: fname.pos });
                p.eat(TokenType::Comma);
                p.skip_semis();
            }
            Ok(fields)
        })?;
        self.expect(TokenType::RBrace, "at end of struct literal")?;
        Ok(Expr::StructLit { name, pkg, fields, pos })
    }

    // 呼び出し・メンバアクセス・structリテラルは後置で連鎖する: f(x)[0].name。
    // 添字(`[`)は次回以降
    fn parse_postfix(&mut self, mut expr: Expr) -> Result<Expr, Box<CompileError>> {
        loop {
            // 修飾structリテラル: math.Point{x: 1, y: 2}(importしたパッケージのexported struct)
            if let Expr::Member { target, name, .. } = &expr
                && let Expr::Ident { name: pkg, .. } = &**target
                && self.allow_struct_lit && self.check(TokenType::LBrace) {
                    let member_pos = expr.pos();
                    expr = self.parse_struct_lit_body(Some(pkg.clone()), name.clone(), member_pos)?;
                    continue;
                }
            // structリテラル: User{name: "alice", age: 30}(カンマまたは改行区切り)
            if let Expr::Ident { name, pos } = &expr
                && self.allow_struct_lit && self.check(TokenType::LBrace) {
                    expr = self.parse_struct_lit_body(None, name.clone(), *pos)?;
                    continue;
                }
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
            if self.eat(TokenType::Question) {
                // f()? — 伝播。直後が文字列リテラルなら文脈つき: f() ? "line ${i}: bad"
                // (文脈は文字列リテラル/補間のみ。任意の式を許すと`f()? - 1`等が曖昧になる)
                let context = if self.check(TokenType::Str) { Some(Box::new(self.parse_primary()?)) } else { None };
                let prop_pos = expr.pos();
                expr = Expr::Prop { operand: Box::new(expr), context, pos: prop_pos };
                continue;
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
            } else if self.eat(TokenType::Dot) {
                let name = self.expect(TokenType::Ident, "after '.'")?.value;
                let member_pos = expr.pos();
                expr = Expr::Member { target: Box::new(expr), name, pos: member_pos };
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
                if let Some(parts) = t.parts {
                    // 補間つき文字列: 式の断片を(元の位置情報つきで)再帰的にパースする。
                    // TS版(parser.tsのparsePrimary)と同じ、lexer.rsが切り出した未パースの
                    // ソース断片を`lex()`で再字句解析し、新しいParserでparse_standalone_expr
                    // を呼ぶ形。エラーはそのまま`?`で外側に伝播する(TS版のthrowと同じ挙動)。
                    // ネスト深さの上限だけはTS版に無い、Rust固有の安全弁(MAX_INTERP_DEPTHの
                    // コメント参照)
                    if self.interp_depth >= MAX_INTERP_DEPTH {
                        return Err(self.error_at(
                            t.pos,
                            format!("string interpolation nested too deeply (max {MAX_INTERP_DEPTH})"),
                            "interpolation-too-deep",
                        ));
                    }
                    let mut segments = Vec::with_capacity(parts.len());
                    for part in parts {
                        segments.push(match part {
                            crate::token::StringPart::Text { text } => InterpSegment::Text { text },
                            crate::token::StringPart::Expr { source, pos } => {
                                let lexed = lex(&source, Some(pos))?;
                                let mut nested = Parser::new(lexed.tokens);
                                nested.interp_depth = self.interp_depth + 1;
                                let expr = nested.parse_standalone_expr()?;
                                InterpSegment::Expr { expr: Box::new(expr) }
                            }
                        });
                    }
                    return Ok(Expr::Interp { segments, pos: t.pos });
                }
                Ok(Expr::String { value: t.value, pos: t.pos })
            }
            TokenType::True | TokenType::False => {
                self.next();
                Ok(Expr::Bool { value: t.kind == TokenType::True, pos: t.pos })
            }
            TokenType::NoneKw => {
                self.next();
                Ok(Expr::None { pos: t.pos })
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
            TokenType::Match => {
                // match式: match r { error => "failed"  int => "ok"  _ => "?" }
                self.next();
                let subject = self.with_no_struct_lit(|p| p.parse_expr())?;
                self.expect(TokenType::LBrace, "after match subject")?;
                self.skip_semis();
                let mut arms = Vec::new();
                while !self.check(TokenType::RBrace) && !self.check(TokenType::Eof) {
                    let arm_pos = self.peek().pos;
                    let mut patterns = vec![self.parse_match_pattern()?];
                    while self.eat(TokenType::Comma) {
                        patterns.push(self.parse_match_pattern()?);
                    }
                    self.expect(TokenType::FatArrow, "after match pattern")?;
                    let body = self.parse_expr()?;
                    arms.push(MatchArm { patterns, body, pos: arm_pos });
                    self.skip_semis();
                }
                self.expect(TokenType::RBrace, "at end of match")?;
                Ok(Expr::Match { subject: Box::new(subject), arms, pos: t.pos })
            }
            TokenType::Select => {
                // select { v := <-ch1 => ...  v := <-ch2 => ...  _ => ... }
                // "_"は非ブロッキング用のdefaultアーム(あれば最大1つ)。matchと見た目は揃えるが、
                // パターンが「型」ではなく「どのchannel操作が先に終わったか」なので独立構文にしてある
                self.next();
                self.expect(TokenType::LBrace, "after 'select'")?;
                self.skip_semis();
                let mut arms = Vec::new();
                let mut default_arm = None;
                while !self.check(TokenType::RBrace) && !self.check(TokenType::Eof) {
                    let arm_t = self.peek().clone();
                    if arm_t.kind == TokenType::Ident && arm_t.value == "_" {
                        self.next();
                        if default_arm.is_some() {
                            return Err(self.error_at(arm_t.pos, "select can only have one default ('_') arm", "multiple-select-defaults"));
                        }
                        self.expect(TokenType::FatArrow, "after '_' in select")?;
                        default_arm = Some(Box::new(self.parse_expr()?));
                    } else {
                        let name_tok = self.expect(TokenType::Ident, "as select binding name")?;
                        self.expect(TokenType::ColonEq, "in select arm ('name := <-ch => body')")?;
                        self.expect(TokenType::Arrow, "select arms receive from a channel ('name := <-ch => body')")?;
                        let channel = self.parse_expr()?;
                        self.expect(TokenType::FatArrow, "after select arm channel")?;
                        let body = self.parse_expr()?;
                        arms.push(SelectArm { name: name_tok.value, channel, body, pos: arm_t.pos });
                    }
                    self.skip_semis();
                }
                self.expect(TokenType::RBrace, "at end of select")?;
                Ok(Expr::Select { arms, default_arm, pos: t.pos })
            }
            TokenType::Chan => {
                // チャネル生成: chan<int>(none)(無制限バッファ) / chan<int>(n)(容量n、送信がブロックしうる)。
                // F-11: 容量は常に明示必須(省略はできない — 無制限を選ぶこと自体はnoneで引き続き可能)
                self.next();
                self.expect(TokenType::Lt, "after 'chan'")?;
                let elem = self.parse_type()?;
                self.expect(TokenType::Gt, "after channel element type")?;
                self.expect(TokenType::LParen, "to create a channel: chan<T>(capacity) or chan<T>(none)")?;
                if self.check(TokenType::RParen) {
                    return Err(self.error_at(
                        self.peek().pos,
                        "chan<T>() no longer defaults to an unbounded buffer (F-11) — write chan<T>(none) for an unbounded channel, or chan<T>(n) for one that blocks sends once n values are buffered",
                        "chan-capacity-required",
                    ));
                }
                let capacity = self.parse_expr()?;
                self.expect(TokenType::RParen, "to create a channel: chan<T>(capacity) or chan<T>(none)")?;
                Ok(Expr::Chan { elem, capacity: Box::new(capacity), pos: t.pos })
            }
            _ => Err(self.error_at(t.pos, format!("unexpected {}", Self::describe_token(&t)), "syntax-error")),
        }
    }

    fn parse_match_pattern(&mut self) -> Result<MatchPattern, Box<CompileError>> {
        let t = self.peek().clone();
        if t.kind == TokenType::Ident && t.value == "_" {
            self.next();
            return Ok(MatchPattern::Wildcard { pos: t.pos });
        }
        // 判別可能union用の部分構造パターン: { kind: "ok" }。書いたフィールドが一致する
        // unionメンバーへ絞り込む
        if t.kind == TokenType::LBrace {
            return Ok(MatchPattern::Type(self.parse_inline_struct_type()?));
        }
        Ok(MatchPattern::Type(self.parse_single_type()?))
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
    fn トークン表示_値に引用符を含む場合はエスケープする() {
        // 修正前は`text`をエスケープせずそのまま引用符で囲んでいたため、値に`'`が
        // 含まれると`unexpected 'it's a test'`のように引用符の対応が崩れて表示が壊れていた
        let errors = parse("struct \"it's a test\" {}").unwrap_err();
        assert!(errors[0].message.contains("'it\\'s a test'"), "got: {}", errors[0].message);
    }

    #[test]
    fn トークン表示_空文字列リテラルは種別名にフォールバックしない() {
        // 修正前は`value.is_empty()`を「補間つき文字列トークンか」の代理指標にしていたため、
        // 素の空文字列リテラル`""`(value=""・parts=None)も種別名(例: "string")に
        // フォールバックしてしまっていた。`parts.is_some()`で判定すればこの衝突は起きない
        let errors = parse("struct \"\" {}").unwrap_err();
        assert!(errors[0].message.contains("''"), "got: {}", errors[0].message);
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

    // ---- struct/type宣言・構造体リテラル・member access・is/match(今回追加分) ----

    #[test]
    fn struct宣言とリテラルとmember_access() {
        let program = parse("struct User {\n\tname: string\n\tage: int\n}\nfn main() {\n\tu := User{name: \"alice\", age: 30}\n\tprint(u.name)\n}").unwrap();
        assert_eq!(program.types.len(), 1);
        let TypeNode::StructType { fields, .. } = &program.types[0].node else { panic!("expected structType") };
        assert_eq!(fields.iter().map(|f| f.name.as_str()).collect::<Vec<_>>(), vec!["name", "age"]);

        let stmts = program.fns[0].body.stmts.clone();
        let Stmt::ShortVarDecl { values, .. } = &stmts[0] else { panic!("expected shortVarDecl") };
        let Expr::StructLit { name, fields, .. } = &values[0] else { panic!("expected structLit") };
        assert_eq!(name, "User");
        assert_eq!(fields.iter().map(|f| f.name.as_str()).collect::<Vec<_>>(), vec!["name", "age"]);

        let Stmt::ExprStmt { expr: Expr::Call { args, .. }, .. } = &stmts[1] else { panic!("expected call") };
        assert!(matches!(&args[0], Expr::Member { name, .. } if name == "name"));
    }

    #[test]
    fn union宣言_行頭パイプの複数行フォーマットで書ける() {
        // ベンチ第1ラウンドでMeshが唯一落ちた負転移パターン(bench/tasks/04参照)。TS版のテストを移植
        let program = parse(
            "type Expr = { kind: \"num\", value: int }\n| { kind: \"add\", left: Expr, right: Expr }\n| { kind: \"neg\", operand: Expr }\nfn main() {}",
        )
        .unwrap();
        let TypeNode::Union { members, .. } = &program.types[0].node else { panic!("expected union") };
        assert_eq!(members.len(), 3);

        // 行末パイプスタイル(元々可)も引き続き動く
        let program = parse("type Status = \"active\" |\n\"banned\"\nfn main() {}").unwrap();
        let TypeNode::Union { members, .. } = &program.types[0].node else { panic!("expected union") };
        assert_eq!(members.len(), 2);
    }

    #[test]
    fn 判別可能union_type宣言のunion内に無名構造体型式を書ける() {
        let program = parse(
            "type GetUserResponse = { kind: \"ok\", user: User } | { kind: \"notFound\" }\nfn main() {}",
        )
        .unwrap();
        let decl = program.types.iter().find(|t| t.name == "GetUserResponse").unwrap();
        let TypeNode::Union { members, .. } = &decl.node else { panic!("expected union") };
        assert_eq!(members.len(), 2);
        let TypeNode::StructType { fields, .. } = &members[0] else { panic!("expected structType") };
        assert_eq!(fields.iter().map(|f| f.name.as_str()).collect::<Vec<_>>(), vec!["kind", "user"]);
    }

    #[test]
    fn 判別可能union_単独の裸構造体型式はbare_struct_shapeでエラー() {
        let errors = parse("type Resp = { kind: \"ok\" }\nfn main() {}").unwrap_err();
        assert_eq!(errors[0].code, "bare-struct-shape");
        assert!(errors[0].message.contains("use 'struct Resp { ... }'"));
    }

    #[test]
    fn 判別可能union_matchパターンに部分構造を書ける() {
        let stmts = parse_body("x := match res {\n{ kind: \"ok\" } => 1\n_ => 0\n}");
        let Stmt::ShortVarDecl { values, .. } = &stmts[0] else { panic!("expected shortVarDecl") };
        let Expr::Match { arms, .. } = &values[0] else { panic!("expected match") };
        assert!(matches!(&arms[0].patterns[0], MatchPattern::Type(TypeNode::StructType { .. })));
    }

    #[test]
    fn match式_アーム_複数パターン_ワイルドカード() {
        let stmts = parse_body("x := match r {\n\tnone, error => \"fail\"\n\tint => \"ok\"\n\t_ => \"other\"\n}");
        let Stmt::ShortVarDecl { values, .. } = &stmts[0] else { panic!("expected shortVarDecl") };
        let Expr::Match { arms, .. } = &values[0] else { panic!("expected match") };
        assert_eq!(arms.len(), 3);
        assert_eq!(arms[0].patterns.len(), 2);
        assert!(matches!(arms[2].patterns[0], MatchPattern::Wildcard { .. }));
    }

    #[test]
    fn isは型を右辺に取る() {
        let stmts = parse_body("ok := x is none");
        let Stmt::ShortVarDecl { values, .. } = &stmts[0] else { panic!("expected shortVarDecl") };
        assert!(matches!(values[0], Expr::Is { .. }));
    }

    #[test]
    fn 文字列補間_テキストと式の断片に分かれる() {
        let stmts = parse_body(r#"msg := "worker ${id} done""#);
        let Stmt::ShortVarDecl { values, .. } = &stmts[0] else { panic!("expected shortVarDecl") };
        let Expr::Interp { segments, .. } = &values[0] else { panic!("expected interp, got {:?}", values[0]) };
        assert_eq!(segments.len(), 3);
        assert!(matches!(&segments[0], InterpSegment::Text { text } if text == "worker "));
        assert!(matches!(&segments[1], InterpSegment::Expr { expr } if matches!(**expr, Expr::Ident { .. })));
        assert!(matches!(&segments[2], InterpSegment::Text { text } if text == " done"));
    }

    #[test]
    fn 文字列補間_式部分はメンバーアクセスや呼び出しを含む任意の式() {
        // examples/users.meshの実例相当: ${u.name} や ${res.user.name} のようなメンバーアクセス
        let stmts = parse_body(r#"msg := "hello ${u.name} (${u.age})""#);
        let Stmt::ShortVarDecl { values, .. } = &stmts[0] else { panic!("expected shortVarDecl") };
        let Expr::Interp { segments, .. } = &values[0] else { panic!("expected interp") };
        // ["hello ", ${u.name}, " (", ${u.age}, ")"] の5断片
        assert_eq!(segments.len(), 5);
        assert!(matches!(&segments[1], InterpSegment::Expr { expr } if matches!(**expr, Expr::Member { .. })));
        assert!(matches!(&segments[3], InterpSegment::Expr { expr } if matches!(**expr, Expr::Member { .. })));
    }

    #[test]
    fn 文字列補間_式部分の位置情報は元ソース上のもの() {
        // 再字句解析した式のposが「断片の中の0番目」ではなく「元の文字列全体の中の位置」に
        // なっていること(lexer.rsのstartPos伝播がparser側でも保たれているかの確認)
        let stmts = parse_body(r#"msg := "x${id}""#);
        let Stmt::ShortVarDecl { values, .. } = &stmts[0] else { panic!("expected shortVarDecl") };
        let Expr::Interp { segments, .. } = &values[0] else { panic!("expected interp") };
        let InterpSegment::Expr { expr } = &segments[1] else { panic!("expected expr segment") };
        // parse_body は "fn main() {\n" の後にsrcを置くので2行目、"msg := \"x${" の直後 = 12列目
        assert_eq!(expr.pos(), Pos { line: 2, col: 12 });
    }

    #[test]
    fn 文字列補間_式部分の構文エラーは呼び出し元に伝播する() {
        // "${1 +}" は二項演算子の右辺が無いまま式が終わる — 再帰的に呼んだ
        // parse_standalone_expr内部のエラー(TSのthrow相当)がそのまま外側に伝わることを確認する
        let err = parse(r#"fn main() { msg := "${1 +}" }"#).unwrap_err();
        assert_eq!(err.len(), 1);
        assert!(err[0].message.contains("end of file"), "got: {}", err[0].message);
    }

    #[test]
    fn 文字列補間_式部分に2つ以上の式は書けない() {
        // "${1 2}" のように式の後にトークンが余る場合はparse_standalone_expr内でエラーになる
        let err = parse(r#"fn main() { msg := "${1 2}" }"#).unwrap_err();
        assert!(err[0].message.contains("unexpected '2' in string interpolation"), "got: {}", err[0].message);
    }

    #[test]
    fn 文字列補間_式部分に補間文字列自体が来ても空のエラーにならない() {
        // "${1 "${2}"}" — 式の後に余るトークンが「補間つき文字列」自身だと、その
        // トークンのvalueは空文字列(lexer.rs参照)のため、そのまま引用符で囲むと
        // `unexpected ''`という中身の分からないエラーになっていた(code reviewでの指摘)。
        // describe_tokenが種別名にフォールバックし、'string'と表示されることを確認する
        let err = parse(r#"fn main() { msg := "${1 "${2}"}" }"#).unwrap_err();
        assert!(err[0].message.contains("unexpected 'string' in string interpolation"), "got: {}", err[0].message);
    }

    // "${...}"の中に同じ形をもう1つ入れ子にして深さdepth段のネスト補間ソースを作る。
    // 各段は`"${` + 前段 + `}"`を足すだけ(前段の中身をエスケープしない)ので長さは
    // 線形にしか伸びない — クォートやバックスラッシュの中身をエスケープする方式だと
    // 段ごとに長さが倍増し、200段程度で現実的なメモリを使い切る(実際に検証で踏んだ)
    fn nested_interp_source(depth: usize) -> String {
        let mut s = "1".to_string();
        for _ in 0..depth {
            s = format!("\"${{{s}}}\"");
        }
        s
    }

    #[test]
    fn 文字列補間_上限未満のネストは正しくパースできる() {
        let src = format!("fn main() {{ msg := {} }}", nested_interp_source(MAX_INTERP_DEPTH - 1));
        parse(&src).unwrap();
    }

    #[test]
    fn 文字列補間_上限を超えるネストはクラッシュせず構文エラーになる() {
        // 実際にスタックオーバーフローでプロセスごと落ちていたのを直した回帰テスト
        // (code reviewでの指摘・実機での再現を踏まえてMAX_INTERP_DEPTHのガードを追加した)
        let src = format!("fn main() {{ msg := {} }}", nested_interp_source(MAX_INTERP_DEPTH + 10));
        let err = parse(&src).unwrap_err();
        assert_eq!(err[0].code, "interpolation-too-deep");
        assert!(err[0].message.contains("too deeply"), "got: {}", err[0].message);
    }

    #[test]
    fn 実例相当_struct_is_match_を組み合わせた一連の流れ() {
        // examples/discriminated_union.meshの簡略版(文字列補間も含む)
        let program = parse(
            "struct User {\n\tname: string\n}\n\
             type GetUserResponse = { kind: \"ok\", user: User } | { kind: \"notFound\" }\n\
             fn findUser(id: string) User | none {\n\tif id == \"1\" {\n\t\treturn User{name: \"alice\"}\n\t}\n\treturn none\n}\n\
             fn getUser(id: string) GetUserResponse {\n\tu := findUser(id)\n\tif u is none {\n\t\treturn GetUserResponse{kind: \"notFound\"}\n\t}\n\treturn GetUserResponse{kind: \"ok\", user: u}\n}\n\
             fn describe(res: GetUserResponse) string {\n\treturn match res {\n\t\t{ kind: \"ok\" } => \"found: ${res.user.name}\"\n\t\t{ kind: \"notFound\" } => \"not found\"\n\t}\n}\n\
             fn main() {\n\tprint(describe(getUser(\"1\")))\n}",
        )
        .unwrap();
        assert_eq!(program.types.len(), 2);
        assert_eq!(program.fns.iter().map(|f| f.name.as_str()).collect::<Vec<_>>(), vec!["findUser", "getUser", "describe", "main"]);
    }

    // ---- 並行処理(spawn/detach/wait/chan/select/send/recv) ----

    #[test]
    fn chan型を関数シグネチャで使える() {
        let program = parse("fn worker(id: int, ch: chan<string>) {\n\tch <- \"done\"\n}").unwrap();
        let param = &program.fns[0].params[1];
        let TypeNode::Chan { elem, .. } = &param.type_node else { panic!("expected chan type, got {:?}", param.type_node) };
        assert!(matches!(&**elem, TypeNode::Name { name, .. } if name == "string"));
    }

    #[test]
    fn spawnはチャネル送受信_呼び出しと組み合わせられる() {
        let stmts = parse_body("ch := chan<int>(none)\nspawn f(1, ch)\nx := <-ch\nch <- 2");
        assert!(matches!(stmts[0], Stmt::ShortVarDecl { .. }));
        assert!(matches!(stmts[1], Stmt::ExprStmt { .. }));
        assert!(matches!(stmts[2], Stmt::ShortVarDecl { .. }));
        assert!(matches!(stmts[3], Stmt::Send { .. }));
    }

    #[test]
    fn spawnは式として受取口を返せる() {
        let stmts = parse_body("task := spawn f(1)");
        let Stmt::ShortVarDecl { values, .. } = &stmts[0] else { panic!("expected shortVarDecl") };
        assert!(matches!(values[0], Expr::Spawn { detached: false, .. }));
    }

    #[test]
    fn spawnの後は関数呼び出しのみ() {
        let err = parse("fn main() { spawn 1 + 2 }").unwrap_err();
        assert_eq!(err[0].code, "invalid-spawn-target");
    }

    #[test]
    fn detachはspawnと同形でdetachedフラグが立つ() {
        let stmts = parse_body("task := detach f(1)");
        let Stmt::ShortVarDecl { values, .. } = &stmts[0] else { panic!("expected shortVarDecl") };
        assert!(matches!(values[0], Expr::Spawn { detached: true, .. }));

        let err = parse("fn main() { detach 1 + 2 }").unwrap_err();
        assert_eq!(err[0].code, "invalid-spawn-target");
    }

    #[test]
    fn waitブロックをパースできる() {
        let stmts = parse_body("wait {\nspawn f(1)\n}");
        assert!(matches!(stmts[0], Stmt::Wait { .. }));
    }

    #[test]
    fn chan_t_nは容量式を持つ() {
        let stmts = parse_body("ch := chan<int>(3)");
        let Stmt::ShortVarDecl { values, .. } = &stmts[0] else { panic!("expected shortVarDecl") };
        let Expr::Chan { elem, capacity, .. } = &values[0] else { panic!("expected chan expr, got {:?}", values[0]) };
        assert!(matches!(elem, TypeNode::Name { name, .. } if name == "int"));
        assert!(matches!(&**capacity, Expr::Int { value, .. } if value == "3"));
    }

    #[test]
    fn f11_chan_t_noneは明示的な無制限バッファ() {
        let stmts = parse_body("ch := chan<int>(none)");
        let Stmt::ShortVarDecl { values, .. } = &stmts[0] else { panic!("expected shortVarDecl") };
        let Expr::Chan { capacity, .. } = &values[0] else { panic!("expected chan expr, got {:?}", values[0]) };
        assert!(matches!(&**capacity, Expr::None { .. }));
    }

    #[test]
    fn f11_chan_tは容量の省略を許さない() {
        let err = parse("fn main() { ch := chan<int>() }").unwrap_err();
        assert_eq!(err[0].code, "chan-capacity-required");
        assert!(err[0].message.contains("no longer defaults to an unbounded buffer"), "got: {}", err[0].message);
    }

    #[test]
    fn select式_アームとdefaultをパースできる() {
        let stmts = parse_body("msg := select {\nv := <-a => v\n_ => \"none\"\n}");
        let Stmt::ShortVarDecl { values, .. } = &stmts[0] else { panic!("expected shortVarDecl") };
        let Expr::Select { arms, default_arm, .. } = &values[0] else { panic!("expected select expr, got {:?}", values[0]) };
        assert_eq!(arms.len(), 1);
        assert_eq!(arms[0].name, "v");
        assert!(default_arm.is_some());
    }

    #[test]
    fn select式_defaultは1つまで() {
        let err = parse("fn main() { msg := select {\n_ => \"a\"\n_ => \"b\"\n} }").unwrap_err();
        assert_eq!(err[0].code, "multiple-select-defaults");
    }

    #[test]
    fn 実例相当_channels_meshの簡略版() {
        // examples/channels.meshの簡略版: spawnで並行起動しchannelで結果を受け取る
        let program = parse(
            "fn worker(id: int, ch: chan<string>) {\n\tch <- \"worker ${id} done\"\n}\n\
             fn main() {\n\tch := chan<string>(none)\n\tspawn worker(1, ch)\n\tmsg := <-ch\n\tprint(msg)\n}",
        )
        .unwrap();
        assert_eq!(program.fns.iter().map(|f| f.name.as_str()).collect::<Vec<_>>(), vec!["worker", "main"]);
    }

    // ---- error/json構造化エラー(`?`伝播・`or`束縛形) ----

    #[test]
    fn 後置のとorをパースできる_文脈つき_束縛形も() {
        let stmts = parse_body("x := f()? or _ => 0");
        let Stmt::ShortVarDecl { values, .. } = &stmts[0] else { panic!("expected shortVarDecl") };
        let Expr::OrElse { left, binding, .. } = &values[0] else { panic!("expected orElse expr, got {:?}", values[0]) };
        assert_eq!(binding.as_deref(), Some("_"));
        assert!(matches!(&**left, Expr::Prop { .. }));

        // 文脈つき伝播: f() ? "ctx"
        let stmts2 = parse_body("x := f() ? \"line ${i}: bad\"");
        let Stmt::ShortVarDecl { values, .. } = &stmts2[0] else { panic!("expected shortVarDecl") };
        let Expr::Prop { context, .. } = &values[0] else { panic!("expected prop expr, got {:?}", values[0]) };
        assert!(context.is_some());

        // 束縛形: or e => 式
        let stmts3 = parse_body("x := f() or e => g(e)");
        let Stmt::ShortVarDecl { values, .. } = &stmts3[0] else { panic!("expected shortVarDecl") };
        let Expr::OrElse { binding, .. } = &values[0] else { panic!("expected orElse expr, got {:?}", values[0]) };
        assert_eq!(binding.as_deref(), Some("e"));
    }

    #[test]
    fn orは束縛無しでも書ける() {
        let stmts = parse_body("x := f() or 0");
        let Stmt::ShortVarDecl { values, .. } = &stmts[0] else { panic!("expected shortVarDecl") };
        let Expr::OrElse { binding, .. } = &values[0] else { panic!("expected orElse expr, got {:?}", values[0]) };
        assert!(binding.is_none());
    }

    #[test]
    fn orは最も弱く結合する() {
        // f() or 0 + 1 は f() or (0 + 1) ではなく (f() or 0) + 1 ...ではなく、
        // TS版と同じくorが最弱結合なので right側は `parseBinary(prec + 1)` で
        // 0 + 1 全体を1つの右辺として読む(orは他の全演算子より弱いため左辺には来ない)
        let stmts = parse_body("x := 1 + 2 or 0");
        let Stmt::ShortVarDecl { values, .. } = &stmts[0] else { panic!("expected shortVarDecl") };
        let Expr::OrElse { left, .. } = &values[0] else { panic!("expected orElse expr, got {:?}", values[0]) };
        assert!(matches!(&**left, Expr::Binary { .. }));
    }

    #[test]
    fn 実例相当_errors_meshの簡略版() {
        let program = parse(
            "fn divide(a: int, b: int) int | error {\n\tif b == 0 {\n\t\treturn error(\"division by zero\")\n\t}\n\treturn a / b\n}\n\
             fn main() {\n\tresult := divide(10, 3)\n\tif result is error {\n\t\tprint(\"error:\", result)\n\t\treturn\n\t}\n\
             \tfallback := divide(1, 0) or _ => 0\n\tprint(\"fallback: ${fallback}\")\n\
             \tlogged := divide(1, 0) or e => len(\"${e}\")\n\tprint(\"logged: ${logged}\")\n\
             \tr := divide(9, 3)\n\tprint(match r {\n\t\terror => \"failed: ${r}\"\n\t\tint => \"match says: ${r}\"\n\t})\n}",
        )
        .unwrap();
        assert_eq!(program.fns.iter().map(|f| f.name.as_str()).collect::<Vec<_>>(), vec!["divide", "main"]);
    }

    // ---- モジュール(import/export)・型注釈つき変数宣言 ----

    #[test]
    fn モジュール_import宣言とexport修飾をパースできる() {
        let program = parse(
            "import \"mathutil\"\n\n\
             export fn add(a: int, b: int) int { return a + b }\n\
             fn helper() int { return 1 }\n\
             export struct Point { x: int }\n\
             export type Status = \"on\" | \"off\"\n\
             fn main() {}",
        )
        .unwrap();
        assert_eq!(program.imports.len(), 1);
        assert_eq!(program.imports[0].path, "mathutil");
        assert_eq!(program.imports[0].alias, "mathutil");
        assert!(program.fns.iter().find(|f| f.name == "add").unwrap().exported);
        assert!(!program.fns.iter().find(|f| f.name == "helper").unwrap().exported);
        assert!(program.types.iter().find(|t| t.name == "Point").unwrap().exported);
        assert!(program.types.iter().find(|t| t.name == "Status").unwrap().exported);
    }

    #[test]
    fn モジュール_importは宣言より前に置く必要がある() {
        let err = parse("fn main() {}\nimport \"x\"").unwrap_err();
        assert_eq!(err[0].code, "import-order");
    }

    #[test]
    fn モジュール_importパスは補間も空文字列も使えない() {
        let err = parse("import \"foo${1}\"\nfn main() {}").unwrap_err();
        assert_eq!(err[0].code, "invalid-import-path");

        let err2 = parse("import \"\"\nfn main() {}").unwrap_err();
        assert_eq!(err2[0].code, "invalid-import-path");
    }

    #[test]
    fn モジュール_修飾型名math_userと修飾structリテラルmath_pointをパースできる() {
        let program = parse("fn f(u: math.User) {}").unwrap();
        let TypeNode::Name { name, pkg, .. } = &program.fns[0].params[0].type_node else { panic!("expected name type") };
        assert_eq!(name, "User");
        assert_eq!(pkg.as_deref(), Some("math"));

        let stmts = parse_body("p := math.Point{x: 1, y: 2}");
        let Stmt::ShortVarDecl { values, .. } = &stmts[0] else { panic!("expected shortVarDecl") };
        let Expr::StructLit { name, pkg, .. } = &values[0] else { panic!("expected structLit, got {:?}", values[0]) };
        assert_eq!(name, "Point");
        assert_eq!(pkg.as_deref(), Some("math"));
    }

    #[test]
    fn 型注釈つき変数宣言をパースできる() {
        let stmts = parse_body("q: mathutil.Point = mathutil.origin()");
        let Stmt::TypedVarDecl { name, type_node, mutable, .. } = &stmts[0] else { panic!("expected typedVarDecl, got {:?}", stmts[0]) };
        assert_eq!(name, "q");
        assert!(!mutable);
        assert!(matches!(type_node, TypeNode::Name { pkg: Some(p), .. } if p == "mathutil"));

        let stmts2 = parse_body("mut best: string | none = none");
        let Stmt::TypedVarDecl { mutable, type_node, .. } = &stmts2[0] else { panic!("expected typedVarDecl, got {:?}", stmts2[0]) };
        assert!(*mutable);
        assert!(matches!(type_node, TypeNode::Union { .. }));
    }

    #[test]
    fn 実例相当_modules_demo_meshの簡略版() {
        let program = parse(
            "import \"mathutil\"\n\n\
             fn main() {\n\tprint(mathutil.add(1, 2))\n\
             \tp := mathutil.Point{x: 3, y: 4}\n\tprint(p.magnitudeSq())\n\
             \tq: mathutil.Point = mathutil.origin()\n\tprint(q.x, q.y)\n}",
        )
        .unwrap();
        assert_eq!(program.imports[0].alias, "mathutil");
        assert_eq!(program.fns.iter().map(|f| f.name.as_str()).collect::<Vec<_>>(), vec!["main"]);
    }
}
