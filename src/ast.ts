// AST (抽象構文木) = パースした結果の木構造。
// 例: `x := 1 + 2` は
//   ShortVarDecl { names: ["x"], values: [Binary { op: "+", left: Int(1), right: Int(2) }] }
// のようなノードになる。

import type { Pos } from "./token";
import type { Type } from "./types";

// ---- 型の構文ノード(ソースに書かれた型注釈) ----
export type TypeNode =
  | { kind: "name"; name: string; pos: Pos } // int, string, error, none, Status(alias)など
  | { kind: "literal"; value: string; pos: Pos } // "active" — 文字列リテラル型
  | { kind: "array"; elem: TypeNode; pos: Pos } // int[]
  | { kind: "chan"; elem: TypeNode; pos: Pos } // chan<int>
  | { kind: "union"; members: TypeNode[]; pos: Pos } // int | error
  | { kind: "structType"; fields: StructFieldNode[]; pos: Pos }; // struct 宣言の中身

export interface StructFieldNode {
  name: string;
  type: TypeNode;
  pos: Pos;
}

// ---- 宣言 ----
export interface Program {
  kind: "program";
  types: TypeDecl[];
  fns: FnDecl[];
}

export interface TypeDecl {
  kind: "typeDecl"; // type Status = "active" | "banned"
  name: string;
  node: TypeNode;
  pos: Pos;
}

export interface Param {
  name: string;
  type: TypeNode;
  pos: Pos;
}

export interface FnDecl {
  kind: "fnDecl";
  name: string;
  params: Param[];
  ret: TypeNode | null; // 戻り値なし = null。失敗し得るなら `int | error` のような union
  body: Block;
  pos: Pos;
}

// ---- 文 ----
export interface Block {
  kind: "block";
  stmts: Stmt[];
}

export type Stmt =
  | ShortVarDecl
  | Assign
  | ExprStmt
  | ReturnStmt
  | IfStmt
  | ForStmt
  | GoStmt
  | SendStmt
  | IncDecStmt
  | BreakStmt
  | ContinueStmt;

export interface ShortVarDecl {
  kind: "shortVarDecl"; // x := 1  /  mut x := 0  /  v, err := f()
  names: string[];
  values: Expr[];
  mutable: boolean; // mut 付き宣言なら true(全 names に適用)
  pos: Pos;
}

export interface Assign {
  kind: "assign"; // x = 1  /  v, err = f()
  targets: Expr[]; // ident / index / member
  values: Expr[];
  pos: Pos;
}

export interface ExprStmt {
  kind: "exprStmt";
  expr: Expr;
  pos: Pos;
}

export interface ReturnStmt {
  kind: "return";
  value: Expr | null; // 多値戻りは廃止(union路線)。常に単一の値
  pos: Pos;
}

export interface IfStmt {
  kind: "if";
  cond: Expr;
  then: Block;
  else_: IfStmt | Block | null;
  pos: Pos;
}

export interface ForStmt {
  kind: "for";
  init: Stmt | null; // for init; cond; post { }
  cond: Expr | null; // for cond { } / for { }
  post: Stmt | null;
  body: Block;
  pos: Pos;
}

export interface GoStmt {
  kind: "go"; // go f(x)
  call: CallExpr;
  pos: Pos;
}

export interface SendStmt {
  kind: "send"; // ch <- v
  channel: Expr;
  value: Expr;
  pos: Pos;
}

export interface IncDecStmt {
  kind: "incDec"; // i++ / i--
  target: Expr;
  op: "++" | "--";
  pos: Pos;
}

export interface BreakStmt {
  kind: "break";
  pos: Pos;
}

export interface ContinueStmt {
  kind: "continue";
  pos: Pos;
}

// ---- 式 ----
// resolvedType は Checker が推論結果を書き込むフィールド(Codegen が参照する)
interface ExprBase {
  pos: Pos;
  resolvedType?: Type;
}

