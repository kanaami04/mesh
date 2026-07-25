// `mesh fmt`: ASTを歩いて正規形のソースへ印字し直す(gofmt相当・設定オプションなし)。
// TS版`src/formatter.ts`(486行)の移植。
//
// 方針(2026-07-20にkanayamaと討議して決めたもの。TS版の冒頭コメントと同じ):
// - インデントはタブ固定
// - 改行の有無は「幅に応じた自動折り返し」をしない(gofmt方式)。struct/array/mapリテラル・
//   関数呼び出しの引数・union宣言は、元のソースで複数行だったかをparserが`multiline`フラグに
//   記録済みで、印字はそれをそのまま尊重するだけ(このファイルでは判断しない)
// - コメントはlexerが別配列(`Program.comments`)へ退避済み。印字時に「直前の要素と同じ行にある
//   コメント→trailing」「そうでなければ次の要素の直前へ→leading」という位置ベースの単純な規則で
//   再割り当てする。コメントを絶対に消さない(見つけたコメントは必ずどこかに出す)ことを優先し、
//   複数行にまたがる式の途中に挟まったコメントの位置までは厳密に再現しない(既知の限界)
//
// 既知の限界(TS版と同じ):
// - 空行の保持はしない——トップレベル宣言間・import後には常に1行、文の間は常に0行という
//   固定ルールで正規化する
// - struct/array/mapリテラルの途中、関数呼び出しの引数の途中にあるコメントは、直後の文の
//   leadingコメントとして出る(位置がずれるだけで消えはしない)

use crate::ast::{
    Block, ElseClause, Expr, FnDecl, IfStmt, InterpSegment, MatchPattern, Program, Stmt, StructFieldNode, TypeDecl, TypeNode,
};
use crate::parser::parse;
use crate::token::{CommentInfo, CompileError};

const INDENT: &str = "\t";

fn indent_of(level: usize) -> String {
    INDENT.repeat(level)
}

pub fn format(source: &str) -> Result<String, Vec<CompileError>> {
    let program = parse(source)?;
    Ok(print_program(&program))
}

struct Printer {
    lines: Vec<String>,
    comments: Vec<CommentInfo>,
    idx: usize,
}

impl Printer {
    fn new(comments: &[CommentInfo]) -> Self {
        let mut comments = comments.to_vec();
        comments.sort_by_key(|c| (c.pos.line, c.pos.col));
        Printer { lines: Vec::new(), comments, idx: 0 }
    }

    // before_line より前にある未消費コメントを、それぞれ単独行のleadingコメントとして出す
    fn flush_leading_comments(&mut self, before_line: usize, indent: usize) {
        while self.idx < self.comments.len() && self.comments[self.idx].pos.line < before_line {
            self.lines.push(indent_of(indent) + &self.comments[self.idx].text);
            self.idx += 1;
        }
    }

    // line と同じ行にある未消費コメントが1つあれば消費して返す(trailing用)
    fn take_trailing_comment(&mut self, line: usize) -> Option<String> {
        if self.idx < self.comments.len() && self.comments[self.idx].pos.line == line {
            let text = self.comments[self.idx].text.clone();
            self.idx += 1;
            return Some(text);
        }
        None
    }

    // 残りのコメントを全部出す(どのノードにも紐づかない末尾コメント)
    fn flush_remaining(&mut self, indent: usize) {
        while self.idx < self.comments.len() {
            self.lines.push(indent_of(indent) + &self.comments[self.idx].text);
            self.idx += 1;
        }
    }

    fn emit(&mut self, text: impl Into<String>) {
        self.lines.push(text.into());
    }

    fn blank(&mut self) {
        self.lines.push(String::new());
    }

    fn result(mut self) -> String {
        // 末尾に空行を溜めない
        while self.lines.last().map(String::is_empty).unwrap_or(false) {
            self.lines.pop();
        }
        self.lines.join("\n") + "\n"
    }
}

// trailingコメントを本文の後ろへ足す(TS版の `+ (trailing ? "  " + trailing : "")`)
fn with_trailing(text: String, trailing: Option<String>) -> String {
    match trailing {
        Some(c) => format!("{text}  {c}"),
        None => text,
    }
}

