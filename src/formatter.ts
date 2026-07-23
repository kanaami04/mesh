// mesh fmt: AST を歩いて正規形のソースへ印字し直す(gofmt相当・設定オプションなし)。
//
// 方針(2026-07-20、kanayamaと討議のうえ決定):
// - インデントはタブ固定
// - 改行の有無は「幅に応じた自動折り返し」をしない(gofmt方式)。struct/array/mapリテラル・
//   関数呼び出しの引数・union宣言は、元のソースで複数行だったかをparserが`multiline`フラグに
//   記録済みで、印字はそれをそのまま尊重するだけ(このファイルでは判断しない)
// - コメントはlexerが別配列(Program.comments)へ退避済み。印字時に「直前の要素と同じ行にある
//   コメント→trailing」「そうでなければ次の要素の直前へ→leading」という位置ベースの単純な規則で
//   再割り当てする。コメントを絶対に消さない(見つけたコメントは必ずどこかに出す)ことを優先し、
//   複数行にまたがる式の途中に挟まったコメントの位置までは厳密に再現しない(既知の限界)
//
// 既知の限界(v1):
// - 空行の保持はしない — トップレベル宣言間・import後には常に1行、文の間は常に0行という
//   固定ルールで正規化する(gofmtは元の空行を最大1行まで保持するが、それには各ノードの
//   終了位置の追跡が要り、現状のASTには無い。将来ASTに終了位置を足すなら追随できる)
// - struct/array/mapリテラルの途中、関数呼び出しの引数の途中にあるコメントは、直後の文の
//   leadingコメントとして出る(位置がずれるだけで消えはしない)

import type {
  Block,
  Expr,
  FnDecl,
  Program,
  Stmt,
  StructFieldNode,
  TypeDecl,
  TypeNode,
} from "./ast";
import { parse } from "./parser";
import type { CommentInfo } from "./token";

export function format(source: string): string {
  const program = parse(source);
  return printProgram(program);
}

const INDENT = "\t";
const indentOf = (level: number) => INDENT.repeat(level);

class Printer {
  private lines: string[] = [];
  private comments: CommentInfo[];
  private idx = 0;

  constructor(comments: CommentInfo[]) {
    this.comments = [...comments].sort((a, b) => a.pos.line - b.pos.line || a.pos.col - b.pos.col);
  }

  // beforeLine より前にある未消費コメントを、それぞれ単独行のleadingコメントとして出す
  flushLeadingComments(beforeLine: number, indent: number) {
    while (this.idx < this.comments.length && this.comments[this.idx].pos.line < beforeLine) {
      this.lines.push(indentOf(indent) + this.comments[this.idx].text);
      this.idx++;
    }
  }

  // line と同じ行にある未消費コメントが1つあれば消費して返す(trailing用)。無ければundefined
  takeTrailingComment(line: number): string | undefined {
    if (this.idx < this.comments.length && this.comments[this.idx].pos.line === line) {
      return this.comments[this.idx++].text;
    }
    return undefined;
  }

  // 安全弁: 最後まで誰にも回収されなかったコメント(例えばファイル末尾の孤立コメント)を
  // 単独行として出す — 「見つけたコメントは消さない」を最後まで保証する
  flushRemaining(indent: number) {
    while (this.idx < this.comments.length) {
      this.lines.push(indentOf(indent) + this.comments[this.idx].text);
      this.idx++;
    }
  }

  emit(text: string) {
    this.lines.push(text);
  }

  blank() {
    this.lines.push("");
  }

  result(): string {
    // 末尾に空行を溜めない(トップレベル最後の項目がblank()を呼んだ場合の後始末)
    while (this.lines.length > 0 && this.lines[this.lines.length - 1] === "") this.lines.pop();
    return this.lines.join("\n") + "\n";
  }
}

function printProgram(program: Program): string {
  const p = new Printer(program.comments);

  for (const imp of program.imports) {
    p.flushLeadingComments(imp.pos.line, 0);
    const trailing = p.takeTrailingComment(imp.pos.line);
    p.emit(`import "${imp.path}"` + (trailing ? "  " + trailing : ""));
  }

  type TopLevelItem =
    | { pos: { line: number }; print: () => void };

  const items: TopLevelItem[] = [
    ...program.types.map((t) => ({ pos: t.pos, print: () => printTypeDecl(p, t) })),
    ...program.fns.map((f) => ({ pos: f.pos, print: () => printFnDecl(p, f) })),
    ...program.consts.map((c) => ({
      pos: c.pos,
      print: () => {
        const kw = c.exported ? "export " : "";
        const typeAnn = c.typeNode ? `: ${printTypeNode(c.typeNode, 0)}` : "";
        const op = c.typeNode ? "=" : ":=";
        const trailing = p.takeTrailingComment(c.pos.line);
        p.emit(`${kw}${c.name}${typeAnn} ${op} ${printExpr(c.value, 0)}` + (trailing ? "  " + trailing : ""));
      },
    })),
  ].sort((a, b) => a.pos.line - b.pos.line);

  if (program.imports.length > 0 && items.length > 0) p.blank();

  items.forEach((item, i) => {
    if (i > 0) p.blank();
    p.flushLeadingComments(item.pos.line, 0);
    item.print();
  });

  p.flushRemaining(0);
  return p.result();
}

