// トークン = ソースコードを意味のある最小単位に分解したもの。
// 例: `x := 10` → [ident "x"] [":="] [int "10"]

// diagnostic-codes.ts も Pos を import type するが、型のみの相互参照なので
// 実行時の循環importにはならない(import type は型検査後に消える)
import type { DiagnosticCode, Fix } from "./diagnostic-codes";

export interface Pos {
  line: number;
  col: number;
}

export type TokenType =
  // リテラル・識別子
  | "ident"
  | "int"
  | "float"
  | "string"
  // キーワード
  | "fn"
  | "return"
  | "if"
  | "else"
  | "for"
  | "spawn"
  | "detach"
  | "wait"
  | "mut"
  | "chan"
  | "map"
  | "range"
  | "none"
  | "is"
  | "or"
  | "match"
  | "select"
  | "type"
  | "struct"
  | "import"
  | "export"
  | "true"
  | "false"
  | "break"
  | "continue"
  // 記号・演算子(トークン種別名 = 記号そのもの)
  | ":="
  | "=="
  | "!="
  | "<="
  | ">="
  | "&&"
  | "||"
  | "|"
  | "<-"
  | "++"
  | "--"
  | "+="
  | "-="
  | "*="
  | "/="
  | "%="
  | "=>"
  | "="
  | "<"
  | ">"
  | "+"
  | "-"
  | "*"
  | "/"
  | "%"
  | "!"
  | "?"
  | ","
  | ":"
  | ";"
  | "("
  | ")"
  | "{"
  | "}"
  | "["
  | "]"
  | "."
  | "eof";

// 文字列補間: "worker ${id} done" は
// [text "worker ", expr "id", text " done"] という部品リストに分解される
export type StringPart =
  | { kind: "text"; text: string }
  | { kind: "expr"; source: string; pos: Pos }; // 式は未パースのソース断片として持つ

export interface Token {
  type: TokenType;
  value: string;
  pos: Pos;
  parts?: StringPart[]; // 補間を含む文字列トークンだけが持つ
}

export const KEYWORDS = new Set<TokenType>([
  "fn",
  "return",
  "if",
  "else",
  "for",
  "spawn",
  "detach",
  "wait",
  "mut",
  "chan",
  "map",
  "range",
  "none",
  "is",
  "or",
  "match",
  "select",
  "type",
  "struct",
  "import",
  "export",
  "true",
  "false",
  "break",
  "continue",
]);

// コンパイルエラー(構文エラーなど)を位置情報つきで表す(F-13: code必須・fixは任意)
export class CompileError extends Error {
  constructor(
    message: string,
    public pos: Pos,
    public code: DiagnosticCode,
    public fix?: Fix,
  ) {
    super(message);
  }
}
