// AST (抽象構文木) = パースした結果の木構造。
// 例: `x := 1 + 2` は
//   ShortVarDecl { names: ["x"], values: [Binary { op: "+", left: Int(1), right: Int(2) }] }
// のようなノードになる。

import type { CommentInfo, Pos } from "./token";
import type { Type } from "./types";

// ---- 型の構文ノード(ソースに書かれた型注釈) ----
export type TypeNode =
  | { kind: "name"; name: string; pkg?: string; pos: Pos } // int, string, Status など。pkg付きは math.User(別パッケージのexported型)
  | { kind: "literal"; value: string; pos: Pos } // "active" — 文字列リテラル型
  | { kind: "array"; elem: TypeNode; pos: Pos } // int[]
  | { kind: "chan"; elem: TypeNode; pos: Pos } // chan<int>
  | { kind: "mapType"; key: TypeNode; value: TypeNode; pos: Pos } // map<string, int>
  // int | error。multiline: mesh fmt(gofmt方式)がユーザーの改行選択をそのまま尊重するための印
  | { kind: "union"; members: TypeNode[]; pos: Pos; multiline?: boolean }
  | { kind: "structType"; fields: StructFieldNode[]; pos: Pos } // struct 宣言の中身
  // fn(int, string) bool — 関数型。宣言と同じ読み(戻り値のunionは戻り値側に束縛)。
  // ret が null なら戻り値なし(void)。パラメータ名は書かない(型のみ)
  | { kind: "fnType"; params: TypeNode[]; ret: TypeNode | null; pos: Pos };

export interface StructFieldNode {
  name: string;
  type: TypeNode;
  pos: Pos;
}

// ---- 宣言 ----
export interface Program {
  kind: "program";
  imports: ImportDecl[];
  types: TypeDecl[];
  fns: FnDecl[];
  consts: ConstDecl[];
  // 生のコメント一覧(位置つき、AST上のどのノードにも紐づいていない)。将来のmesh fmtが
  // 印字時に再合成するための素材 — checker/codegenは一切参照しない
  comments: CommentInfo[];
}

export interface ConstDecl {
  kind: "constDecl"; // x := 10  /  x: int = 10  /  export x := 10(F-9c: トップレベル定数。常に不変)
  name: string;
  typeNode: TypeNode | null; // 型注釈があれば(x: int = 10)。無ければ値から推論(x := 10)
  value: Expr;
  exported: boolean;
  pos: Pos;
}

export interface ImportDecl {
  kind: "importDecl"; // import "math" — プロジェクトルート直下のディレクトリをパッケージとして取り込む
  path: string;
  alias: string; // 修飾に使う名前(= パスの最終セグメント。v1は単一セグメントのみ)
  pos: Pos;
}

export interface TypeDecl {
  kind: "typeDecl"; // type Status = "active" | "banned"
  name: string;
  node: TypeNode;
  exported: boolean; // export type X = ... / export struct X { ... }
  isError: boolean; // error type X = ... / error struct X { ... }(F-2後半): '?'/'or'の伝播対象にする
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
  receiver: Receiver | null; // fn (u: User) describe() ... — Goスタイルのメソッドレシーバ
  typeParams: string[]; // fn first<T>(...) — トップレベル関数限定(F-1後半)。無ければ空配列
  params: Param[];
  ret: TypeNode | null; // 戻り値なし = null。失敗し得るなら `int | error` のような union
  body: Block;
  exported: boolean; // export fn ...(メソッドは対象外 — structのexportに従う)
  pos: Pos;
}

export interface Receiver {
  name: string;
  type: TypeNode; // v1は struct 型のみ(checkerが検証)
  pos: Pos;
}

// ---- 文 ----
export interface Block {
  kind: "block";
  stmts: Stmt[];
  // mesh fmt用: 元ソースで複数行だったか(空でも{}のまま1行、ということもある)。
  // fnExpr(インラインクロージャ)の印字だけがこれを見る — fn宣言/if/for/wait本体は
  // 常に複数行が既存の慣習なので参照しない
  multiline?: boolean;
}

export type Stmt =
  | ShortVarDecl
  | TypedVarDecl
  | Assign
  | ExprStmt
  | ReturnStmt
  | IfStmt
  | ForStmt
  | RangeForStmt
  | WaitStmt
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

export interface TypedVarDecl {
  kind: "typedVarDecl"; // x: T = v  /  mut best: string | none = none
  name: string;
  typeNode: TypeNode;
  value: Expr;
  mutable: boolean;
  pos: Pos;
}