function printTypeDecl(p: Printer, decl: TypeDecl) {
  const kw = decl.exported ? "export " : "";
  const errKw = decl.isError ? "error " : "";
  const jsonKw = decl.isJson ? "json " : "";
  if (decl.node.kind === "structType") {
    p.emit(`${kw}${errKw}${jsonKw}struct ${decl.name} {`);
    for (const f of decl.node.fields) {
      p.flushLeadingComments(f.pos.line, 1);
      const trailing = p.takeTrailingComment(f.pos.line);
      p.emit(`${indentOf(1)}${f.name}: ${printTypeNode(f.type, 1)}` + (trailing ? "  " + trailing : ""));
    }
    p.emit("}");
    return;
  }
  const trailing = p.takeTrailingComment(decl.pos.line);
  const rhs = printTypeNode(decl.node, 0);
  p.emit(`${kw}${errKw}type ${decl.name} = ${rhs}` + (trailing ? "  " + trailing : ""));
}

function printFnDecl(p: Printer, fn: FnDecl) {
  const kw = fn.exported ? "export " : "";
  const recv = fn.receiver ? `(${fn.receiver.name}: ${printTypeNode(fn.receiver.type, 0)}) ` : "";
  const typeParams = fn.typeParams.length > 0 ? `<${fn.typeParams.join(", ")}>` : "";
  const params = fn.params.map((param) => `${param.name}: ${printTypeNode(param.type, 0)}`).join(", ");
  const ret = fn.ret ? ` ${printTypeNode(fn.ret, 0)}` : "";
  p.emit(`${kw}fn ${recv}${fn.name}${typeParams}(${params})${ret} {`);
  printBlockStmts(p, fn.body, 1);
  p.emit("}");
}

function printBlockStmts(p: Printer, block: Block, indent: number) {
  for (const stmt of block.stmts) {
    p.flushLeadingComments(stmt.pos.line, indent);
    printStmt(p, stmt, indent);
  }
}

function printStmt(p: Printer, stmt: Stmt, indent: number) {
  const ind = indentOf(indent);
  const emitSimple = (text: string) => {
    const trailing = p.takeTrailingComment(stmt.pos.line);
    p.emit(ind + text + (trailing ? "  " + trailing : ""));
  };

  switch (stmt.kind) {
    case "shortVarDecl": {
      const kw = stmt.mutable ? "mut " : "";
      emitSimple(`${kw}${stmt.names.join(", ")} := ${stmt.values.map((v) => printExpr(v, indent)).join(", ")}`);
      return;
    }
    case "typedVarDecl": {
      const kw = stmt.mutable ? "mut " : "";
      emitSimple(`${kw}${stmt.name}: ${printTypeNode(stmt.typeNode, indent)} = ${printExpr(stmt.value, indent)}`);
      return;
    }
    case "assign": {
      const op = stmt.compoundOp ? `${stmt.compoundOp}=` : "=";
      emitSimple(
        `${stmt.targets.map((t) => printExpr(t, indent)).join(", ")} ${op} ` +
          stmt.values.map((v) => printExpr(v, indent)).join(", "),
      );
      return;
    }
    case "exprStmt":
      emitSimple(printExpr(stmt.expr, indent));
      return;
    case "return":
      emitSimple(stmt.value === null ? "return" : `return ${printExpr(stmt.value, indent)}`);
      return;
    case "if":
      printIf(p, stmt, indent);
      return;
    case "for": {
      const init = stmt.init ? printStmtInline(stmt.init, indent) : "";
      const cond = stmt.cond ? printExpr(stmt.cond, indent) : "";
      const post = stmt.post ? printStmtInline(stmt.post, indent) : "";
      const header =
        stmt.init || stmt.post ? `for ${init}; ${cond}; ${post}` : stmt.cond ? `for ${cond}` : "for";
      const trailing = p.takeTrailingComment(stmt.pos.line);
      p.emit(ind + header + " {" + (trailing ? "  " + trailing : ""));
      printBlockStmts(p, stmt.body, indent + 1);
      p.emit(ind + "}");
      return;
    }
    case "rangeFor": {
      const trailing = p.takeTrailingComment(stmt.pos.line);
      p.emit(
        ind + `for ${stmt.names.join(", ")} := range ${printExpr(stmt.subject, indent)} {` +
          (trailing ? "  " + trailing : ""),
      );
      printBlockStmts(p, stmt.body, indent + 1);
      p.emit(ind + "}");
      return;
    }
    case "wait": {
      const trailing = p.takeTrailingComment(stmt.pos.line);
      p.emit(ind + "wait {" + (trailing ? "  " + trailing : ""));
      printBlockStmts(p, stmt.body, indent + 1);
      p.emit(ind + "}");
      return;
    }
    case "send":
      emitSimple(`${printExpr(stmt.channel, indent)} <- ${printExpr(stmt.value, indent)}`);
      return;
    case "incDec":
      emitSimple(`${printExpr(stmt.target, indent)}${stmt.op}`);
      return;
    case "break":
      emitSimple("break");
      return;
    case "continue":
      emitSimple("continue");
      return;
  }
}

