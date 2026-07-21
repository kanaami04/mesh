// AST(抽象構文木)= パースした結果の木構造。TS版(src/ast.ts、403行)からの移植だが、
// 今回のPRではparser.ts全体(1217行)は移さず、意味のある実用サブセットだけに絞っている。
//
// **今回までのスコープ**: fn宣言(ジェネリクス・レシーバは次回)、トップレベル定数、
// if/else-ifチェーン、for(3形態)、break/continue、変数宣言・代入・複合代入・
// インクリメント、二項演算子(優先順位込み)、単項演算子、関数呼び出し、
// **struct/type宣言(判別可能union込み)・構造体リテラル・メンバーアクセス・is式・match式**。
// **対象外(次回以降のPRで追加)**: ジェネリクス、`or`束縛形・`?`伝播、
// spawn/wait/chan/select、文字列補間、配列/mapリテラル、import/export、defer、
// 添字アクセス、範囲for、send文、型注釈つき変数宣言、error/jsonマーカー
// (`?`/`or`が無いと構造化エラーの旨みが薄いため、それらとセットで後回し)。
// これらを含む式・文に出会うと(対応するトークンを認識しないので)構文エラーとして
// 検出される — クラッシュはしない、「まだ対応していません」という誠実な失敗の仕方になる。
//
// 演算子(二項・単項・複合代入・インクリメント)は専用のenumを作らず`TokenType`を
// そのまま使っている。Plus/Minus/EqEq等は既にlexer側で列挙済みなので、
// 意味の重複するenumを増やさない判断

use crate::token::Pos;

// ---- 型の構文ノード(ソースに書かれた型注釈) ----
// TS版の8種のうち、今回はname/union/structType/literalの4つ(array/chan/mapType/fnTypeは
// 次回以降)。inline structType(判別可能unionのメンバー)もこの一種として表す。
// literalは判別可能unionのタグ(`{ kind: "ok" }`の"ok"部分)に必須なため、
// struct/union対応と同時に追加した(当初の見積もりで見落としていた)
#[derive(Debug, Clone, PartialEq)]
pub enum TypeNode {
    Name { name: String, pkg: Option<String>, pos: Pos }, // int, string, Status, math.User など
    Literal { value: String, pos: Pos },                  // "active" — 文字列リテラル型
    Union { members: Vec<TypeNode>, pos: Pos },           // int | error
    StructType { fields: Vec<StructFieldNode>, pos: Pos }, // struct宣言の中身 / union内の無名{...}
}

impl TypeNode {
    pub fn pos(&self) -> Pos {
        match self {
            TypeNode::Name { pos, .. }
            | TypeNode::Literal { pos, .. }
            | TypeNode::Union { pos, .. }
            | TypeNode::StructType { pos, .. } => *pos,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct StructFieldNode {
    pub name: String,
    pub type_node: TypeNode,
    pub pos: Pos,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Param {
    pub name: String,
    pub type_node: TypeNode,
    pub pos: Pos,
}

// ---- 宣言 ----
#[derive(Debug, Clone, PartialEq)]
pub struct Program {
    pub fns: Vec<FnDecl>,
    pub consts: Vec<ConstDecl>,
    pub types: Vec<TypeDecl>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ConstDecl {
    pub name: String,
    pub type_node: Option<TypeNode>, // 型注釈があれば(x: int = 10)。無ければ値から推論
    pub value: Expr,
    pub exported: bool,
    pub pos: Pos,
}

// struct X { ... } / type X = ...。error/jsonマーカー(isError/isJson)は次回以降
// (`?`/`or`が実装されてから意味を持つため)
#[derive(Debug, Clone, PartialEq)]
pub struct TypeDecl {
    pub name: String,
    pub node: TypeNode,
    pub exported: bool,
    pub pos: Pos,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FnDecl {
    pub name: String,
    pub params: Vec<Param>,
    pub ret: Option<TypeNode>, // 戻り値なし = None
    pub body: Block,
    pub exported: bool,
    pub pos: Pos,
}

// ---- 文 ----
#[derive(Debug, Clone, PartialEq)]
pub struct Block {
    pub stmts: Vec<Stmt>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Stmt {
    ShortVarDecl { names: Vec<String>, values: Vec<Expr>, mutable: bool, pos: Pos },
    Assign { targets: Vec<Expr>, values: Vec<Expr>, compound_op: Option<crate::token::TokenType>, pos: Pos },
    ExprStmt { expr: Expr, pos: Pos },
    Return { value: Option<Expr>, pos: Pos },
    If(IfStmt),
    For { init: Option<Box<Stmt>>, cond: Option<Expr>, post: Option<Box<Stmt>>, body: Block, pos: Pos },
    IncDec { target: Expr, op: crate::token::TokenType, pos: Pos }, // PlusPlus / MinusMinus
    Break { pos: Pos },
    Continue { pos: Pos },
}

#[derive(Debug, Clone, PartialEq)]
pub struct IfStmt {
    pub cond: Expr,
    pub then: Block,
    pub else_: Option<Box<ElseClause>>,
    pub pos: Pos,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ElseClause {
    If(IfStmt),
    Block(Block),
}

// ---- 式 ----
#[derive(Debug, Clone, PartialEq)]
pub struct StructLitField {
    pub name: String,
    pub value: Expr,
    pub pos: Pos,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MatchArm {
    pub patterns: Vec<MatchPattern>, // カンマ区切りで複数可
    pub body: Expr,                  // v1は単一式のみ(ブロックアームは将来)
    pub pos: Pos,
}

#[derive(Debug, Clone, PartialEq)]
pub enum MatchPattern {
    Type(TypeNode),  // none / error / int / { kind: "ok" } などの型パターン
    Wildcard { pos: Pos }, // _ (最後のアームのみ。checker側で検証)
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Int { value: String, pos: Pos },
    Float { value: String, pos: Pos },
    String { value: String, pos: Pos }, // 補間(InterpExpr)は次回以降 — 再字句解析が絡み複雑なため
    Bool { value: bool, pos: Pos },
    None { pos: Pos }, // 不在の値。T | none の union にだけ入れられる(checker移植後に検査)
    Ident { name: String, pos: Pos },
    Binary { op: crate::token::TokenType, left: Box<Expr>, right: Box<Expr>, pos: Pos },
    Unary { op: crate::token::TokenType, operand: Box<Expr>, pos: Pos }, // ! または -
    Call { callee: Box<Expr>, args: Vec<Expr>, pos: Pos },
    Member { target: Box<Expr>, name: String, pos: Pos }, // obj.name
    StructLit { name: String, fields: Vec<StructLitField>, pos: Pos }, // User{name: "a"}(pkg修飾は次回)
    Is { operand: Box<Expr>, target: TypeNode, pos: Pos }, // x is none / x is { kind: "ok" }
    Match { subject: Box<Expr>, arms: Vec<MatchArm>, pos: Pos },
}

impl Expr {
    pub fn pos(&self) -> Pos {
        match self {
            Expr::Int { pos, .. }
            | Expr::Float { pos, .. }
            | Expr::String { pos, .. }
            | Expr::Bool { pos, .. }
            | Expr::None { pos, .. }
            | Expr::Ident { pos, .. }
            | Expr::Binary { pos, .. }
            | Expr::Unary { pos, .. }
            | Expr::Call { pos, .. }
            | Expr::Member { pos, .. }
            | Expr::StructLit { pos, .. }
            | Expr::Is { pos, .. }
            | Expr::Match { pos, .. } => *pos,
        }
    }
}