export interface Assign {
  kind: "assign"; // x = 1  /  v, err = f()  /  x += 1(F-9b: 複合代入。常に単一target/value)
  targets: Expr[]; // ident / index / member
  values: Expr[];
  compoundOp?: "+" | "-" | "*" | "/" | "%"; // += -= *= /= %= のとき Parser が立てる
  intDiv?: boolean; // compoundOpが int同士の /= のとき Checker が立てる(切り捨て+ゼロ検査)
  intMod?: boolean; // compoundOpが int同士の %= のとき Checker が立てる(ゼロ剰余検査)
  intArith?: boolean; // compoundOpが int同士の += -= *= のとき Checker が立てる(F-10: safe-integer検査)
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

export interface WaitStmt {
  kind: "wait"; // wait { spawn f()  spawn g() } — 中で起動したタスクを全部待つ
  body: Block;
  pos: Pos;
}

export interface RangeForStmt {
  kind: "rangeFor"; // for i, v := range arr / for k, v := range m / for i := range 10
  names: string[]; // 1個(int range)または2個。"_" で捨てられる
  subject: Expr;
  body: Block;
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
  | StructLit
  | SpawnExpr
  | MapLit
  | SelectExpr;

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
  kind: "arrayLit"; // [1, 2, 3]  /  Todo[]{}  /  int[]{1, 2}
  elems: Expr[];
  elemType?: TypeNode; // T[]{...} で明示された要素型。空配列や型固定に使う
  multiline?: boolean; // mesh fmt用: 元ソースで要素が複数行にまたがっていたか
}
export interface BinaryExpr extends ExprBase {
  kind: "binary";
  op: string;
  left: Expr;
  right: Expr;
  intDiv?: boolean; // int / int のとき Checker が立てる(切り捨て+ゼロ除算検査)
  intMod?: boolean; // int % int のとき Checker が立てる(ゼロ剰余検査)
  intArith?: boolean; // int同士の + - * のとき Checker が立てる(F-10: safe-integer検査)
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
  multiline?: boolean; // mesh fmt用: 元ソースで引数が複数行にまたがっていたか
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
  resolvedPkg?: string; // math.add のようなパッケージ修飾参照と checker が解決したら入る(codegen用)
}
export interface FnExpr extends ExprBase {
  kind: "fnExpr"; // 無名関数: fn(x: int) int { return x * 2 }
  params: Param[];
  ret: TypeNode | null;
  body: Block;
}
export interface ChanExpr extends ExprBase {
  // chan<int>(none) 無制限バッファ(F-11: 明示必須。既定では選べない) / chan<int>(n) 容量n(nがブロッキング送信)
  kind: "chanExpr";
  elem: TypeNode;
  capacity: Expr; // 常に必須(F-11)。'none' なら無制限、それ以外はint式で容量nを表す
}
export interface IsExpr extends ExprBase {
  kind: "is"; // x is none / x is error — narrowing の起点
  operand: Expr;
  target: TypeNode;
}
export interface PropExpr extends ExprBase {
  kind: "prop"; // f()? — none/error なら呼び出し元へ即伝播
  operand: Expr;
  // f() ? "line ${i}: bad" — 失敗時だけ評価される文脈(文字列リテラル/補間のみ)。
  // 付けた場合、none/error どちらの失敗も error("文脈[: 元メッセージ]") として伝播する
  context?: Expr;
}
export interface OrElseExpr extends ExprBase {
  kind: "orElse"; // f() or fallback — none なら右辺の値を使う(errorには束縛形が必須)
  left: Expr;
  right: Expr;
  // f() or e => fallback — 失敗値(none/error)を e に束縛して右辺を評価。
  // error を含む union のフォールバックはこの形が必須("_" で意図的に捨てたことを字面に残す)
  binding?: string;
}
export interface MapLit extends ExprBase {
  kind: "mapLit"; // map<string, int>{"a": 1, "b": 2}
  key: TypeNode;
  value: TypeNode;
  entries: { key: Expr; value: Expr; pos: Pos }[];
  multiline?: boolean; // mesh fmt用: 元ソースでエントリが複数行にまたがっていたか
}
export interface SpawnExpr extends ExprBase {
  kind: "spawn"; // task := spawn f(x) — 並行起動して結果の受取口(chan<T>)を返す
  call: CallExpr;
  // 2段スコープ設計(2026-07-18決定):
  //   spawn  = 今の関数が所有。関数を抜けるとき暗黙に待たれる(リーク不可能)
  //   detach = プログラムが所有。関数は待たずに戻れる(メール送信等のバックグラウンド用)
  detached: boolean;
}
export interface StructLit extends ExprBase {
  kind: "structLit"; // User{name: "alice", age: 30} / math.Point{x: 1, y: 2}(パッケージ修飾)
  name: string;
  pkg?: string;
  fields: { name: string; value: Expr; pos: Pos }[];
  // checkerが埋める(F-2後半): このリテラルの型がerror typeとしてマークされていれば、
  // codegenが実行時マーカーを埋め込んで '?'/'or' が実際の値から判定できるようにする
  isErrorInstance?: boolean;
  multiline?: boolean; // mesh fmt用: 元ソースでフィールドが複数行にまたがっていたか
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

export interface SelectExpr extends ExprBase {
  kind: "select"; // select { v := <-ch1 => ...  v := <-ch2 => ...  _ => ... }
  arms: SelectArm[];
  defaultArm: Expr | null; // "_" アーム。あれば非ブロッキング(即座に何も準備できていなければこちら)
}
export interface SelectArm {
  name: string; // 受信した値(T | closed)を束縛する名前。アームの body スコープ内だけで有効
  channel: Expr;
  body: Expr;
  pos: Pos;
}