// for文のヘッダ(init/post)専用: 単一文をインデント・トレイリングコメント無しでそのまま文字列化する
// for文のヘッダ(init/post)専用: parserがそこに許すのはこの4種のみなので必ず非nullで返る
function printStmtInline(stmt: Stmt, indent: number): string {
  return tryPrintStmtInline(stmt, indent) ?? ""; // 到達しない
}

// 1行に収められる単純文だけを文字列化する(コメント付与・インデント無し)。if/for/wait/
// rangeForのようなブロックを持つ文はnullを返し、呼び出し側に複数行フォールバックさせる
// (fnExprの1行書き判定、for文ヘッダの両方で使う共通部分)
function tryPrintStmtInline(stmt: Stmt, indent: number): string | null {
  switch (stmt.kind) {
    case "shortVarDecl": {
      const kw = stmt.mutable ? "mut " : "";
      return `${kw}${stmt.names.join(", ")} := ${stmt.values.map((v) => printExpr(v, indent)).join(", ")}`;
    }
    case "typedVarDecl": {
      const kw = stmt.mutable ? "mut " : "";
      return `${kw}${stmt.name}: ${printTypeNode(stmt.typeNode, indent)} = ${printExpr(stmt.value, indent)}`;
    }
    case "assign": {
      const op = stmt.compoundOp ? `${stmt.compoundOp}=` : "=";
      return `${stmt.targets.map((t) => printExpr(t, indent)).join(", ")} ${op} ` +
        stmt.values.map((v) => printExpr(v, indent)).join(", ");
    }
    case "incDec":
      return `${printExpr(stmt.target, indent)}${stmt.op}`;
    case "exprStmt":
      return printExpr(stmt.expr, indent);
    case "return":
      return stmt.value === null ? "return" : `return ${printExpr(stmt.value, indent)}`;
    case "send":
      return `${printExpr(stmt.channel, indent)} <- ${printExpr(stmt.value, indent)}`;
    case "break":
      return "break";
    case "continue":
      return "continue";
    default:
      return null; // if/for/wait/rangeFor: ブロックを持つので1行化しない
  }
}

function printIf(p: Printer, stmt: Extract<Stmt, { kind: "if" }>, indent: number) {
  const ind = indentOf(indent);
  const trailing = p.takeTrailingComment(stmt.pos.line);
  p.emit(ind + `if ${printExpr(stmt.cond, indent)} {` + (trailing ? "  " + trailing : ""));
  printBlockStmts(p, stmt.then, indent + 1);
  printElseChain(p, stmt.else_, indent);
}

// "} else if ... {" / "} else {" は閉じ括弧と同じ行にまとめる(独立したif文としてではなく、
// 1つのif/else連鎖として印字する)。再帰でどれだけ else if が連なっても対応する
function printElseChain(p: Printer, else_: Extract<Stmt, { kind: "if" }>["else_"], indent: number) {
  const ind = indentOf(indent);
  if (else_ === null) {
    p.emit(ind + "}");
    return;
  }
  if (else_.kind === "if") {
    const trailing = p.takeTrailingComment(else_.pos.line);
    p.emit(ind + `} else if ${printExpr(else_.cond, indent)} {` + (trailing ? "  " + trailing : ""));
    printBlockStmts(p, else_.then, indent + 1);
    printElseChain(p, else_.else_, indent);
    return;
  }
  p.emit(ind + "} else {");
  printBlockStmts(p, else_, indent + 1);
  p.emit(ind + "}");
}

