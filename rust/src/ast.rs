// AST(抽象構文木)= パースした結果の木構造。TS版(src/ast.ts、403行)からの移植だが、
// 今回のPRではparser.ts全体(1217行)は移さず、意味のある実用サブセットだけに絞っている。
//
// **今回のスコープ**: fn宣言(ジェネリクス・レシーバは次回)、トップレベル定数、
// if/else-ifチェーン、for(3形態)、break/continue、変数宣言・代入・複合代入・
// インクリメント、二項演算子(優先順位込み)、単項演算子、関数呼び出し。
// **対象外(次回以降のPRで追加)**: struct/type宣言、ジェネリクス、match/is/or、
// spawn/wait/chan/select、文字列補間、配列/mapリテラル、import/export、defer、
// 修飾フィールドアクセス(member/index)、範囲for、send文、型注釈つき変数宣言。
// これらを含む式・文に出会うと(対応するトークンを認識しないので)構文エラーとして
// 検出される — クラッシュはしない、「まだ対応していません」という誠実な失敗の仕方になる。
//
// 演算子(二項・単項・複合代入・インクリメント)は専用のenumを作らず`TokenType`を
// そのまま使っている。Plus/Minus/EqEq等は既にlexer側で列挙済みなので、
// 意味の重複するenumを増やさない判断

use crate::token::Pos;

// ---- 型の構文ノード(ソースに書かれた型注釈) ----
// TS版の8種のうち、今回は name と union の2つだけ(array/chan/mapType/structType/fnType/
// literalは次回以降)
#[derive(Debug, Clone, PartialEq)]
pub enum TypeNode {
    Name { name: String, pkg: Option<String>, pos: Pos }, // int, string, Status, math.User など
    Union { members: Vec<TypeNode>, pos: Pos },           // int | error
}

impl TypeNode {
    pub fn pos(&self) -> Pos {
        match self {
            TypeNode::Name { pos, .. } | TypeNode::Union { pos, .. } => *pos,
        }
    }
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
}

#[derive(Debug, Clone, PartialEq)]
pub struct ConstDecl {
    pub name: String,
    pub type_node: Option<TypeNode>, // 型注釈があれば(x: int = 10)。無ければ値から推論
    pub value: Expr,
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
pub enum Expr {
    Int { value: String, pos: Pos },
    Float { value: String, pos: Pos },
    String { value: String, pos: Pos }, // 補間(InterpExpr)は次回以降 — 再字句解析が絡み複雑なため
    Bool { value: bool, pos: Pos },
    Ident { name: String, pos: Pos },
    Binary { op: crate::token::TokenType, left: Box<Expr>, right: Box<Expr>, pos: Pos },
    Unary { op: crate::token::TokenType, operand: Box<Expr>, pos: Pos }, // ! または -
    Call { callee: Box<Expr>, args: Vec<Expr>, pos: Pos },
}

impl Expr {
    pub fn pos(&self) -> Pos {
        match self {
            Expr::Int { pos, .. }
            | Expr::Float { pos, .. }
            | Expr::String { pos, .. }
            | Expr::Bool { pos, .. }
            | Expr::Ident { pos, .. }
            | Expr::Binary { pos, .. }
            | Expr::Unary { pos, .. }
            | Expr::Call { pos, .. } => *pos,
        }
    }
}