export type Expr =
  | IntLit
  | FloatLit
  | StringLit
  | InterpExpr
  | BoolLit
  | NoneLit
  | Ident
  | ArrayLit
  | BinaryExpr
  | UnaryExpr
  | RecvExpr
  | CallExpr
  | IndexExpr
  | MemberExpr
  | FnExpr
  | ChanExpr
  | IsExpr
  | PropExpr
  | OrElseExpr
  | MatchExpr
  | StructLit;

export interface IntLit extends ExprBase {
  kind: "int";
  value: string;
}
export interface FloatLit extends ExprBase {
  kind: "float";
  value: string;
}
export interface StringLit extends ExprBase {
  kind: "string";
  value: string;
}
export interface InterpExpr extends ExprBase {
  kind: "interp"; // "worker ${id} done"
  segments: InterpSegment[];
}
export type InterpSegment = { kind: "text"; text: string } | { kind: "expr"; expr: Expr };
export interface BoolLit extends ExprBase {
  kind: "bool";
  value: boolean;
}
export interface NoneLit extends ExprBase {
  kind: "none"; // 不在の値。T | none の union にだけ入れられる
}
export interface Ident extends ExprBase {
  kind: "ident";
  name: string;
}
export interface ArrayLit extends ExprBase {
  kind: "arrayLit"; // [1, 2, 3]
  elems: Expr[];
}
export interface BinaryExpr extends ExprBase {
  kind: "binary";
  op: string;
  left: Expr;
  right: Expr;
  intDiv?: boolean; // int / int のとき Checker が立てる(切り捨て+ゼロ除算検査)
  intMod?: boolean; // int % int のとき Checker が立てる(ゼロ剰余検査)
}
export interface UnaryExpr extends ExprBase {
  kind: "unary"; // !x / -x
  op: "!" | "-";
  operand: Expr;
}
export interface RecvExpr extends ExprBase {
  kind: "recv"; // <-ch
  channel: Expr;
}
export interface CallExpr extends ExprBase {
  kind: "call";
  callee: Expr;
  args: Expr[];
}
export interface IndexExpr extends ExprBase {
  kind: "index"; // a[i]
  target: Expr;
  index: Expr;
}
export interface MemberExpr extends ExprBase {
  kind: "member"; // obj.name
  target: Expr;
  name: string;
}
export interface FnExpr extends ExprBase {
  kind: "fnExpr"; // 無名関数: fn(x: int) int { return x * 2 }
  params: Param[];
  ret: TypeNode | null;
  body: Block;
}
export interface ChanExpr extends ExprBase {
  kind: "chanExpr"; // chan<int>()
  elem: TypeNode;
}
export interface IsExpr extends ExprBase {
  kind: "is"; // x is none / x is error — narrowing の起点
  operand: Expr;
  target: TypeNode;
}
export interface PropExpr extends ExprBase {
  kind: "prop"; // f()! — none/error なら呼び出し元へ即伝播
  operand: Expr;
}
export interface OrElseExpr extends ExprBase {
  kind: "orElse"; // f() or fallback — none/error なら右辺の値を使う
  left: Expr;
  right: Expr;
}
export interface StructLit extends ExprBase {
  kind: "structLit"; // User{name: "alice", age: 30}
  name: string;
  fields: { name: string; value: Expr; pos: Pos }[];
}
export interface MatchExpr extends ExprBase {
  kind: "match"; // match r { error => "failed"  int => "ok: ${r}" }
  subject: Expr;
  arms: MatchArm[];
}
export interface MatchArm {
  patterns: MatchPattern[]; // カンマ区切りで複数可
  body: Expr; // v1 は単一式のみ(ブロックアームは将来)
  pos: Pos;
}
export type MatchPattern =
  | { kind: "type"; type: TypeNode } // none / error / int などの型パターン
  | { kind: "wildcard"; pos: Pos }; // _ (最後のアームのみ)