function printTypeNode(t: TypeNode, indent: number): string {
  switch (t.kind) {
    case "name":
      return t.pkg ? `${t.pkg}.${t.name}` : t.name;
    case "literal":
      return `"${escapeString(t.value)}"`;
    case "array":
      return `${printTypeNode(t.elem, indent)}[]`;
    case "chan":
      return `chan<${printTypeNode(t.elem, indent)}>`;
    case "mapType":
      return `map<${printTypeNode(t.key, indent)}, ${printTypeNode(t.value, indent)}>`;
    case "union": {
      if (t.multiline && t.members.length > 1) {
        const contIndent = indentOf(indent + 1);
        return t.members
          .map((m, i) => (i === 0 ? printTypeNode(m, indent) : `\n${contIndent}| ${printTypeNode(m, indent)}`))
          .join("");
      }
      return t.members.map((m) => printTypeNode(m, indent)).join(" | ");
    }
    case "structType":
      return printInlineStructFields(t.fields, indent);
    case "fnType":
      return `fn(${t.params.map((p) => printTypeNode(p, indent)).join(", ")})` +
        (t.ret ? ` ${printTypeNode(t.ret, indent)}` : "");
  }
}

function printInlineStructFields(fields: StructFieldNode[], indent: number): string {
  const body = fields.map((f) => `${f.name}: ${printTypeNode(f.type, indent)}`).join(", ");
  return `{ ${body} }`;
}

function printExpr(expr: Expr, indent: number): string {
  switch (expr.kind) {
    case "int":
      return expr.value;
    case "float":
      return expr.value;
    case "string":
      return `"${escapeString(expr.value)}"`;
    case "interp":
      return `"${expr.segments.map((s) => (s.kind === "text" ? escapeString(s.text) : `\${${printExpr(s.expr, indent)}}`)).join("")}"`;
    case "bool":
      return String(expr.value);
    case "none":
      return "none";
    case "ident":
      return expr.name;
    case "arrayLit": {
      const prefix = expr.elemType ? `${printTypeNode(expr.elemType, indent)}[]` : "";
      return prefix + printBracedList(expr.elems.map((e) => printExpr(e, indent + 1)), "[", "]", expr.multiline, indent);
    }
    case "binary":
      return `${printExpr(expr.left, indent)} ${expr.op} ${printExpr(expr.right, indent)}`;
    case "unary":
      return `${expr.op}${printExpr(expr.operand, indent)}`;
    case "recv":
      return `<-${printExpr(expr.channel, indent)}`;
    case "call": {
      const args = expr.args.map((a) => printExpr(a, indent + 1));
      return `${printExpr(expr.callee, indent)}${printBracedList(args, "(", ")", expr.multiline, indent, true)}`;
    }
    case "index":
      return `${printExpr(expr.target, indent)}[${printExpr(expr.index, indent)}]`;
    case "member":
      return `${printExpr(expr.target, indent)}.${expr.name}`;
    case "fnExpr": {
      const params = expr.params.map((param) => `${param.name}: ${printTypeNode(param.type, indent)}`).join(", ");
      const ret = expr.ret ? ` ${printTypeNode(expr.ret, indent)}` : "";
      // インラインクロージャは1行書きが慣習的に多い(fn(n: int) bool { return n > 3 } のように)。
      // 元が1行で、かつ中身が単純文だけならその形を尊重する。複雑な制御構文が混ざる、または
      // 元から複数行だったなら、通常のブロックと同じ複数行印字にフォールバックする
      if (expr.body.multiline === false) {
        const inlineParts = expr.body.stmts.map((s) => tryPrintStmtInline(s, indent));
        if (inlineParts.every((s): s is string => s !== null)) {
          const body = inlineParts.length > 0 ? ` ${inlineParts.join("; ")} ` : "";
          return `fn(${params})${ret} {${body}}`;
        }
      }
      const inner = new Printer([]);
      printBlockStmts(inner, expr.body, indent + 1);
      const body = inner.result().replace(/\n$/, "");
      return `fn(${params})${ret} {\n${body}\n${indentOf(indent)}}`;
    }
    case "chanExpr":
      return `chan<${printTypeNode(expr.elem, indent)}>(${printExpr(expr.capacity, indent)})`;
    case "is":
      return `${printExpr(expr.operand, indent)} is ${printTypeNode(expr.target, indent)}`;
    case "prop":
      return `${printExpr(expr.operand, indent)}?` + (expr.context ? ` ${printExpr(expr.context, indent)}` : "");
    case "orElse":
      return `${printExpr(expr.left, indent)} or ` +
        (expr.binding !== undefined ? `${expr.binding} => ${printExpr(expr.right, indent)}` : printExpr(expr.right, indent));
    case "match": {
      const inner = new Printer([]);
      for (const arm of expr.arms) {
        const patterns = arm.patterns
          .map((pat) => (pat.kind === "wildcard" ? "_" : printTypeNode(pat.type, indent + 1)))
          .join(", ");
        inner.emit(`${indentOf(indent + 1)}${patterns} => ${printExpr(arm.body, indent + 1)}`);
      }
      const body = inner.result().replace(/\n$/, "");
      return `match ${printExpr(expr.subject, indent)} {\n${body}\n${indentOf(indent)}}`;
    }
    case "structLit": {
      const name = expr.pkg ? `${expr.pkg}.${expr.name}` : expr.name;
      const fields = expr.fields.map((f) => `${f.name}: ${printExpr(f.value, indent + 1)}`);
      return `${name}${printBracedList(fields, "{", "}", expr.multiline, indent)}`;
    }
    case "spawn":
      return `${expr.detached ? "detach" : "spawn"} ${printExpr(expr.call, indent)}`;
    case "mapLit": {
      const entries = expr.entries.map((e) => `${printExpr(e.key, indent + 1)}: ${printExpr(e.value, indent + 1)}`);
      return `map<${printTypeNode(expr.key, indent)}, ${printTypeNode(expr.value, indent)}>` +
        printBracedList(entries, "{", "}", expr.multiline, indent);
    }
    case "select": {
      const inner = new Printer([]);
      for (const arm of expr.arms) {
        inner.emit(
          `${indentOf(indent + 1)}${arm.name} := <-${printExpr(arm.channel, indent + 1)} => ${printExpr(arm.body, indent + 1)}`,
        );
      }
      if (expr.defaultArm) {
        inner.emit(`${indentOf(indent + 1)}_ => ${printExpr(expr.defaultArm, indent + 1)}`);
      }
      const body = inner.result().replace(/\n$/, "");
      return `select {\n${body}\n${indentOf(indent)}}`;
    }
  }
}