fn print_program(program: &Program) -> String {
    let mut p = Printer::new(&program.comments);

    for imp in &program.imports {
        p.flush_leading_comments(imp.pos.line, 0);
        let trailing = p.take_trailing_comment(imp.pos.line);
        p.emit(with_trailing(format!("import \"{}\"", imp.path), trailing));
    }

    // トップレベル項目は元ソースの行順に並べ直す(型/関数/定数が混在していても順序を保つ)
    enum Item<'a> {
        Type(&'a TypeDecl),
        Fn(&'a FnDecl),
        Const(&'a crate::ast::ConstDecl),
    }
    let mut items: Vec<(usize, Item)> = Vec::new();
    items.extend(program.types.iter().map(|t| (t.pos.line, Item::Type(t))));
    items.extend(program.fns.iter().map(|f| (f.pos.line, Item::Fn(f))));
    items.extend(program.consts.iter().map(|c| (c.pos.line, Item::Const(c))));
    items.sort_by_key(|(line, _)| *line);

    if !program.imports.is_empty() && !items.is_empty() {
        p.blank();
    }

    for (i, (line, item)) in items.iter().enumerate() {
        if i > 0 {
            p.blank();
        }
        p.flush_leading_comments(*line, 0);
        match item {
            Item::Type(t) => print_type_decl(&mut p, t),
            Item::Fn(f) => print_fn_decl(&mut p, f),
            Item::Const(c) => {
                let kw = if c.exported { "export " } else { "" };
                let type_ann = c.type_node.as_ref().map(|t| format!(": {}", print_type_node(t, 0))).unwrap_or_default();
                let op = if c.type_node.is_some() { "=" } else { ":=" };
                let trailing = p.take_trailing_comment(c.pos.line);
                p.emit(with_trailing(format!("{kw}{}{type_ann} {op} {}", c.name, print_expr(&c.value, 0)), trailing));
            }
        }
    }

    p.flush_remaining(0);
    p.result()
}

fn print_type_decl(p: &mut Printer, decl: &TypeDecl) {
    let kw = if decl.exported { "export " } else { "" };
    let err_kw = if decl.is_error { "error " } else { "" };
    let json_kw = if decl.is_json { "json " } else { "" };
    if let TypeNode::StructType { fields, .. } = &decl.node {
        p.emit(format!("{kw}{err_kw}{json_kw}struct {} {{", decl.name));
        for f in fields {
            p.flush_leading_comments(f.pos.line, 1);
            let trailing = p.take_trailing_comment(f.pos.line);
            p.emit(with_trailing(format!("{}{}: {}", indent_of(1), f.name, print_type_node(&f.type_node, 1)), trailing));
        }
        p.emit("}");
        return;
    }
    let trailing = p.take_trailing_comment(decl.pos.line);
    let rhs = print_type_node(&decl.node, 0);
    p.emit(with_trailing(format!("{kw}{err_kw}type {} = {rhs}", decl.name), trailing));
}

fn print_fn_decl(p: &mut Printer, f: &FnDecl) {
    let kw = if f.exported { "export " } else { "" };
    let recv = f
        .receiver
        .as_ref()
        .map(|r| format!("({}: {}) ", r.name, print_type_node(&r.type_node, 0)))
        .unwrap_or_default();
    let type_params = if f.type_params.is_empty() { String::new() } else { format!("<{}>", f.type_params.join(", ")) };
    let params: Vec<String> = f.params.iter().map(|param| format!("{}: {}", param.name, print_type_node(&param.type_node, 0))).collect();
    let ret = f.ret.as_ref().map(|r| format!(" {}", print_type_node(r, 0))).unwrap_or_default();
    p.emit(format!("{kw}fn {recv}{}{type_params}({}){ret} {{", f.name, params.join(", ")));
    print_block_stmts(p, &f.body, 1);
    p.emit("}");
}

fn print_block_stmts(p: &mut Printer, block: &Block, indent: usize) {
    for stmt in &block.stmts {
        p.flush_leading_comments(stmt_pos(stmt).line, indent);
        print_stmt(p, stmt, indent);
    }
}

fn stmt_pos(stmt: &Stmt) -> crate::token::Pos {
    match stmt {
        Stmt::ShortVarDecl { pos, .. }
        | Stmt::TypedVarDecl { pos, .. }
        | Stmt::Assign { pos, .. }
        | Stmt::ExprStmt { pos, .. }
        | Stmt::Return { pos, .. }
        | Stmt::For { pos, .. }
        | Stmt::IncDec { pos, .. }
        | Stmt::Break { pos }
        | Stmt::Continue { pos }
        | Stmt::Wait { pos, .. }
        | Stmt::Send { pos, .. }
        | Stmt::RangeFor { pos, .. }
        | Stmt::DeferStmt { pos, .. } => *pos,
        Stmt::If(if_stmt) => if_stmt.pos,
    }
}

fn print_stmt(p: &mut Printer, stmt: &Stmt, indent: usize) {
    let ind = indent_of(indent);
    // ブロックを持たない文は`try_print_stmt_inline`と同じ本文を使う(TS版のemitSimple)
    if let Some(text) = try_print_stmt_inline(stmt, indent) {
        let trailing = p.take_trailing_comment(stmt_pos(stmt).line);
        p.emit(with_trailing(format!("{ind}{text}"), trailing));
        return;
    }
    match stmt {
        Stmt::If(if_stmt) => print_if(p, if_stmt, indent),
        Stmt::For { init, cond, post, body, pos } => {
            let init_s = init.as_deref().and_then(|s| try_print_stmt_inline(s, indent)).unwrap_or_default();
            let cond_s = cond.as_ref().map(|c| print_expr(c, indent)).unwrap_or_default();
            let post_s = post.as_deref().and_then(|s| try_print_stmt_inline(s, indent)).unwrap_or_default();
            let header = if init.is_some() || post.is_some() {
                format!("for {init_s}; {cond_s}; {post_s}")
            } else if cond.is_some() {
                format!("for {cond_s}")
            } else {
                "for".to_string()
            };
            let trailing = p.take_trailing_comment(pos.line);
            p.emit(with_trailing(format!("{ind}{header} {{"), trailing));
            print_block_stmts(p, body, indent + 1);
            p.emit(format!("{ind}}}"));
        }
        Stmt::RangeFor { names, subject, body, pos } => {
            let trailing = p.take_trailing_comment(pos.line);
            p.emit(with_trailing(format!("{ind}for {} := range {} {{", names.join(", "), print_expr(subject, indent)), trailing));
            print_block_stmts(p, body, indent + 1);
            p.emit(format!("{ind}}}"));
        }
        Stmt::Wait { body, pos } => {
            let trailing = p.take_trailing_comment(pos.line);
            p.emit(with_trailing(format!("{ind}wait {{"), trailing));
            print_block_stmts(p, body, indent + 1);
            p.emit(format!("{ind}}}"));
        }
        // try_print_stmt_inlineがNoneを返すのは上の3種+ifだけ
        _ => unreachable!("inline印字できない文はif/for/rangeFor/waitのみ"),
    }
}

// 1行に収められる単純文だけを文字列化する(コメント付与・インデント無し)。if/for/wait/
// rangeForのようなブロックを持つ文はNoneを返す(fnExprの1行書き判定、for文ヘッダ、
// 通常の文の印字で共有する)
fn try_print_stmt_inline(stmt: &Stmt, indent: usize) -> Option<String> {
    let text = match stmt {
        Stmt::ShortVarDecl { names, values, mutable, .. } => {
            let kw = if *mutable { "mut " } else { "" };
            let vals: Vec<String> = values.iter().map(|v| print_expr(v, indent)).collect();
            format!("{kw}{} := {}", names.join(", "), vals.join(", "))
        }
        Stmt::TypedVarDecl { name, type_node, value, mutable, .. } => {
            let kw = if *mutable { "mut " } else { "" };
            format!("{kw}{name}: {} = {}", print_type_node(type_node, indent), print_expr(value, indent))
        }
        Stmt::Assign { targets, values, compound_op, .. } => {
            let op = compound_op.as_ref().map(|o| format!("{o}=")).unwrap_or_else(|| "=".to_string());
            let tgts: Vec<String> = targets.iter().map(|t| print_expr(t, indent)).collect();
            let vals: Vec<String> = values.iter().map(|v| print_expr(v, indent)).collect();
            format!("{} {op} {}", tgts.join(", "), vals.join(", "))
        }
        Stmt::IncDec { target, op, .. } => format!("{}{op}", print_expr(target, indent)),
        Stmt::ExprStmt { expr, .. } => print_expr(expr, indent),
        Stmt::Return { value, .. } => match value {
            None => "return".to_string(),
            Some(v) => format!("return {}", print_expr(v, indent)),
        },
        Stmt::Send { channel, value, .. } => format!("{} <- {}", print_expr(channel, indent), print_expr(value, indent)),
        Stmt::Break { .. } => "break".to_string(),
        Stmt::Continue { .. } => "continue".to_string(),
        Stmt::DeferStmt { call, .. } => format!("defer {}", print_expr(call, indent)),
        // if/for/wait/rangeFor: ブロックを持つので1行化しない
        _ => return None,
    };
    Some(text)
}

fn print_if(p: &mut Printer, stmt: &IfStmt, indent: usize) {
    let ind = indent_of(indent);
    let trailing = p.take_trailing_comment(stmt.pos.line);
    p.emit(with_trailing(format!("{ind}if {} {{", print_expr(&stmt.cond, indent)), trailing));
    print_block_stmts(p, &stmt.then, indent + 1);
    print_else_chain(p, stmt.else_.as_deref(), indent);
}

// "} else if ... {" / "} else {" は閉じ括弧と同じ行にまとめる(独立したif文としてではなく、
// 1つのif/else連鎖として印字する)。再帰でどれだけ else if が連なっても対応する
fn print_else_chain(p: &mut Printer, else_: Option<&ElseClause>, indent: usize) {
    let ind = indent_of(indent);
    match else_ {
        None => p.emit(format!("{ind}}}")),
        Some(ElseClause::If(nested)) => {
            let trailing = p.take_trailing_comment(nested.pos.line);
            p.emit(with_trailing(format!("{ind}}} else if {} {{", print_expr(&nested.cond, indent)), trailing));
            print_block_stmts(p, &nested.then, indent + 1);
            print_else_chain(p, nested.else_.as_deref(), indent);
        }
        Some(ElseClause::Block(block)) => {
            p.emit(format!("{ind}}} else {{"));
            print_block_stmts(p, block, indent + 1);
            p.emit(format!("{ind}}}"));
        }
    }
}

fn print_type_node(t: &TypeNode, indent: usize) -> String {
    match t {
        TypeNode::Name { name, pkg, .. } => match pkg {
            Some(pkg) => format!("{pkg}.{name}"),
            None => name.clone(),
        },
        TypeNode::Literal { value, .. } => format!("\"{}\"", escape_string(value)),
        TypeNode::Array { elem, .. } => format!("{}[]", print_type_node(elem, indent)),
        TypeNode::Chan { elem, .. } => format!("chan<{}>", print_type_node(elem, indent)),
        TypeNode::MapType { key, value, .. } => format!("map<{}, {}>", print_type_node(key, indent), print_type_node(value, indent)),
        TypeNode::Union { members, multiline, .. } => {
            if *multiline && members.len() > 1 {
                let cont = indent_of(indent + 1);
                return members
                    .iter()
                    .enumerate()
                    .map(|(i, m)| if i == 0 { print_type_node(m, indent) } else { format!("\n{cont}| {}", print_type_node(m, indent)) })
                    .collect::<Vec<_>>()
                    .join("");
            }
            members.iter().map(|m| print_type_node(m, indent)).collect::<Vec<_>>().join(" | ")
        }
        TypeNode::StructType { fields, .. } => print_inline_struct_fields(fields, indent),
        TypeNode::FnType { params, ret, .. } => {
            let ps: Vec<String> = params.iter().map(|p| print_type_node(p, indent)).collect();
            let r = ret.as_ref().map(|r| format!(" {}", print_type_node(r, indent))).unwrap_or_default();
            format!("fn({}){r}", ps.join(", "))
        }
    }
}

fn print_inline_struct_fields(fields: &[StructFieldNode], indent: usize) -> String {
    let body: Vec<String> = fields.iter().map(|f| format!("{}: {}", f.name, print_type_node(&f.type_node, indent))).collect();
    format!("{{ {} }}", body.join(", "))
}

fn print_expr(expr: &Expr, indent: usize) -> String {
    match expr {
        Expr::Int { value, .. } | Expr::Float { value, .. } => value.clone(),
        Expr::String { value, .. } => format!("\"{}\"", escape_string(value)),
        Expr::Interp { segments, .. } => {
            let body: String = segments
                .iter()
                .map(|s| match s {
                    InterpSegment::Text { text } => escape_string(text),
                    InterpSegment::Expr { expr } => format!("${{{}}}", print_expr(expr, indent)),
                })
                .collect();
            format!("\"{body}\"")
        }
        Expr::Bool { value, .. } => value.to_string(),
        Expr::None { .. } => "none".to_string(),
        Expr::Ident { name, .. } => name.clone(),
        Expr::ArrayLit { elems, elem_type, multiline, .. } => {
            let prefix = elem_type.as_ref().map(|t| format!("{}[]", print_type_node(t, indent))).unwrap_or_default();
            let items: Vec<String> = elems.iter().map(|e| print_expr(e, indent + 1)).collect();
            format!("{prefix}{}", print_braced_list(&items, "[", "]", *multiline, indent, false))
        }
        Expr::Binary { op, left, right, .. } => format!("{} {op} {}", print_expr(left, indent), print_expr(right, indent)),
        Expr::Unary { op, operand, .. } => format!("{op}{}", print_expr(operand, indent)),
        Expr::Recv { channel, .. } => format!("<-{}", print_expr(channel, indent)),
        Expr::Call { callee, args, multiline, .. } => {
            let items: Vec<String> = args.iter().map(|a| print_expr(a, indent + 1)).collect();
            format!("{}{}", print_expr(callee, indent), print_braced_list(&items, "(", ")", *multiline, indent, true))
        }
        Expr::Index { target, index, .. } => format!("{}[{}]", print_expr(target, indent), print_expr(index, indent)),
        Expr::Member { target, name, .. } => format!("{}.{name}", print_expr(target, indent)),
        Expr::FnExpr { params, ret, body, .. } => {
            let ps: Vec<String> = params.iter().map(|param| format!("{}: {}", param.name, print_type_node(&param.type_node, indent))).collect();
            let r = ret.as_ref().map(|r| format!(" {}", print_type_node(r, indent))).unwrap_or_default();
            // インラインクロージャは1行書きが慣習的に多い(`fn(n: int) bool { return n > 3 }`)。
            // 元が1行で、かつ中身が単純文だけならその形を尊重する。複雑な制御構文が混ざる、
            // または元から複数行だったなら、通常のブロックと同じ複数行印字にフォールバック
            if !body.multiline {
                let inline: Option<Vec<String>> = body.stmts.iter().map(|s| try_print_stmt_inline(s, indent)).collect();
                if let Some(parts) = inline {
                    let inner = if parts.is_empty() { String::new() } else { format!(" {} ", parts.join("; ")) };
                    return format!("fn({}){r} {{{inner}}}", ps.join(", "));
                }
            }
            let mut printer = Printer::new(&[]);
            print_block_stmts(&mut printer, body, indent + 1);
            let inner = printer.result().trim_end_matches('\n').to_string();
            format!("fn({}){r} {{\n{inner}\n{}}}", ps.join(", "), indent_of(indent))
        }
        Expr::Chan { elem, capacity, .. } => format!("chan<{}>({})", print_type_node(elem, indent), print_expr(capacity, indent)),
        Expr::Is { operand, target, .. } => format!("{} is {}", print_expr(operand, indent), print_type_node(target, indent)),
        Expr::Prop { operand, context, .. } => {
            let ctx = context.as_ref().map(|c| format!(" {}", print_expr(c, indent))).unwrap_or_default();
            format!("{}?{ctx}", print_expr(operand, indent))
        }
        Expr::OrElse { left, right, binding, .. } => {
            let rhs = match binding {
                Some(name) => format!("{name} => {}", print_expr(right, indent)),
                None => print_expr(right, indent),
            };
            format!("{} or {rhs}", print_expr(left, indent))
        }
        Expr::Match { subject, arms, .. } => {
            let mut printer = Printer::new(&[]);
            for arm in arms {
                let patterns: Vec<String> = arm
                    .patterns
                    .iter()
                    .map(|pat| match pat {
                        MatchPattern::Wildcard { .. } => "_".to_string(),
                        MatchPattern::Type(t) => print_type_node(t, indent + 1),
                    })
                    .collect();
                printer.emit(format!("{}{} => {}", indent_of(indent + 1), patterns.join(", "), print_expr(&arm.body, indent + 1)));
            }
            let body = printer.result().trim_end_matches('\n').to_string();
            format!("match {} {{\n{body}\n{}}}", print_expr(subject, indent), indent_of(indent))
        }
        Expr::StructLit { name, pkg, fields, multiline, .. } => {
            let full = match pkg {
                Some(pkg) => format!("{pkg}.{name}"),
                None => name.clone(),
            };
            let items: Vec<String> = fields.iter().map(|f| format!("{}: {}", f.name, print_expr(&f.value, indent + 1))).collect();
            format!("{full}{}", print_braced_list(&items, "{", "}", *multiline, indent, false))
        }
        Expr::Spawn { call, detached, .. } => {
            format!("{} {}", if *detached { "detach" } else { "spawn" }, print_expr(call, indent))
        }
        Expr::MapLit { key, value, entries, multiline, .. } => {
            let items: Vec<String> = entries
                .iter()
                .map(|e| format!("{}: {}", print_expr(&e.key, indent + 1), print_expr(&e.value, indent + 1)))
                .collect();
            format!(
                "map<{}, {}>{}",
                print_type_node(key, indent),
                print_type_node(value, indent),
                print_braced_list(&items, "{", "}", *multiline, indent, false)
            )
        }
        Expr::Select { arms, default_arm, .. } => {
            let mut printer = Printer::new(&[]);
            for arm in arms {
                printer.emit(format!(
                    "{}{} := <-{} => {}",
                    indent_of(indent + 1),
                    arm.name,
                    print_expr(&arm.channel, indent + 1),
                    print_expr(&arm.body, indent + 1)
                ));
            }
            if let Some(d) = default_arm {
                printer.emit(format!("{}_ => {}", indent_of(indent + 1), print_expr(d, indent + 1)));
            }
            let body = printer.result().trim_end_matches('\n').to_string();
            format!("select {{\n{body}\n{}}}", indent_of(indent))
        }
    }
}

// `{ ... }` / `[ ... ]` / `( ... )` の中身を、元が複数行だったかに応じて印字する共通部分。
// comma=true(呼び出し引数)は末尾にも","を付ける。ASI(セミコロン自動挿入)は","の直後の
// 改行にはセミコロンを挿さないので、これで閉じ括弧を独立行に置いても安全になる——素朴に
// 改行だけ入れると、直前の項目の最後のトークンの後にセミコロンが挿入され、カンマ必須の
// 引数リスト文法と衝突して構文エラーになる(TS版が実際に踏んだバグ: `print(match r {...})`
// のような複数行呼び出しが軒並み壊れていた)。struct/array/mapリテラル(comma=false)は
// 要素間が改行のみで区切れる文法(skip_semisが吸収する)なので、この問題自体が元から無い
fn print_braced_list(items: &[String], open: &str, close: &str, multiline: bool, indent: usize, comma: bool) -> String {
    if items.is_empty() {
        return format!("{open}{close}");
    }
    if !multiline {
        return format!("{open}{}{close}", items.join(", "));
    }
    let inner = indent_of(indent + 1);
    let sep = if comma { "," } else { "" };
    let body: Vec<String> = items.iter().map(|it| format!("{inner}{it}{sep}")).collect();
    format!("{open}\n{}\n{}{close}", body.join("\n"), indent_of(indent))
}

// `$`の直後に`{`が続くリテラル文字は再パース時に補間開始だと誤認されるので、常に`\$`として
// エスケープし直す(TS版が実際に踏んだバグ: `"\${1+1}"`というリテラル文字列が、フォーマット後は
// 本当に評価されて"2"に化けていた——フォーマッタがプログラムの意味を変えてはいけない)
fn escape_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '$' if chars.peek() == Some(&'{') => out.push_str("\\$"),
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    // 整形結果がもう一度整形しても変わらない(冪等)ことも同時に確かめる
    fn fmt(src: &str) -> String {
        let once = format(src).expect("整形対象はパースできること");
        let twice = format(&once).expect("整形結果は再度パースできること");
        assert_eq!(once, twice, "整形は冪等であるべき");
        once
    }

    #[test]
    fn インデントはタブに正規化される() {
        let out = fmt("fn main() {\n      print(1)\n}\n");
        assert_eq!(out, "fn main() {\n\tprint(1)\n}\n");
    }

    #[test]
    fn トップレベル宣言の間には空行を1つ入れる() {
        let out = fmt("fn a() {\n}\nfn b() {\n}\n");
        assert_eq!(out, "fn a() {\n}\n\nfn b() {\n}\n");
    }

    #[test]
    fn importの後に空行を入れる() {
        let out = fmt("import \"mathutil\"\nfn main() {\n}\n");
        assert_eq!(out, "import \"mathutil\"\n\nfn main() {\n}\n");
    }

    #[test]
    fn コメントはleadingとtrailingに振り分けられる() {
        let out = fmt("fn main() {\n// 先頭のコメント\nprint(1) // 行末のコメント\n}\n");
        assert_eq!(out, "fn main() {\n\t// 先頭のコメント\n\tprint(1)  // 行末のコメント\n}\n");
    }

    #[test]
    fn 元が複数行のリテラルは複数行のまま() {
        let src = "fn main() {\n\txs := [\n\t\t1,\n\t\t2,\n\t]\n\tprint(xs)\n}\n";
        assert_eq!(fmt(src), "fn main() {\n\txs := [\n\t\t1\n\t\t2\n\t]\n\tprint(xs)\n}\n");
        // 1行で書いたものは1行のまま(gofmt方式: 幅による自動折り返しをしない)
        let one = "fn main() {\n\txs := [1, 2]\n\tprint(xs)\n}\n";
        assert_eq!(fmt(one), one);
    }

    #[test]
    fn 複数行の呼び出し引数は末尾カンマ付きで出す() {
        // ASI対策: カンマが無いと閉じ括弧の前でセミコロンが挿入され再パースで壊れる
        let src = "fn main() {\n\tprint(\n\t\t1,\n\t\t2,\n\t)\n}\n";
        assert_eq!(fmt(src), src);
    }

    #[test]
    fn 補間に見える文字列リテラルはエスケープし直す() {
        // フォーマッタがプログラムの意味を変えてはいけない
        let out = fmt("fn main() {\n\tprint(\"\\${1}\")\n}\n");
        assert!(out.contains("\\${1}"), "out: {out}");
    }

    #[test]
    fn 無名関数は元が1行なら1行のまま() {
        let src = "fn main() {\n\tf := fn(n: int) bool { return n > 3 }\n\tprint(f)\n}\n";
        assert_eq!(fmt(src), src);
    }

    #[test]
    fn if_else連鎖は閉じ括弧と同じ行にまとめる() {
        let src = "fn main() {\n\tif 1 > 2 {\n\t\tprint(1)\n\t} else if 2 > 3 {\n\t\tprint(2)\n\t} else {\n\t\tprint(3)\n\t}\n}\n";
        assert_eq!(fmt(src), src);
    }

    #[test]
    fn 複数行のunion宣言は改行を保つ() {
        // 先頭メンバーは`=`の直後(行頭`|`は先頭には付かない文法)
        let src = "type Resp =\n\t{ kind: \"ok\" }\n\t| { kind: \"err\" }\n\nfn main() {\n}\n";
        // 先頭メンバーは`=`の後ろに続き、2つ目以降が`| `付きの継続行になる
        let out = fmt(src);
        assert!(out.contains("type Resp = { kind: \"ok\" }\n\t| { kind: \"err\" }"), "out: {out}");
    }
}