// { ... } / [ ... ] / ( ... ) の中身を、元が複数行だったかに応じて印字する共通部分。
// braceStyle=true の場合(struct/mapリテラル)は要素間を改行時カンマ無しで揃える
// (既存の複数行リテラルの慣習と同じ書式)
function printBracedList(
  items: string[],
  open: string,
  close: string,
  multiline: boolean | undefined,
  indent: number,
  comma = false,
): string {
  if (items.length === 0) return `${open}${close}`;
  if (!multiline) return `${open}${items.join(", ")}${close}`;
  const inner = indentOf(indent + 1);
  // 呼び出し引数(comma=true)は末尾にも","を付ける。ASI(セミコロン自動挿入)は","の直後の
  // 改行にはセミコロンを挿さない(","はASI_AFTER集合に無い)ので、これで閉じ括弧を独立行に
  // 置いても安全になる — 素朴に改行だけ入れると、直前の項目の最後のトークン(ほぼ何でも
  // ASI_AFTER対象)の後にセミコロンが挿入され、カンマ必須の引数リスト文法と衝突して構文
  // エラーになる(実際に踏んだバグ: print(match r {...}) や dist(Point{...}) のような
  // 複数行呼び出しが軒並み壊れていた)。struct/array/mapリテラル(comma=false)は要素間が
  // 改行のみで区切れる文法(skipSemisが吸収する)なので、この問題自体が元から無い
  const body = items.map((it) => `${inner}${it}${comma ? "," : ""}`).join("\n");
  return `${open}\n${body}\n${indentOf(indent)}${close}`;
}

function escapeString(s: string): string {
  // $の直後に{が続くリテラル文字は再パース時に補間開始だと誤認されるので、常に\$として
  // エスケープし直す(実際に踏んだバグ: "\${1+1}"というリテラル文字列が、フォーマット後は
  // 本当に評価されて"2"に化けていた — フォーマッタがプログラムの意味を変えてはいけない)
  return s
    .replace(/\\/g, "\\\\")
    .replace(/"/g, '\\"')
    .replace(/\n/g, "\\n")
    .replace(/\t/g, "\\t")
    .replace(/\$\{/g, "\\${");
}
