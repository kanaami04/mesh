// フルchecker(milestone 22・第一歩): 診断(位置+コード+メッセージ)を積むchecker。
// 既存のchecker.rs(最小リゾルバ)とは別物——あちらは診断を出さずcodegenの型解決だけを
// 担う「フェーズ2」としてそのまま残し、このフルcheckerはcodegenの前段に置く新設の
// 「フェーズ1」という位置づけ(docs/handoff.md「次のフェーズ: フルchecker移植」節で
// 合意した設計)。TS版`src/checker/`のcontext.ts(スコープ/宣言の基盤)+
// expressions.ts(識別子解決)+statements.ts(変数宣言/代入検査)のごく一部に相当する。
//
// **スコープ(意図的な段階的拡張)**: 元のcodegen移植のmilestone 1と同じ「スカラーの
// Mesh」(struct/map/配列/channel/並行処理/import/ジェネリクスは対象外)。診断コードも
// 「変数宣言・型不一致・未定義名」のごく少数(diagnostic_codes.rs参照)だけを実装する。
// milestone 23でトップレベル宣言(fn/const)自体の名前衝突検査(下記`check_program`参照)、
// milestone 24でmain関数の形検査(missing-main/invalid-main-signature)、
// milestone 25で演算子の妥当性検査(invalid-operation/incomparable-types/not-bool/
// use-is-none/division-by-zero——下記`infer_binary`/`check_arith`/`infer_unary`参照)、
// milestone 26でユーザー定義関数呼び出しの引数個数・型検査(argument-count——下記
// `fn_signature`/`Expr::Call`参照)、milestone 27で組み込み関数の個数・スカラー引数型検査
// (argument-count/builtin-arg-type——下記`infer_builtin_call`参照)、milestone 29で
// 名前付きstructのモデル化+structリテラルのフィールド検証(unknown-field/missing-fields/
// duplicate-field/型不一致——型「解決」は既存`checker.rs`を再利用、診断「発行」だけ再実装。
// 下記`type_ctx`/`resolve_type_ann`/`infer_struct_lit`参照)、milestone 30で
// structのフィールドアクセス読み取り(`u.name`→型/unknown-field/method-not-called、
// 下記`infer_member`参照)+メソッド解決(method_table構築・`u.method()`呼び出しの解決、
// Callアーム参照)を追加。milestone 31でstructメソッドを卒業——メソッド呼び出しの
// 引数個数・型照合(下記`check_args_against`)+**メソッド本体の検査**(milestone 30まで
// `check_program`の最終ループがレシーバ付き関数を丸ごとスキップしていた穴。`check_fn`が
// レシーバをスコープへ宣言するのとセット)+宣言時の4診断(下記`declare_method`)。
// milestone 32で判別可能unionの構築検証(union名で書いたstructリテラルのメンバー特定+
// フィールド検証。下記`resolve_union_lit_member`)。**milestone 33で配列をモデル化**——
// milestone 27以来の「コレクションは常にANY」というinvariantを配列に限って外し、
// 配列リテラル・添字・要素代入・range-for・配列を取る組み込み(要素型/コールバック署名/
// 戻り値型)を検査する(下記`resolve_type_ann`のArrayアーム・`is_fully_modeled`・
// `require_array`参照)。
// **milestone 34でmap/channelもモデル化**——mapリテラル/読み書き(`V | none`)/
// compound-assign-on-map/keys/values/delete、channel生成/送受信(`T | closed`)/close/
// not-a-channel、range-forのmap対応。あわせて**`is`によるnarrowing**(if文のthen/else/
// フォールスルー。下記`check_if`)と`or`式の走査を追加した——これが無いと、unionを返すように
// なった読みが`if v is closed { break }`の後で誤って弾かれる(誤検知)。
// **これでコレクション(配列/map/channel)はひととおりモデル化できた**。
// 残る縮退はunion型注釈・関数型注釈(`fn(...)`全体)・pkg修飾型・型パラメータ。
// pkg修飾struct・非structへのメンバーアクセス(not-a-struct)・match式の中身・
// `or`の4診断・値位置のvoid-used-as-value・run/buildへのゲート統合は引き続き対象外
// ——アーキテクチャが正しいと分かった時点で、機能ごとに広げていく方針(既存21マイルストーンと
// 同じ進め方)。

use crate::ast::{Block, ConstDecl, ElseClause, Expr, FnDecl, IfStmt, InterpSegment, MatchArm, MatchPattern, Program, SelectArm, Stmt, StructLitField, TypeNode};
use crate::diagnostic_codes::{Diagnostic, DiagnosticCode};
use crate::token::{Pos, TokenType};
use crate::types::{self, ANY, BOOL, ERROR, FLOAT, INT, NONE, STRING, VOID, Type};
use std::cell::OnceCell;
use std::collections::HashMap;
use std::rc::Rc;

// JS化したときに意味を持ってしまう名前(TS版`checker/context.ts`のRESERVEDをそのまま移植)
const RESERVED: &[&str] = &[
    "await", "async", "function", "const", "let", "var", "class", "new", "this", "typeof", "instanceof", "in", "of", "yield", "delete", "void", "switch", "case", "default", "do", "while",
    "with", "export", "import", "extends", "super", "null", "undefined", "try", "catch", "finally", "throw", "eval", "arguments",
];

struct Binding {
    ty: Type,
    mutable: bool,
}

pub struct FullCheckerCtx {
    // scopes[0]がトップレベル(fn/const名の置き場)、それ以降が通常の関数ローカルスコープ。
    // TS版`checker/modules.ts`のcheckPackageもトップレベルのfn/constを普通の
    // declareBindingでscopes[0]へ登録しており(milestone 22時点では別集合
    // `top_level_names`で代用していたが、milestone 23でこの統一設計へ寄せた)、
    // ローカル変数と全く同じdeclare()を通ることで予約語・組み込み名衝突・重複宣言・
    // shadowingの検査が自動的に効くようになる
    scopes: Vec<HashMap<String, Binding>>,
    // 今検査中の関数の戻り値型のスタック(無名関数はmilestone 22の対象外なので実際には
    // 深さ1までしか積まれないが、TS版`ctx.retStack`と同じ形にしておく)
    ret_stack: Vec<Type>,
    diagnostics: Vec<Diagnostic>,
    // milestone 29: struct/union型のレジストリ。型「解決」は既存の`checker.rs`
    // (最小リゾルバ、codegen依存ゼロ)を**再利用**する——`resolve_type_decls`が
    // knot-tying(自己参照・循環対応、milestone 19の資産)でstruct_types/union_typesを
    // 構築する。診断の「発行」だけをfull_checker側で書く(checker.rsのvalidateは
    // Result即失敗で形が違うため、TS版`expressions.ts`/`calls.ts`を手本に診断蓄積形で
    // 書き直す)。詳細はtodo.md/handoff.md milestone 29参照
    type_ctx: crate::checker::CheckerCtx,
}

impl FullCheckerCtx {
    fn new() -> Self {
        FullCheckerCtx {
            scopes: vec![HashMap::new()],
            ret_stack: Vec::new(),
            diagnostics: Vec::new(),
            type_ctx: crate::checker::CheckerCtx::new(),
        }
    }

    fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    fn pop_scope(&mut self) {
        self.scopes.pop();
    }

    fn error(&mut self, pos: Pos, code: DiagnosticCode, message: impl Into<String>) {
        self.diagnostics.push(Diagnostic { pos, code, message: message.into() });
    }

    fn lookup(&self, name: &str) -> Option<&Binding> {
        self.scopes.iter().rev().find_map(|s| s.get(name))
    }

    // TS版`checker/context.ts`の`declareBinding`相当。ブランク識別子("_")は宣言しない
    fn declare(&mut self, name: &str, ty: Type, pos: Pos, mutable: bool) {
        if name == "_" {
            return;
        }
        if RESERVED.contains(&name) {
            self.error(pos, DiagnosticCode::ReservedWord, format!("'{name}' is a reserved word and cannot be used as a name"));
            return;
        }
        if crate::checker::is_builtin(name) {
            self.error(pos, DiagnosticCode::BuiltinRedeclared, format!("'{name}' is a builtin function and cannot be redeclared"));
            return;
        }
        if self.scopes.last().expect("scopes is never empty").contains_key(name) {
            self.error(pos, DiagnosticCode::AlreadyDeclared, format!("'{name}' is already declared in this scope"));
            return;
        }
        // シャドーイング禁止(TS版と同じ2026-07-17決定): 外側スコープ(トップレベルの
        // fn/const名を含む——TS版コメント「外側スコープ(関数名を含む)」と同じ扱い。
        // トップレベル名もscopes[0]にいるのでlookup()が自然に見つける)に同名があれば、
        // 更新し忘れて`:=`してしまっただけの疑いが強いバグとして拒否する
        if self.lookup(name).is_some() {
            self.error(pos, DiagnosticCode::Shadowing, format!("'{name}' shadows an outer binding — use '=' to update it, or pick a different name"));
            return;
        }
        self.scopes.last_mut().expect("scopes is never empty").insert(name.to_string(), Binding { ty, mutable });
    }

    // milestone 34: 絞り込み(narrowing)用に、既存の束縛を**同じ可変性のまま**現在のスコープへ
    // 上書き登録する。`declare`と違い予約語/重複/シャドーイングの検査は通さない——これは
    // 新しい宣言ではなく「同じ変数について今わかっている型」を差し替える操作なので、
    // 検査を通すと`shadowing`の誤検知になる(codegen側milestone 7のnarrowingと同じ考え方)
    fn narrow(&mut self, name: &str, ty: Type) {
        let mutable = self.lookup(name).map(|b| b.mutable).unwrap_or(false);
        self.scopes.last_mut().expect("scopes is never empty").insert(name.to_string(), Binding { ty, mutable });
    }
}

// 型注釈を解決する。スカラー(int/float/...)+ milestone 29から**名前付きstruct**
// (type_ctxのレジストリ経由)を解決する。**配列/map/channel/union/関数型/pkg修飾型は
// 引き続きANY**(milestone 33で配列、milestone 34でmap/channelがこの縮退から卒業した
// ——下記Array/MapType/Chanアーム。残るのはunion/関数型/pkg修飾型/型パラメータ)。
// 縮退させた型はレジストリ側の完全解決された型と突き合わせてはいけない(`is_fully_modeled`
// 参照)。ANYフォールバックは診断を出さない(未対応構文を誤りとして報告しないため)。
fn resolve_type_ann(tc: &crate::checker::CheckerCtx, node: &TypeNode) -> Type {
    match node {
        TypeNode::Name { name, pkg: None, .. } => match name.as_str() {
            "int" => INT,
            "float" => FLOAT,
            "string" => STRING,
            "bool" => BOOL,
            "void" => VOID,
            "error" => ERROR,
            "none" => NONE,
            // 名前付きstruct、**milestone 39からは名前付きunionも**レジストリから解決する。
            // 未知/その他はANYのまま
            _ => match tc.lookup_struct(name) {
                Some(t) if matches!(t, Type::Struct { .. }) => t.clone(),
                // レジストリのunionはchecker.rsが完全解決したもの(knot-tying済み)。
                // ただし`is_fully_modeled`はunionを引き続き「モデル化できていない」扱いに
                // するので、レジストリ側の宣言型との突き合わせ(structリテラルのフィールド等)
                // には使われない——matchのパターン照合・narrowingにだけ効く
                // **中身が解決できていないunionはANYのまま**——`type_equals`/`union_of`/
                // `assignable`はいずれも「union bodyは解決済み」前提で`.expect()`するので、
                // 裸のunion循環などで空のまま残った登録を配るとパニックする
                // (自分で書いた回帰テストで実際に踏んだ。milestone 37の`mesh test`
                // クラッシュと同じ形)
                _ => match tc.lookup_union(name) {
                    Some(t) if safe_to_compare(t) => t.clone(),
                    _ => ANY,
                },
            },
        },
        // milestone 33で配列、**milestone 34でmap/channel**をANY縮退から卒業させた。
        // 要素/キー/値の型は再帰的に解決する——中身がunion等でANYへ潰れた場合
        // (`map<string, int|none>`等)は「中にANYを含む型」になるので、レジストリ側の
        // 完全解決された型と突き合わせてはいけない(下記`is_fully_modeled`が弾く)
        TypeNode::Array { elem, .. } => Type::Array(Box::new(resolve_type_ann(tc, elem))),
        TypeNode::MapType { key, value, .. } => Type::Map { key: Box::new(resolve_type_ann(tc, key)), value: Box::new(resolve_type_ann(tc, value)) },
        TypeNode::Chan { elem, .. } => Type::Chan(Box::new(resolve_type_ann(tc, elem))),
        // milestone 39: 書き下しのunion注釈(`int | error`)。メンバーを1つずつ
        // `resolve_type_ann`で解決してから組む——**メンバーが1つでもANYへ縮退したら
        // `union_of`がANYを返す**(ANYの短絡)ので、「中身を完全に解決できたunionだけが
        // unionとして残る」という安全な性質が自動的に得られる。
        // matchのsubjectがunionになって初めて、パターン系の診断(impossible-pattern・
        // unreachable-pattern・match-not-exhaustive)が実際に効くようになる
        TypeNode::Union { members, .. } => {
            let ms: Vec<Type> = members.iter().map(|m| resolve_type_ann(tc, m)).collect();
            // 未解決のunionが紛れていたら`union_of`自身がパニックするので、その手前で降りる
            if ms.iter().all(safe_to_compare) { types::union_of(ms) } else { ANY }
        }
        _ => ANY,
    }
}

// milestone 22のスコープ(スカラーのMesh)に含まれる式だけ型を推論しつつ未定義名を検査する。
// スコープ外の式(struct/array/map/channel/spawn/match/is/...)は中まで再帰せずANYへ
// フォールバックする——それらの内部にある未定義名の見落としは、対応する構文が
// milestone 22の対象に入るタイミングで解消する
fn infer_expr(ctx: &mut FullCheckerCtx, expr: &Expr) -> Type {
    match expr {
        Expr::Int { .. } => INT,
        Expr::Float { .. } => FLOAT,
        // 文字列リテラルは(STRINGではなく)リテラル型。TS版`checkExprSingle`・
        // codegen側checker.rsの`infer_expr`と同じ——`1 < "a"`の診断で相手の型が
        // `string`ではなく`"a"`と表示される等、TS版とのメッセージ一致に効く。
        // mut宣言時の型はShortVarDeclでwiden_literalする(TS版statements.tsと同じ)
        Expr::String { value, .. } => Type::Literal(value.clone()),
        Expr::Bool { .. } => BOOL,
        Expr::None { .. } => NONE,
        Expr::Interp { segments, .. } => {
            for seg in segments {
                if let InterpSegment::Expr { expr } = seg {
                    infer_expr(ctx, expr);
                }
            }
            STRING
        }
        Expr::Ident { name, pos } => {
            if let Some(b) = ctx.lookup(name) {
                // トップレベルのfn/const名もscopes[0]にBindingとして入っている
                // (check_programが本体検査より前に登録する)ので、ここは組み込み関数と
                // 通常のローカル変数/トップレベル名を区別せず同じ経路で解決できる
                b.ty.clone()
            } else if crate::checker::is_builtin(name) {
                ANY
            } else {
                // 文言はTS版`expressions.ts:253`と同じ`undefined: 'x'`(milestone 22で
                // `'x' is not defined`という独自文言になっていたのをmilestone 31で揃えた——
                // メソッド本体の検査が入りこの診断が新しい場所で出るようになったため発覚)
                ctx.error(*pos, DiagnosticCode::UndefinedName, format!("undefined: '{name}'"));
                ANY
            }
        }
        Expr::Binary { op, left, right, pos } => infer_binary(ctx, *op, left, right, *pos),
        Expr::Unary { op, operand, pos } => infer_unary(ctx, *op, operand, *pos),
        Expr::Call { callee, args, pos, .. } => {
            // milestone 27: 組み込み関数呼び出し(`print`/`len`/...)を先にintercept。
            // 名前が組み込みならユーザーはそれをshadowできない(declare()が拒否する)ため、
            // 裸のIdentが組み込み名と一致すれば必ず組み込み呼び出し
            if let Expr::Ident { name, .. } = &**callee
                && crate::checker::is_builtin(name)
            {
                return infer_builtin_call(ctx, name, args, *pos);
            }
            // milestone 30: メソッド呼び出し(`u.method(args)`)。calleeがMemberで
            // targetがstruct、nameがフィールドでなければメソッドとして解決する。ここで
            // interceptしないと、bareのフィールドアクセス(infer_member)がメソッド名を
            // method-not-calledと誤判定してしまう(呼び出しているのに)。TS版`inferCall`の
            // recv.method分岐に対応。milestone 31で引数の個数・型照合も行うようになった
            // (解決できなかった場合も引数式は未定義名検出のため必ず推論する)
            // **重要**: Member calleeは必ずここで完結させる(fall throughして下の
            // `infer_expr(callee)`へ落とすと、targetが二重評価され`undef.foo()`のような場合に
            // undefined-nameが2回出る。TS版`calls.ts:78-80`が「targetは一度だけ評価する」と
            // 明記している不変条件。code reviewで発覚し再現確認済み)
            if let Expr::Member { target, name, pos: mpos } = &**callee {
                let tgt = infer_expr(ctx, target);
                if let Type::Struct { name: sname, fields: fcell, .. } = &tgt {
                    let field_names: Vec<String> = fcell.get().map(|fs| fs.iter().map(|f| f.name.clone()).collect()).unwrap_or_default();
                    if !field_names.iter().any(|n| n == name) {
                        // フィールドではない——メソッドとして解決を試みる(所有型へcloneして
                        // type_ctxのborrowを閉じてから引数を推論する)
                        let method_ty = ctx.type_ctx.lookup_method(sname, name).cloned();
                        let arg_tys: Vec<Type> = args.iter().map(|a| infer_expr(ctx, a)).collect();
                        return match method_ty {
                            // milestone 31: 引数の個数・型照合。メソッド型の`params[0]`は
                            // レシーバなので落として照合する(TS版`inferCall`の
                            // `paramsWithoutReceiver`と同じ)
                            Some(Type::Fn { params, ret }) => {
                                check_args_against(ctx, *pos, args, &arg_tys, params.get(1..).unwrap_or(&[]));
                                widen_field_type(*ret)
                            }
                            _ => {
                                // フィールドでもメソッドでもない——unknown-field(メソッド呼び出し文言)
                                let fnames = if field_names.is_empty() { "none".to_string() } else { field_names.join(", ") };
                                ctx.error(*mpos, DiagnosticCode::UnknownField, format!("{} has no field or method '{name}' (fields: {fnames})", types::type_to_string(&tgt)));
                                ANY
                            }
                        };
                    }
                    // フィールド名の呼び出し(関数値フィールド等)——フィールド型はwidenでANYに
                    // なるので引数照合は効かない(値呼び出しの照合はdefer)。下の共通処理で完結
                }
                // struct以外/フィールド呼び出し: 引数だけ推論してANY(targetは上で評価済み——
                // 二重評価を避けるためここで完結させ、generic経路へは落とさない)
                for a in args {
                    infer_expr(ctx, a);
                }
                return ANY;
            }
            let callee_ty = infer_expr(ctx, callee);
            let arg_tys: Vec<Type> = args.iter().map(|a| infer_expr(ctx, a)).collect();
            // milestone 26: calleeがユーザー定義関数(Type::Fn。check_programが
            // 非ジェネリック自由関数をシグネチャ付きで登録)なら個数・各引数の型を照合する
            // (TS版`checkArgsAgainst`)。ローカル変数に束縛した関数値の呼び出し
            // (`f := add; f(1,2)`)もcalleeがType::Fnとして伝播するので同じく照合される
            // (TS版`checkCallOfValue`も同じ経路)。pkg修飾呼び出し・メソッド呼び出し・
            // ジェネリック関数はcalleeがANYになるため対象外——従来どおりANYを返す
            if let Type::Fn { params, ret } = &callee_ty {
                let (params, ret) = (params.clone(), (**ret).clone());
                check_args_against(ctx, *pos, args, &arg_tys, &params);
                ret
            } else {
                ANY
            }
        }
        // milestone 29: 名前付きstructリテラルのフィールド検証。pkg修飾・判別可能union構築は
        // 次段階(下記infer_struct_litがANYを返す)
        Expr::StructLit { name, pkg, fields, pos, .. } => infer_struct_lit(ctx, name, pkg.as_deref(), fields, *pos),
        // milestone 30: structのフィールドアクセス読み取り(`u.name`)。field→型・
        // メソッド名→method-not-called・未知→unknown-field。呼び出し形(`u.method()`)は
        // Call側で先にinterceptするのでここには来ない(bareのメンバー参照のみ)
        Expr::Member { target, name, pos } => infer_member(ctx, target, name, *pos),
        // milestone 33: 配列リテラル(`[1, 2, 3]` / `int[]{}`)。TS版`expressions.ts`の
        // arrayLitケースの移植——要素型が明示されていればそれ、無ければ**先頭要素の型を
        // widenしたもの**(`["a","b"]`は`"a"[]`ではなく`string[]`)を要素型とし、
        // 残りの要素を照合する
        Expr::ArrayLit { elems, elem_type, .. } => {
            let declared = elem_type.as_ref().map(|node| resolve_type_ann(&ctx.type_ctx, node));
            let elem = match &declared {
                Some(t) => t.clone(),
                None => match elems.first() {
                    // 空配列は要素型不明(TS版と同じくANY要素の配列)。宣言に使われた場合の
                    // `cannot-infer-type`は`Stmt::ShortVarDecl`側で報告する
                    None => return Type::Array(Box::new(ANY)),
                    Some(first) => types::widen_literal(infer_expr(ctx, first)),
                },
            };
            // 明示型があれば先頭要素も照合対象(推論経由なら先頭は基準なのでスキップ)
            let rest = if declared.is_some() { &elems[..] } else { &elems[1..] };
            for e in rest {
                let t = infer_expr(ctx, e);
                // 要素型がANY縮退を含むなら突き合わせない(milestone 29/32と同じ配慮)
                if is_fully_modeled(&elem) && !types::assignable(&t, &elem) {
                    ctx.error(e.pos(), DiagnosticCode::TypeMismatch, format!("array element must be {}, got {}", types::type_to_string(&elem), types::type_to_string(&t)));
                }
            }
            Type::Array(Box::new(elem))
        }
        // 添字アクセス(`xs[0]` / `m["k"]` / `s[0]`)。TS版`expressions.ts`のindexケースの
        // 移植(milestone 33で配列・文字列、milestone 34でmap)。読み書きで共通の
        // `infer_index`へ委譲する
        Expr::Index { target, index, pos } => infer_index(ctx, target, index, *pos).read,
        // milestone 34: mapリテラル(`map<string, int>{"a": 1}`)。キー/値の型は注釈から
        // 解決し、各エントリを照合する(TS版`expressions.ts`のmapLitケース)
        Expr::MapLit { key, value, entries, .. } => {
            let key_ty = resolve_type_ann(&ctx.type_ctx, key);
            let value_ty = resolve_type_ann(&ctx.type_ctx, value);
            for e in entries {
                let kt = infer_expr(ctx, &e.key);
                let vt = infer_expr(ctx, &e.value);
                if is_fully_modeled(&key_ty) && !types::assignable(&kt, &key_ty) {
                    ctx.error(e.key.pos(), DiagnosticCode::TypeMismatch, format!("map key must be {}, got {}", types::type_to_string(&key_ty), types::type_to_string(&kt)));
                }
                if is_fully_modeled(&value_ty) && !types::assignable(&vt, &value_ty) {
                    ctx.error(e.value.pos(), DiagnosticCode::TypeMismatch, format!("map value must be {}, got {}", types::type_to_string(&value_ty), types::type_to_string(&vt)));
                }
            }
            Type::Map { key: Box::new(key_ty), value: Box::new(value_ty) }
        }
        // milestone 34: channel生成(`chan<int>(2)` / `chan<int>(none)`)。F-11: capacityは
        // 常に必須で、`none`以外はintでなければならない
        Expr::Chan { elem, capacity, .. } => {
            if !matches!(**capacity, Expr::None { .. }) {
                let cap = infer_expr(ctx, capacity);
                if !types::type_equals(&cap, &INT) && !matches!(cap, Type::Any) {
                    ctx.error(capacity.pos(), DiagnosticCode::TypeMismatch, format!("channel capacity must be int or none, got {}", types::type_to_string(&cap)));
                }
            }
            Type::Chan(Box::new(resolve_type_ann(&ctx.type_ctx, elem)))
        }
        // milestone 34: 受信(`<-ch`)は常に`T | closed`(mapの`V | none`と同じ理由——
        // closeされうることを型で強制する)
        Expr::Recv { channel, pos } => {
            let ch = infer_expr(ctx, channel);
            match &ch {
                Type::Chan(elem) => types::union_of(vec![(**elem).clone(), types::CLOSED]),
                Type::Any => ANY,
                other => {
                    ctx.error(*pos, DiagnosticCode::NotAChannel, format!("cannot receive from non-channel type {}", types::type_to_string(other)));
                    ANY
                }
            }
        }
        // milestone 34: `x is T`はbool。値の型は変えないが、if文の条件に使われたときは
        // `check_if`が絞り込みに使う(下記`check_if`+`FullCheckerCtx::narrow`)
        Expr::Is { operand, .. } => {
            infer_expr(ctx, operand);
            BOOL
        }
        // milestone 34: `or`式(`m["k"] or 0`)。map読みが`V | none`を返すようになり、
        // mapは**ほぼ必ず`or`と組で使われる**ため、中へ踏み込まないと添字のキー型検査が
        // まるごと素通りしてしまう(実装中にプローブで検出)。
        // **診断はまだ出さない**——`or-never-fails`/`or-requires-binding`/
        // `or-fallback-type-mismatch`/`or-no-success-value`のTS版4診断は別カテゴリの
        // 移植として次段階に回し、ここでは「両辺を推論して型を伝播させる」ことだけ行う。
        // 式の型は失敗メンバーを除いた「成功側」(TS版の`rest`)——`age := m["a"] or 0`が
        // intになり、後続の検査が効くようになる
        Expr::OrElse { left, right, binding, pos } => {
            let left_ty = infer_expr(ctx, left);
            // 束縛形(`or e => ...`)は失敗値をeに束縛して右辺を評価する。宣言しないと
            // 右辺のeがundefined-nameの誤検知になる
            ctx.push_scope();
            if let Some(name) = binding
                && name != "_"
            {
                ctx.declare(name, crate::checker::or_binding_type(&left_ty), *pos, false);
            }
            infer_expr(ctx, right);
            ctx.pop_scope();
            success_type_of(&left_ty)
        }
        // milestone 39: match式・select式。TS版`checker/match-select.ts`の移植。
        // **アーム本体を走査するようになった**のがこのmilestoneの本題——今まで
        // `_ => ANY`で素通りしていたため、matchのアームの中にある未定義名も型不一致も
        // まるごと検出漏れになっていた(Meshはunion路線なのでmatchの中に本文が集まる)
        Expr::Match { subject, arms, pos } => infer_match(ctx, subject, arms, *pos),
        Expr::Select { arms, default_arm, pos } => infer_select(ctx, arms, default_arm.as_deref(), *pos),
        // spawn/prop(`?`)/無名関数など: まだ対象外なので中へは踏み込まない
        _ => ANY,
    }
}

// struct-litのフィールド値の型検査を行ってよい宣言型か。full_checkerが縮退させる型
// (union/関数型/pkg修飾型/型パラメータ、およびそれらを内側に含むコレクション
// ——resolve_type_annがANY相当にする)は、
// レジストリ側(checker.rsが完全解決)の型と突き合わせると誤検知するので検査しない。
// スカラー・struct・literal・none/closed等はfull_checker側の値の型が信頼できるので検査する
fn is_checkable_field_type(t: &Type) -> bool {
    is_fully_modeled(t)
}

// full_checkerがその型を**完全に**モデル化できているか(内部のどこにもANY縮退が無いか)。
// レジストリ側(checker.rsが完全解決)の宣言型と、full_checker側の値の型を突き合わせて
// よいかの判定に使う——縮退した型を突き合わせると必ず不一致になり誤検知になる
// (milestone 29/32のcode reviewで2度踏んだ穴)。milestone 33で配列をモデル化したことで
// 「配列か否か」という粗い判定では足りなくなった(`int[]`は突き合わせてよいが、
// 要素がANYへ潰れる`map<string,int>[]`は駄目)ため、再帰的な判定に置き換えている
fn is_fully_modeled(t: &Type) -> bool {
    match t {
        Type::Any => false,
        // まだモデル化していない型(関数/union/型パラメータ)。resolve_type_annは
        // これらをANYへ縮退させるので、レジストリ側の型と突き合わせられない
        Type::Fn { .. } | Type::Union { .. } | Type::TypeParam(_) => false,
        // コレクションは中身まで解決できていれば突き合わせてよい(milestone 33: 配列、
        // milestone 34: map/channel)
        Type::Array(elem) | Type::Chan(elem) => is_fully_modeled(elem),
        Type::Map { key, value } => is_fully_modeled(key) && is_fully_modeled(value),
        _ => true,
    }
}

// structフィールドの型を読み取り側へ返す際、full_checkerが完全にはモデル化できない型
// (関数型/union、およびそれらを内側に含むコレクション)はANYへ畳む——`u.cbs`
// (`fn(..)[]`フィールド)をbuiltinへ渡しても誤検知しないため。スカラー・struct・literal・
// **中身まで解決できるコレクション**(milestone 33で配列、milestone 34でmap/channel)は
// そのまま返す(`u.cells[0]`や`u.counts["k"]`のようなネストしたアクセスに型が伝播する)
fn widen_field_type(t: Type) -> Type {
    if is_checkable_field_type(&t) { t } else { ANY }
}

// `or`式の結果型(TS版`expressions.ts`の`rest` = 失敗メンバーを除いたunion)。
// unionでなければそのまま(TS版はその場合`or-never-fails`を報告するが、その診断は次段階)
fn success_type_of(t: &Type) -> Type {
    let Type::Union { body } = t else { return t.clone() };
    let Some(ub) = body.get() else { return ANY };
    let rest: Vec<Type> = ub.members.iter().filter(|m| !crate::checker::is_failure_type(m)).cloned().collect();
    if rest.is_empty() { ANY } else { types::union_of(rest) }
}

// 添字アクセス(`xs[0]` / `m["k"]`)の解決結果。`read`は式としての型(mapは`V | none`)、
// `write`は代入先としての型(mapは`V`——TS版`statements.ts`が
// `container?.kind === "map"`のとき`expected`を`container.value`へ差し替えるのと同じ)。
// `container`はコンテナ自体の型(代入側がmapかどうかの判定に使う)
struct IndexInfo {
    read: Type,
    write: Type,
    container: Type,
}

// 添字アクセスの型解決(milestone 33で配列、milestone 34でmap)。TS版`expressions.ts`の
// indexケースの移植。**targetとindexはここで一度だけ評価する**——読み取りと代入で
// 同じ経路を通し、二重評価(undefined-nameが2回出る等)を避けるため
fn infer_index(ctx: &mut FullCheckerCtx, target: &Expr, index: &Expr, pos: Pos) -> IndexInfo {
    let tgt = infer_expr(ctx, target);
    let idx = infer_expr(ctx, index);
    // mapを先に分岐する(TS版と同じ順序)。読み取りは常に`V | none`——無いキーを
    // 無視できないというunion路線の帰結
    if let Type::Map { key, value } = &tgt {
        if is_fully_modeled(key) && !types::assignable(&idx, key) {
            ctx.error(index.pos(), DiagnosticCode::TypeMismatch, format!("map key must be {}, got {}", types::type_to_string(key), types::type_to_string(&idx)));
        }
        let value = (**value).clone();
        return IndexInfo { read: types::union_of(vec![value.clone(), NONE]), write: value, container: tgt };
    }
    // 添字の型検査は**コンテナが配列/文字列と分かっている場合だけ**行う
    // (ANYコンテナ——未対応の型やエラー回復——では検査しない)
    let container_known = matches!(tgt, Type::Array(_)) || types::is_stringy(&tgt);
    if container_known && !matches!(idx, Type::Any) && (!types::is_numeric(&idx) || types::type_equals(&idx, &FLOAT)) {
        ctx.error(index.pos(), DiagnosticCode::InvalidIndexType, format!("index must be int, got {}", types::type_to_string(&idx)));
    }
    let elem = match &tgt {
        Type::Array(elem) => (**elem).clone(),
        t if types::is_stringy(t) => STRING,
        Type::Any => ANY,
        other => {
            ctx.error(pos, DiagnosticCode::NotIndexable, format!("cannot index into {}", types::type_to_string(other)));
            ANY
        }
    };
    IndexInfo { read: elem.clone(), write: elem, container: tgt }
}

// 推論済みの引数型をパラメータ型と照合する共通部分(TS版`calls.ts`の`checkArgsAgainst`)。
// 自由関数/関数値の呼び出し(milestone 26)とメソッド呼び出し(milestone 31)が
// ここへ合流する——TS版と同じく、個数が違っても重なる範囲は型照合する
// (min(args, params))。paramがANY(union/コレクション/未対応型)なら常にassignableに
// なるので誤検知しない
fn check_args_against(ctx: &mut FullCheckerCtx, call_pos: Pos, args: &[Expr], arg_tys: &[Type], params: &[Type]) {
    if arg_tys.len() != params.len() {
        ctx.error(call_pos, DiagnosticCode::ArgumentCount, format!("expected {} argument(s), got {}", params.len(), arg_tys.len()));
    }
    for (i, (at, pt)) in arg_tys.iter().zip(params.iter()).enumerate() {
        if !types::assignable(at, pt) {
            ctx.error(args[i].pos(), DiagnosticCode::TypeMismatch, format!("argument {}: cannot use {} as {}", i + 1, types::type_to_string(at), types::type_to_string(pt)));
        }
    }
}

// structのフィールドアクセス読み取り(`u.name`)の型解決(milestone 30)。TS版
// `memberFieldType`(src/checker/calls.ts)のstruct分岐の移植。field→型・
// 宣言済みメソッド名→method-not-called・それ以外→unknown-field。target が struct 以外
// (union/scalar/any)はこの一歩ではANY(union の narrow-required・not-a-struct は次段階)
fn infer_member(ctx: &mut FullCheckerCtx, target: &Expr, name: &str, pos: Pos) -> Type {
    let tgt = infer_expr(ctx, target);
    let Type::Struct { name: sname, fields: fcell, .. } = &tgt else {
        return ANY;
    };
    let Some(fs) = fcell.get() else {
        return ANY;
    };
    if let Some(f) = fs.iter().find(|f| f.name == name) {
        return widen_field_type(f.type_.clone());
    }
    // フィールドではない——メソッドなら method-not-called、そうでなければ unknown-field
    if ctx.type_ctx.lookup_method(sname, name).is_some() {
        ctx.error(pos, DiagnosticCode::MethodNotCalled, format!("'{name}' is a method — call it like {name}(...)"));
        return ANY;
    }
    let field_names = fs.iter().map(|f| f.name.clone()).collect::<Vec<_>>().join(", ");
    ctx.error(pos, DiagnosticCode::UnknownField, format!("{} has no field '{name}' (fields: {field_names})", types::type_to_string(&tgt)));
    ANY
}

// structリテラル(`User{...}`・`Resp{kind: "ok", ...}`)のフィールド検証。TS版
// `expressions.ts`のstructLitケースの移植——診断を積んで継続する。フィールド値の型・重複・
// 未知・欠落を検査する。milestone 29で名前付きstruct、**milestone 32でunion名での構築**
// (判別可能unionのタグdisambiguation・名前付きstruct同士のunionのフィールド集合解決。
// 下記`resolve_union_lit_member`)に対応した。pkg修飾structは次段階のためANYを返す。
fn infer_struct_lit(ctx: &mut FullCheckerCtx, name: &str, pkg: Option<&str>, fields: &[StructLitField], pos: Pos) -> Type {
    // 値は先に全て推論する(未定義名検出のため。arityや対象外でも必ず訪れる)
    let field_types: Vec<Type> = fields.iter().map(|f| infer_expr(ctx, &f.value)).collect();
    // pkg修飾はmilestone 29対象外
    if pkg.is_some() {
        return ANY;
    }
    // 名前がregistryのstructなら従来どおり(milestone 29)。structでなければunion
    // (判別可能union構築、milestone 32)を試し、どちらでもなければ対象外——ANY
    // `member_ty`はフィールド検証に使う具体的なstruct、`result_ty`は式全体の型
    // (union構築なら**絞り込んだメンバーではなくunion自身**——TS版と同じ。match/isで
    // 絞り込むまで常にunionとして扱うことで、mut再代入やwideningを新規に考えずに済む)、
    // `display_name`は診断に出す名前(union構築なら書かれたunion名。メンバーは無名なので)
    let (member_ty, result_ty, display_name) = match ctx.type_ctx.lookup_struct(name) {
        Some(t @ Type::Struct { name: sname, .. }) => (t.clone(), t.clone(), sname.clone()),
        _ => match ctx.type_ctx.lookup_union(name).cloned() {
            Some(union_ty) => match resolve_union_lit_member(ctx, &union_ty, name, fields, &field_types, pos) {
                Some(member) => (member, union_ty, name.to_string()),
                None => return ANY, // 絞り込み失敗(診断は出し済み)or struct memberが無いunion
            },
            None => return ANY,
        },
    };
    let Type::Struct { fields: decl_cell, .. } = &member_ty else {
        return ANY;
    };
    let Some(decl_fields) = decl_cell.get() else {
        return ANY; // 未解決(循環等でセットされていない)——素通り
    };
    // TS版structLitのforEach(重複→duplicate-field・未知→unknown-field・型不一致→
    // type-mismatch)。いずれも診断を積んで次のフィールドへ継続する
    let mut seen: std::collections::HashSet<&str> = std::collections::HashSet::new();
    for (f, ty) in fields.iter().zip(&field_types) {
        if !seen.insert(f.name.as_str()) {
            ctx.error(f.pos, DiagnosticCode::DuplicateField, format!("duplicate field '{}'", f.name));
            continue;
        }
        match decl_fields.iter().find(|d| d.name == f.name) {
            None => {
                let field_names = decl_fields.iter().map(|d| d.name.clone()).collect::<Vec<_>>().join(", ");
                ctx.error(f.pos, DiagnosticCode::UnknownField, format!("{display_name} has no field '{}' (fields: {field_names})", f.name));
            }
            Some(decl) => {
                // フィールド値の型検査は、full_checkerが値側の型を確実にモデル化できる型
                // (スカラー・struct・literal等)のフィールドだけを対象にする。宣言側の型が
                // 関数型/union等(レジストリではchecker.rsが完全解決する)の
                // フィールドは、full_checker側の値の型がANYや縮退したType::Fn(fn_signatureは
                // パラメータをANYにする)になり、完全解決された宣言型と突き合わせると誤検知する
                // (例: `cb: fn(int[]) int`へ名前付き関数を代入——fn_signatureのparam=ANY vs
                // レジストリのparam=int[]で不一致になる。code reviewで発覚)。それらの型の
                // フィールドはコレクション/関数型をモデル化する次段階まで検査をスキップ
                if is_checkable_field_type(&decl.type_) && !types::assignable(ty, &decl.type_) {
                    ctx.error(f.value.pos(), DiagnosticCode::TypeMismatch, format!("field '{}': cannot use {} as {}", f.name, types::type_to_string(ty), types::type_to_string(&decl.type_)));
                }
            }
        }
    }
    // 全フィールド必須(v1。ゼロ値・デフォルト無し)
    let missing: Vec<String> = decl_fields.iter().filter(|d| !seen.contains(d.name.as_str())).map(|d| d.name.clone()).collect();
    if !missing.is_empty() {
        ctx.error(pos, DiagnosticCode::MissingFields, format!("missing field(s) in {display_name}: {}", missing.join(", ")));
    }
    result_ty
}

// union名で書かれたstructリテラル(`GetUserResponse{kind: "ok", user: u}`)から、実際に
// 構築されるメンバーstructを特定する(milestone 32)。TS版`expressions.ts`のstructLitケースの
// union分岐の移植。codegen側の`checker::resolve_struct_lit_member`(milestone 12)と同じ
// 3分岐だが、あちらは`Result`で即失敗する最小リゾルバなのに対し、こちらは診断コード付きで
// 積んで継続する(milestone 29で決めたサブフォーク: 型「解決」はchecker.rsを再利用、
// 診断「発行」だけfull_checkerで再実装)。
// 特定できなければ`None`(診断は必要に応じて出し済み。呼び出し元はANYへフォールバックする)。
fn resolve_union_lit_member(ctx: &mut FullCheckerCtx, union_ty: &Type, name: &str, fields: &[StructLitField], field_types: &[Type], pos: Pos) -> Option<Type> {
    let Type::Union { body } = union_ty else { return None };
    // **bodyが未設定なら黙って諦める**: `checker.rs`の`resolve_named_type`はknot-tyingのため
    // 空のOnceCellを先に`declare_union`し、中身(`cell.set`)は解決が全部成功した後で入れる。
    // つまりタグ不足(`compute_discriminant_tag`のErr)等で解決に失敗したunionは
    // **レジストリには居るがbodyだけ空**という状態で残る——ここで拾えるのはそのケース。
    // TS版も宣言時に`discriminated-union-tag-required`で報告済みなので二重報告しない
    // (full_checkerは型宣言側の診断が未移植なので、実際には無診断=検出漏れになる。
    // なおresolve_type_declsは最初のErrで走査全体を打ち切るため、それ以降に宣言された型も
    // 巻き添えで未登録になる——check_program冒頭のコメントに記した既存の限界)
    let ub = body.get()?;
    let struct_members: Vec<&Type> = ub.members.iter().filter(|m| matches!(m, Type::Struct { .. })).collect();
    let anonymous_members: Vec<&Type> = struct_members.iter().filter(|m| matches!(m, Type::Struct { name, .. } if name == types::ANONYMOUS_STRUCT_NAME)).copied().collect();

    // (1) 無名`{...}`メンバーが2個以上 = F-7の判別可能union。**書かれたタグフィールドの
    // 値だけ**でメンバーを特定する(フィールド集合は一切見ない)
    if anonymous_members.len() >= 2 {
        // タグが無い(=判別可能unionとして成立していない)なら黙って諦める。上のbody未設定
        // ガードで大半は先に弾かれるが、TS版もここは「宣言時に報告済み」として二重報告を
        // 避けている(TS版コメント: "型宣言自体がタグ不足で既にエラー報告済み")
        let tag_name = ub.discriminant_tag.as_ref()?;
        let tag_value = fields
            .iter()
            .zip(field_types)
            .find(|(f, _)| &f.name == tag_name)
            .and_then(|(_, ty)| if let Type::Literal(v) = ty { Some(v.clone()) } else { None });
        let Some(tag_value) = tag_value else {
            ctx.error(
                pos,
                DiagnosticCode::DiscriminatedUnionTagMissing,
                format!("'{name}{{...}}' needs its tag field '{tag_name}' set to select a member (e.g. {name}{{ {tag_name}: \"...\", ... }})"),
            );
            return None;
        };
        let matched = anonymous_members.iter().find(|m| member_tag_value(m, tag_name).as_deref() == Some(tag_value.as_str()));
        return match matched {
            Some(m) => Some((*m).clone()),
            None => {
                let valid: Vec<String> = anonymous_members.iter().filter_map(|m| member_tag_value(m, tag_name).map(|v| format!("{v:?}"))).collect();
                ctx.error(
                    pos,
                    DiagnosticCode::DiscriminatedUnionNoMatch,
                    format!("no member of '{name}' has {tag_name}: {tag_value:?} (valid {tag_name} values: {})", valid.join(" | ")),
                );
                None
            }
        };
    }

    // (2) structメンバーが1個以下: そのメンバー(無ければANY相当=None。`type Status =
    // "active" | "banned"`のような非structのunionはTS版も無診断で素通りする)
    if struct_members.len() <= 1 {
        return struct_members.first().map(|m| (*m).clone());
    }

    // (3) 名前付きstruct同士のunion(`type Shape = Circle | Square`): フィールド集合で
    // 解決し、複数残ればフィールド値の型で絞る(TS版と同じ2段階)
    let mut written: Vec<&str> = Vec::new();
    for f in fields {
        if !written.contains(&f.name.as_str()) {
            written.push(&f.name);
        }
    }
    let mut candidates: Vec<&Type> = struct_members
        .iter()
        .filter(|m| {
            let Some(mf) = struct_fields_of(m) else { return false };
            let mut member_names: Vec<&str> = Vec::new();
            for f in mf {
                if !member_names.contains(&f.name.as_str()) {
                    member_names.push(&f.name);
                }
            }
            member_names.len() == written.len() && written.iter().all(|n| member_names.contains(n))
        })
        .copied()
        .collect();
    // 候補が複数ならフィールド値の型でさらに絞る(TS版の2段目)。ただし**縮退する型
    // (fn/union、およびそれらを内側に含むコレクション)の宣言フィールドは絞り込みの材料に
    // しない**——full_checker側の
    // 値の型はANY等へ潰れており、完全解決された宣言型と突き合わせると必ず不一致になるため、
    // そのまま使うと候補を誤って全滅させてしまう(code reviewで発覚・両CLIで再現確認:
    // `type AB = A | B`の両メンバーが`cb: fn(int[]) int`を持つとき、TS版の
    // discriminated-union-ambiguousがRust版ではno-matchになっていた)。milestone 29の
    // フィールド型検査と同じ`is_checkable_field_type`ガードを、この絞り込みにも適用する
    let mut undecidable = false;
    if candidates.len() > 1 {
        candidates.retain(|m| {
            let Some(mf) = struct_fields_of(m) else { return false };
            fields.iter().zip(field_types).all(|(f, ty)| match mf.iter().find(|d| d.name == f.name) {
                Some(d) if !is_checkable_field_type(&d.type_) => {
                    undecidable = true;
                    true
                }
                Some(d) => types::assignable(ty, &d.type_),
                None => false,
            })
        });
    }
    // 縮退フィールドを飛ばしたせいで絞り切れなかった場合は、TS版なら一意に決まる可能性が
    // 残っている(型で絞れば1個になる)ので、ambiguousと決めつけずに黙って諦める
    // ——full_checkerの既定方針どおり、誤検知ではなく検出漏れ側に倒す
    if candidates.len() > 1 && undecidable {
        return None;
    }
    match candidates.len() {
        1 => Some(candidates[0].clone()),
        0 => {
            let shapes: Vec<String> = struct_members
                .iter()
                .map(|m| format!("{{ {} }}", struct_fields_of(m).map(|mf| mf.iter().map(|f| f.name.clone()).collect::<Vec<_>>().join(", ")).unwrap_or_default()))
                .collect();
            ctx.error(
                pos,
                DiagnosticCode::DiscriminatedUnionNoMatch,
                format!("no member of '{name}' matches the field(s) {{{}}} (union members: {})", written.join(", "), shapes.join(" | ")),
            );
            None
        }
        _ => {
            ctx.error(
                pos,
                DiagnosticCode::DiscriminatedUnionAmbiguous,
                format!("ambiguous — multiple members of '{name}' match the field(s) {{{}}}", written.join(", ")),
            );
            None
        }
    }
}

// union メンバー(struct)の解決済みフィールド列。未解決(knot-tying中)ならNone
fn struct_fields_of(m: &Type) -> Option<&[types::StructField]> {
    let Type::Struct { fields, .. } = m else { return None };
    fields.get().map(|v| v.as_slice())
}

// union メンバーが持つタグフィールドのリテラル値(`{kind: "ok", ...}`なら"ok")
fn member_tag_value(m: &Type, tag_name: &str) -> Option<String> {
    let fields = struct_fields_of(m)?;
    fields.iter().find(|f| f.name == tag_name).and_then(|f| match &f.type_ {
        Type::Literal(v) => Some(v.clone()),
        _ => None,
    })
}

// 二項演算の妥当性検査+結果型の推論(milestone 25)。TS版`inferBinary`+`checkArithOp`
// (src/checker/expressions.ts)の移植。codegen側のmilestone 13/14で同じロジックを既に
// `checker.rs`へ移植済みだが、あちらは診断を出さず`Result<_, String>`で即失敗する設計
// (最小リゾルバ)。こちらは診断(コード+メッセージ+位置)を積んで走査を続ける。
// **narrowingは不要**: TS版は`&&`/`||`の右辺検査で左辺`x is T`の絞り込みを適用する
// (F-6)が、絞り込みの対象になるのはunion型だけで、full_checkerのスカラースコープでは
// union型は`resolve_scalar_type`でANYに潰れる——ANYはnot-bool/incomparable-types等の
// 全チェックの安全弁を素通りするので、絞り込みの有無で結果が変わらない
// (codegen milestone 14がスクラッチctxで対処した回帰は、そもそもここでは起きない)
fn infer_binary(ctx: &mut FullCheckerCtx, op: TokenType, left: &Expr, right: &Expr, pos: Pos) -> Type {
    // グロブ`use TokenType::*`は`TokenType::Type`(typeキーワード)を取り込んで
    // `Type`enumを隠すため、必要なバリアントだけ明示的にインポートする
    use TokenType::{AndAnd, EqEq, Ge, Gt, Le, Lt, NotEq, OrOr};
    // &&/||: 左右それぞれがbool(またはANY)であること。位置は全体ではなく各オペランド自身
    if op == AndAnd || op == OrOr {
        let lt = infer_expr(ctx, left);
        if !types::type_equals(&lt, &BOOL) && !matches!(lt, Type::Any) {
            ctx.error(left.pos(), DiagnosticCode::NotBool, format!("'{op}' requires bool operands, got {}", types::type_to_string(&lt)));
        }
        let rt = infer_expr(ctx, right);
        if !types::type_equals(&rt, &BOOL) && !matches!(rt, Type::Any) {
            ctx.error(right.pos(), DiagnosticCode::NotBool, format!("'{op}' requires bool operands, got {}", types::type_to_string(&rt)));
        }
        return BOOL;
    }

    let lt = infer_expr(ctx, left);
    let rt = infer_expr(ctx, right);

    // ==/!=: noneとの比較は narrowing の効く`is none`へ一本化する(P1)。それ以外は双方向assignable
    if op == EqEq || op == NotEq {
        if matches!(left, Expr::None { .. }) || matches!(right, Expr::None { .. }) {
            // TS版はfix(`==`→`is`)も付けるが、Diagnosticにfixフィールドがまだ無い
            // (milestone 22スコープ外)ため、コード・メッセージ・位置のみ一致させる
            ctx.error(pos, DiagnosticCode::UseIsNone, "use 'is none' to test for none (== does not narrow the type)");
            return BOOL;
        }
        if !types::assignable(&lt, &rt) && !types::assignable(&rt, &lt) {
            ctx.error(pos, DiagnosticCode::IncomparableTypes, format!("cannot compare {} with {}", types::type_to_string(&lt), types::type_to_string(&rt)));
        }
        return BOOL;
    }

    // < <= > >=: 両方numeric・両方stringy・どちらかANYのいずれか
    if matches!(op, Lt | Le | Gt | Ge) {
        let ok = (types::is_numeric(&lt) && types::is_numeric(&rt))
            || (types::is_stringy(&lt) && types::is_stringy(&rt))
            || matches!(lt, Type::Any)
            || matches!(rt, Type::Any);
        if !ok {
            ctx.error(pos, DiagnosticCode::IncomparableTypes, format!("cannot compare {} with {}", types::type_to_string(&lt), types::type_to_string(&rt)));
        }
        return BOOL;
    }

    // 算術演算(+ - * / %)
    check_arith(ctx, op, &lt, right, &rt, pos)
}

// 算術演算(+ - * / %)の妥当性検査+結果型。TS版`checkArithOp`の移植。
// right_exprはリテラル0除算検査に使う実AST(型だけでは "0" という値まで分からない)。
// int/floatの分類フラグ(intDiv/intMod/intArith)はcodegenの関心事なので、
// 診断だけを担うfull_checkerでは結果型のみ返す
fn check_arith(ctx: &mut FullCheckerCtx, op: TokenType, left: &Type, right_expr: &Expr, right: &Type, pos: Pos) -> Type {
    use TokenType::{Percent, Plus, Slash};
    if op == Plus && types::is_stringy(left) && types::is_stringy(right) {
        return STRING;
    }
    if types::is_numeric(left) && types::is_numeric(right) {
        let is_int = types::type_equals(left, &INT) && types::type_equals(right, &INT);
        // リテラルの0で割る/剰余するのは実行するまでもなくバグ。コンパイル時に弾く
        if is_int
            && matches!(op, Slash | Percent)
            && let Expr::Int { value, pos: rpos } = right_expr
            && value == "0"
        {
            let word = if op == Slash { "division" } else { "modulo" };
            ctx.error(*rpos, DiagnosticCode::DivisionByZero, format!("integer {word} by zero"));
        }
        // 「どちらかANY」の安全弁はis_numeric分岐の中と外の2箇所にある——is_numeric(ANY)は
        // 常にtrueなので、`ANY op 構造体`のような片方だけANYの組み合わせは最初の分岐
        // (両方numeric)を満たさず外側で拾われる(TS版checkArithOpと同じ2段構え)
        if matches!(left, Type::Any) || matches!(right, Type::Any) {
            return ANY;
        }
        return if is_int { INT } else { FLOAT };
    }
    if matches!(left, Type::Any) || matches!(right, Type::Any) {
        return ANY;
    }
    // 不正な演算(型不一致・未絞り込みのunion型への算術等)。TS版と同じく op=='+' で
    // 片側がstring本体のときだけ「str()で変換を」のヒントを添える(TS本体のこの診断で唯一のhint)
    let hint = if op == Plus && (types::type_equals(left, &STRING) || types::type_equals(right, &STRING)) {
        " (hint: use str() to convert values to string)"
    } else {
        ""
    };
    ctx.error(pos, DiagnosticCode::InvalidOperation, format!("invalid operation: {} {op} {}{hint}", types::type_to_string(left), types::type_to_string(right)));
    ANY
}

// 単項演算子(! / -)の妥当性検査(milestone 25)。TS版`case "unary"`の移植——
// `!`はnot-bool・単項`-`はinvalid-operationで、算術二項演算子と同じ`invalid-operation`を共有する
fn infer_unary(ctx: &mut FullCheckerCtx, op: TokenType, operand: &Expr, pos: Pos) -> Type {
    let t = infer_expr(ctx, operand);
    if op == TokenType::Bang {
        if !types::type_equals(&t, &BOOL) && !matches!(t, Type::Any) {
            ctx.error(pos, DiagnosticCode::NotBool, format!("'!' requires bool, got {}", types::type_to_string(&t)));
        }
        return BOOL;
    }
    if !types::is_numeric(&t) {
        ctx.error(pos, DiagnosticCode::InvalidOperation, format!("unary '-' requires int or float, got {}", types::type_to_string(&t)));
    }
    t
}

// arity照合(TS版`expectArity`)。個数が違えばargument-countを積んでfalseを返す。
// メッセージはTS版の組み込み用フォーマット(`name() expects N argument(s), got M`)
fn expect_arity(ctx: &mut FullCheckerCtx, name: &str, got: usize, want: usize, pos: Pos) -> bool {
    if got != want {
        ctx.error(pos, DiagnosticCode::ArgumentCount, format!("{name}() expects {want} argument(s), got {got}"));
        return false;
    }
    true
}

// milestone 34: mapを要求する組み込み(keys/values/delete)の共通処理。mapならキー/値の型を
// 返し(呼び出し側がキー型の検査や戻り値型に使う)、ANY(未対応の型・エラー回復)なら
// 黙って`None`、それ以外はTS版と同じ`builtin-arg-type`——配列の`require_array`と同じ形。
// **注**: 関数型はmilestone 26以降ANYではなくType::Fnとして追跡されるので、関数値を
// コレクション組み込みへ渡すと`... requires a map, got fn(...)`が出る(TS版と一致)
fn require_map(ctx: &mut FullCheckerCtx, arg_ty: &Type, arg_pos: Pos, msg: String) -> Option<(Type, Type)> {
    match arg_ty {
        Type::Map { key, value } => Some(((**key).clone(), (**value).clone())),
        Type::Any => None,
        _ => {
            ctx.error(arg_pos, DiagnosticCode::BuiltinArgType, msg);
            None
        }
    }
}

// milestone 33: 配列を要求する組み込みの共通処理。配列なら要素型を返し(呼び出し側が
// 要素型の検査に使う)、ANY(未モデル化のコレクション等)なら黙って`None`、それ以外は
// TS版と同じ`builtin-arg-type`。配列をモデル化したことで、TS版の`arr.kind === "array"`
// ガードがそのまま移植できるようになった(milestone 27では全てANYへ潰れて到達しなかった)
fn require_array(ctx: &mut FullCheckerCtx, arg_ty: &Type, arg_pos: Pos, msg: String) -> Option<Type> {
    match arg_ty {
        Type::Array(elem) => Some((**elem).clone()),
        Type::Any => None,
        _ => {
            ctx.error(arg_pos, DiagnosticCode::BuiltinArgType, msg);
            None
        }
    }
}

// 組み込み関数呼び出しの検査(milestone 27で導入、milestone 33で配列対応)。
// TS版`inferBuiltinCall`(src/checker/builtins.ts)の移植。
// **効く検査**: (1)全組み込みのarity(argument-count)、(2)引数の型検査(builtin-arg-type)——
// `len(5)`・`contains(5, x)`・`round(3)`のようにスカラーを誤って渡した場合、
// (3)戻り値型(str→string, len→int, sort→同じ配列型, map→callback戻り値の配列 等)、
// (4)**配列を取る組み込みの要素型検査**(`push`/`contains`/`indexOf`のtype-mismatch、
// `get`のinvalid-index-type、`sort`/`join`の要素型)と**高階組み込みのcallback署名検査**
// (`filter`/`map`/`reduce`のcallback-signature-mismatch)——milestone 33で配列をモデル化する
// までは「コレクションは常にANY」で到達しなかった経路(下記`require_array`参照)。
// milestone 34でmapを取る組み込み(keys/values/delete)も同じ形にした(`require_map`)。
// milestone 34以降、map/channelもモデル化されているので`keys`/`values`は`K[]`/`V[]`を返し、
// `delete`はキー型を、`close`はchannelであることを検査する。
// **関数型はANYではなくType::Fnとして追跡される**ので、コレクション組み込みへ関数値を
// 渡すと`push() requires an array, got fn(...)`が正しく出る(TS版と一致)。
fn infer_builtin_call(ctx: &mut FullCheckerCtx, name: &str, args: &[Expr], pos: Pos) -> Type {
    // 引数はarityに関わらず全て推論する(未定義名検査のため)
    let at: Vec<Type> = args.iter().map(|a| infer_expr(ctx, a)).collect();
    let n = at.len();
    let ts = |t: &Type| types::type_to_string(t);
    match name {
        "print" => VOID, // 可変長・任意型
        "str" => {
            expect_arity(ctx, name, n, 1, pos);
            STRING
        }
        "len" => {
            // 配列(milestone 33)とmap(milestone 34)を明示的に許す。channelは弾く(TS版と同じ)
            if expect_arity(ctx, name, n, 1, pos)
                && !types::is_stringy(&at[0])
                && !matches!(at[0], Type::Any | Type::Array(_) | Type::Map { .. })
            {
                ctx.error(args[0].pos(), DiagnosticCode::BuiltinArgType, format!("len() requires string, array or map, got {}", ts(&at[0])));
            }
            INT
        }
        "push" => {
            if expect_arity(ctx, name, n, 2, pos)
                && let Some(elem) = require_array(ctx, &at[0], args[0].pos(), format!("push() requires an array, got {}", ts(&at[0])))
                && is_fully_modeled(&elem)
                && !types::assignable(&at[1], &elem)
            {
                ctx.error(args[1].pos(), DiagnosticCode::TypeMismatch, format!("cannot push {} into {}", ts(&at[1]), ts(&at[0])));
            }
            VOID
        }
        "error" => {
            if expect_arity(ctx, name, n, 1, pos) && !types::assignable(&at[0], &STRING) {
                ctx.error(args[0].pos(), DiagnosticCode::BuiltinArgType, format!("error() requires a string message, got {}", ts(&at[0])));
            }
            ERROR
        }
        "delete" => {
            if expect_arity(ctx, name, n, 2, pos)
                && let Some((key, _)) = require_map(ctx, &at[0], args[0].pos(), format!("delete() requires a map, got {}", ts(&at[0])))
                && is_fully_modeled(&key)
                && !types::assignable(&at[1], &key)
            {
                ctx.error(args[1].pos(), DiagnosticCode::TypeMismatch, format!("map key must be {}, got {}", ts(&key), ts(&at[1])));
            }
            VOID
        }
        "sleep" => {
            if expect_arity(ctx, name, n, 1, pos) && !types::is_numeric(&at[0]) {
                ctx.error(args[0].pos(), DiagnosticCode::BuiltinArgType, format!("sleep() requires milliseconds (int), got {}", ts(&at[0])));
            }
            VOID
        }
        "contains" => {
            if expect_arity(ctx, name, n, 2, pos)
                && let Some(elem) = require_array(ctx, &at[0], args[0].pos(), format!("contains() requires an array, got {}", ts(&at[0])))
                && is_fully_modeled(&elem)
                && !types::assignable(&at[1], &elem)
            {
                ctx.error(args[1].pos(), DiagnosticCode::TypeMismatch, format!("contains() second argument must be {}, got {}", ts(&elem), ts(&at[1])));
            }
            BOOL
        }
        "indexOf" => {
            if expect_arity(ctx, name, n, 2, pos)
                && let Some(elem) = require_array(ctx, &at[0], args[0].pos(), format!("indexOf() requires an array, got {}", ts(&at[0])))
                && is_fully_modeled(&elem)
                && !types::assignable(&at[1], &elem)
            {
                ctx.error(args[1].pos(), DiagnosticCode::TypeMismatch, format!("indexOf() second argument must be {}, got {}", ts(&elem), ts(&at[1])));
            }
            types::union_of(vec![INT, NONE])
        }
        "get" => {
            // F-9d: 範囲外を型で強制する安全な読み(`elem | none`を返す)
            if expect_arity(ctx, name, n, 2, pos)
                && let Some(elem) = require_array(ctx, &at[0], args[0].pos(), format!("get() requires an array, got {}", ts(&at[0])))
            {
                if !types::type_equals(&at[1], &INT) && !matches!(at[1], Type::Any) {
                    ctx.error(args[1].pos(), DiagnosticCode::InvalidIndexType, format!("index must be int, got {}", ts(&at[1])));
                }
                return types::union_of(vec![elem, NONE]);
            }
            ANY
        }
        "keys" => {
            if expect_arity(ctx, name, n, 1, pos)
                && let Some((key, _)) = require_map(ctx, &at[0], args[0].pos(), format!("keys() requires a map, got {}", ts(&at[0])))
            {
                return Type::Array(Box::new(key));
            }
            Type::Array(Box::new(ANY))
        }
        "values" => {
            if expect_arity(ctx, name, n, 1, pos)
                && let Some((_, value)) = require_map(ctx, &at[0], args[0].pos(), format!("values() requires a map, got {}", ts(&at[0])))
            {
                return Type::Array(Box::new(value));
            }
            Type::Array(Box::new(ANY))
        }
        "sort" => {
            if expect_arity(ctx, name, n, 1, pos)
                && let Some(elem) = require_array(ctx, &at[0], args[0].pos(), format!("sort() requires an array, got {}", ts(&at[0])))
                && !types::is_numeric(&elem)
                && !types::is_stringy(&elem)
                && !matches!(elem, Type::Any)
            {
                ctx.error(args[0].pos(), DiagnosticCode::BuiltinArgType, format!("sort() requires int[], float[] or string[], got {}", ts(&at[0])));
            }
            // 非破壊(新しい配列を返す)。引数が配列ならその型をそのまま返す
            match at.first() {
                Some(t @ Type::Array(_)) => t.clone(),
                _ => ANY,
            }
        }
        "split" => {
            if expect_arity(ctx, name, n, 2, pos) {
                // is_stringyはANYを含まないので明示的にANYを免除(struct/メソッド由来の実stringが
                // full_checkerではANYに潰れるため——免除しないと誤検知。code reviewで発覚)
                if !types::is_stringy(&at[0]) && !matches!(at[0], Type::Any) {
                    ctx.error(args[0].pos(), DiagnosticCode::BuiltinArgType, format!("split() requires a string, got {}", ts(&at[0])));
                }
                if !types::is_stringy(&at[1]) && !matches!(at[1], Type::Any) {
                    ctx.error(args[1].pos(), DiagnosticCode::BuiltinArgType, format!("split() separator must be a string, got {}", ts(&at[1])));
                }
            }
            Type::Array(Box::new(STRING))
        }
        "join" => {
            if expect_arity(ctx, name, n, 2, pos) {
                if let Some(elem) = require_array(ctx, &at[0], args[0].pos(), format!("join() requires an array, got {}", ts(&at[0])))
                    && !types::is_stringy(&elem)
                    && !matches!(elem, Type::Any)
                {
                    ctx.error(args[0].pos(), DiagnosticCode::BuiltinArgType, format!("join() requires string[], got {}", ts(&at[0])));
                }
                if !types::is_stringy(&at[1]) && !matches!(at[1], Type::Any) {
                    ctx.error(args[1].pos(), DiagnosticCode::BuiltinArgType, format!("join() separator must be a string, got {}", ts(&at[1])));
                }
            }
            STRING
        }
        "trim" | "upper" | "lower" => {
            // is_stringyはANYを含まないので明示的にANYを免除する(len/round等と同じ)——
            // full_checkerではstruct/メソッド/pkg/ジェネリック由来の実stringもANYに潰れるため、
            // 免除しないと`trim(s.name)`(s.nameは実string)を誤検知する(code reviewで発覚)
            if expect_arity(ctx, name, n, 1, pos) && !types::is_stringy(&at[0]) && !matches!(at[0], Type::Any) {
                ctx.error(args[0].pos(), DiagnosticCode::BuiltinArgType, format!("{name}() requires a string, got {}", ts(&at[0])));
            }
            STRING
        }
        "toInt" => {
            if expect_arity(ctx, name, n, 1, pos) && !types::is_stringy(&at[0]) && !matches!(at[0], Type::Any) {
                ctx.error(args[0].pos(), DiagnosticCode::BuiltinArgType, format!("toInt() requires a string, got {}", ts(&at[0])));
            }
            ANY // 本来 int | error
        }
        "toFloat" => {
            if expect_arity(ctx, name, n, 1, pos) && !types::type_equals(&at[0], &INT) && !matches!(at[0], Type::Any) {
                ctx.error(args[0].pos(), DiagnosticCode::BuiltinArgType, format!("toFloat() requires an int, got {}", ts(&at[0])));
            }
            FLOAT
        }
        "round" | "floor" | "ceil" => {
            if expect_arity(ctx, name, n, 1, pos) && !types::type_equals(&at[0], &FLOAT) && !matches!(at[0], Type::Any) {
                ctx.error(args[0].pos(), DiagnosticCode::BuiltinArgType, format!("{name}() requires a float, got {}", ts(&at[0])));
            }
            INT
        }
        "close" => {
            // milestone 34: channelがモデル化されたので明示的に許す
            if expect_arity(ctx, name, n, 1, pos) && !matches!(at[0], Type::Any | Type::Chan(_)) {
                ctx.error(args[0].pos(), DiagnosticCode::BuiltinArgType, format!("close() requires a channel, got {}", ts(&at[0])));
            }
            VOID
        }
        // milestone 33: 高階関数はコールバックの署名も検査する(TS版のcallback-signature-mismatch)。
        // 配列がモデル化され、関数型はmilestone 26から`Type::Fn`で追跡されているので、
        // TS版のロジックがそのまま移植できるようになった
        "filter" => {
            if expect_arity(ctx, name, n, 2, pos)
                && let Some(elem) = require_array(ctx, &at[0], args[0].pos(), format!("filter() requires an array, got {}", ts(&at[0])))
            {
                match &at[1] {
                    Type::Fn { params, ret } => {
                        if params.len() != 1 || (is_fully_modeled(&elem) && is_fully_modeled(&params[0]) && !types::assignable(&elem, &params[0])) {
                            ctx.error(args[1].pos(), DiagnosticCode::CallbackSignatureMismatch, format!("filter() callback must take a single {} parameter", ts(&elem)));
                        }
                        if !types::type_equals(ret, &BOOL) && !matches!(**ret, Type::Any) {
                            ctx.error(args[1].pos(), DiagnosticCode::CallbackSignatureMismatch, format!("filter() callback must return bool, got {}", ts(ret)));
                        }
                    }
                    Type::Any => {}
                    other => ctx.error(args[1].pos(), DiagnosticCode::BuiltinArgType, format!("filter() second argument must be a function, got {}", ts(other))),
                }
            }
            // 絞り込むだけなので要素型は変わらない
            match at.first() {
                Some(t @ Type::Array(_)) => t.clone(),
                _ => ANY,
            }
        }
        "map" => {
            if expect_arity(ctx, name, n, 2, pos)
                && let Some(elem) = require_array(ctx, &at[0], args[0].pos(), format!("map() requires an array, got {}", ts(&at[0])))
            {
                match &at[1] {
                    Type::Fn { params, .. } => {
                        if params.len() != 1 || (is_fully_modeled(&elem) && is_fully_modeled(&params[0]) && !types::assignable(&elem, &params[0])) {
                            ctx.error(args[1].pos(), DiagnosticCode::CallbackSignatureMismatch, format!("map() callback must take a single {} parameter", ts(&elem)));
                        }
                    }
                    Type::Any => {}
                    other => ctx.error(args[1].pos(), DiagnosticCode::BuiltinArgType, format!("map() second argument must be a function, got {}", ts(other))),
                }
            }
            // 要素型はコールバックの戻り値型になる
            match at.get(1) {
                Some(Type::Fn { ret, .. }) => Type::Array(ret.clone()),
                _ => Type::Array(Box::new(ANY)),
            }
        }
        "reduce" => {
            if expect_arity(ctx, name, n, 3, pos)
                && let Some(elem) = require_array(ctx, &at[0], args[0].pos(), format!("reduce() requires an array, got {}", ts(&at[0])))
            {
                match &at[1] {
                    Type::Fn { params, ret } if params.len() != 2 => {
                        let _ = ret;
                        ctx.error(args[1].pos(), DiagnosticCode::CallbackSignatureMismatch, "reduce() callback must take (accumulator, element)");
                    }
                    Type::Fn { params, ret } => {
                        if is_fully_modeled(&params[0]) && !types::assignable(&at[2], &params[0]) {
                            ctx.error(args[2].pos(), DiagnosticCode::TypeMismatch, format!("reduce() initial value must be {}, got {}", ts(&params[0]), ts(&at[2])));
                        }
                        if is_fully_modeled(&elem) && is_fully_modeled(&params[1]) && !types::assignable(&elem, &params[1]) {
                            ctx.error(args[1].pos(), DiagnosticCode::CallbackSignatureMismatch, format!("reduce() callback's second parameter must accept {}", ts(&elem)));
                        }
                        if is_fully_modeled(ret) && is_fully_modeled(&params[0]) && !types::assignable(ret, &params[0]) {
                            ctx.error(args[1].pos(), DiagnosticCode::CallbackSignatureMismatch, format!("reduce() callback must return {} (the accumulator type), got {}", ts(&params[0]), ts(ret)));
                        }
                    }
                    Type::Any => {}
                    other => ctx.error(args[1].pos(), DiagnosticCode::BuiltinArgType, format!("reduce() second argument must be a function, got {}", ts(other))),
                }
            }
            // reduceの戻り値はaccumulator型でスカラーになりうる(map/filter等の配列戻り値と違い
            // モデル化できる)。Type::Fnはmilestone 26で追跡済みなので、TS版と同じく
            // callbackの第1引数型(f.params[0])を返す——fnでなければ初期値(args[2])の型、
            // それも無ければANY(TS版 `f.kind==="fn" && f.params.length===2 ? f.params[0] : args[2] ?? ANY`)。
            // これにより `round(reduce(xs, add, 0))`(reduce→int)のような後続検査がTS版と一致する
            match at.get(1) {
                Some(Type::Fn { params, .. }) if params.len() == 2 => params[0].clone(),
                _ => at.get(2).cloned().unwrap_or(ANY),
            }
        }
        _ => ANY,
    }
}

fn check_block(ctx: &mut FullCheckerCtx, block: &Block) {
    ctx.push_scope();
    for stmt in &block.stmts {
        check_stmt(ctx, stmt);
    }
    ctx.pop_scope();
}

fn check_stmt(ctx: &mut FullCheckerCtx, stmt: &Stmt) {
    match stmt {
        // names/valuesは1:1対応が前提(Go風多重代入はF-9で廃止済み。パーサが数を揃える)
        Stmt::ShortVarDecl { names, values, mutable, pos } => {
            // TS版`statements.ts`と同じ順序: **文全体の値を先に全部検査してから**名前を宣言する
            // (宣言を挟むと`a, b := 1, a`のような右辺の未定義名を見落とす)。
            // `already_errored`も**文単位**で判定する——名前ごとに取り直すと、先の値で
            // undefined-nameが出ていても後の名前にcannot-infer-typeを重ねて出してしまう
            // (`a, b := undefinedThing, []`。code reviewで発覚・両CLIで再現確認)
            let diags_before = ctx.diagnostics.len();
            let value_tys: Vec<Type> = values.iter().map(|v| infer_expr(ctx, v)).collect();
            let already_errored = ctx.diagnostics.len() > diags_before;
            for (i, name) in names.iter().enumerate() {
                let ty = value_tys.get(i).cloned().unwrap_or(ANY);
                // mut宣言はリテラル型を広げる(`mut s := "a"`のsはstring型——後で
                // `s = "b"`と別リテラルを代入できるように)。TS版statements.tsの
                // `stmt.mutable ? widenLiteral(t) : t`と同じ
                let ty = if *mutable { types::widen_literal(ty) } else { ty };
                // milestone 33: 空の配列リテラル(`xs := []`)は要素型が決まらないので
                // 型注釈を要求する(TS版`cannot-infer-type`)。**TS版の`containsAny`を
                // そのまま移植すると誤検知の山になる**——full_checkerはmap/channel/union/
                // pkg修飾など多くをANYへ縮退させており、TS版なら具体型がつく値
                // (`m := {"a": 1}`等)まで「推論できない」と報告してしまうため。
                // 空配列リテラルという**この診断の本来の引き金**だけに絞って移植する
                // (他の形は、対応する型をモデル化するmilestoneで広げていく)。
                // 値の検査で既に診断が出ている場合は二重に出さない(TS版の`alreadyErrored`)
                if name != "_"
                    && !already_errored
                    && matches!(values.get(i), Some(Expr::ArrayLit { elems, elem_type: None, .. }) if elems.is_empty())
                {
                    ctx.error(
                        *pos,
                        DiagnosticCode::CannotInferType,
                        format!("cannot infer a complete type for '{name}' (got {}) — add a type annotation, e.g. '{name}: T[] = []'", types::type_to_string(&ty)),
                    );
                }
                ctx.declare(name, ty, *pos, *mutable);
            }
        }
        Stmt::TypedVarDecl { name, type_node, value, mutable, pos } => {
            let declared = resolve_type_ann(&ctx.type_ctx, type_node);
            let value_ty = infer_expr(ctx, value);
            if !types::assignable(&value_ty, &declared) {
                ctx.error(
                    *pos,
                    DiagnosticCode::TypeMismatch,
                    format!("cannot assign a value of type '{}' to '{name}' of type '{}'", types::type_to_string(&value_ty), types::type_to_string(&declared)),
                );
            }
            ctx.declare(name, declared, *pos, *mutable);
        }
        Stmt::Assign { targets, values, compound_op, pos } => {
            for (target, value) in targets.iter().zip(values.iter()) {
                let value_ty = infer_expr(ctx, value);
                check_assign_target(ctx, target, &value_ty, *pos, compound_op.as_ref());
            }
        }
        Stmt::IncDec { target, op, pos } => {
            // TS版`case "incDec"`と同じ順序: まず対象の型を推論(identなら未定義名も検査)し、
            // int/floatでなければinvalid-operation(算術二項演算子と共有する診断)、
            // そのあとidentなら可変性を検査する
            let t = infer_expr(ctx, target);
            if !types::is_numeric(&t) {
                ctx.error(*pos, DiagnosticCode::InvalidOperation, format!("'{op}' requires int or float, got {}", types::type_to_string(&t)));
            }
            if let Expr::Ident { name, .. } = target {
                // 借用を跨がないよう可変性をboolへ落としてからctx.errorを呼ぶ
                let immutable = matches!(ctx.lookup(name), Some(b) if !b.mutable);
                if immutable {
                    ctx.error(*pos, DiagnosticCode::ImmutableAssignment, format!("'{name}' was declared without 'mut' and cannot be reassigned"));
                }
            }
        }
        Stmt::ExprStmt { expr, .. } => {
            infer_expr(ctx, expr);
        }
        Stmt::Return { value, .. } => {
            // 戻り値なしのreturn(値が要る関数で足りない場合はmissing-return-value)は
            // この一歩で実装した7種に含めていないので診断しない——値がある場合の
            // type-mismatchだけ、7種の同じ診断コードとしてここでも検査する。
            // milestone 31でTS版`statements.ts:178-187`と細部まで揃えた(位置は`return`
            // ではなく**値の式**・文言・戻り値なし関数でのreturn値はvoid-used-as-value)
            if let Some(v) = value {
                let value_ty = infer_expr(ctx, v);
                let expected = ctx.ret_stack.last().cloned().unwrap_or(VOID);
                if types::type_equals(&expected, &VOID) {
                    ctx.error(v.pos(), DiagnosticCode::VoidUsedAsValue, "this function has no return value");
                } else if !types::assignable(&value_ty, &expected) {
                    ctx.error(
                        v.pos(),
                        DiagnosticCode::TypeMismatch,
                        format!("cannot return {} as {}", types::type_to_string(&value_ty), types::type_to_string(&expected)),
                    );
                }
            }
        }
        Stmt::If(if_stmt) => check_if(ctx, if_stmt),
        Stmt::For { init, cond, post, body, .. } => {
            // ヘッダのinit変数(例: `for i := 0; ...`)用の外側スコープを1つ作り、
            // bodyはcheck_block経由でその内側にネストした別スコープを持つ(TS版
            // `checker/statements.ts`の`for`ケースと同じ2段構成——pushScope(ヘッダ)の
            // 後、bodyはcheckBlockが自分でpushScopeする)。ここを1段のスコープに
            // 潰すと、body内で`i := ...`のようにヘッダ変数と同名の宣言をしたとき
            // 「同じスコープ」と誤認してalready-declaredになってしまう
            // (正しくは外側スコープの再利用としてshadowing——実装時に見落とし、
            // git履歴・TS版との突き合わせで発覚した)
            ctx.push_scope();
            if let Some(init) = init {
                check_for_init(ctx, init);
            }
            if let Some(cond) = cond {
                infer_expr(ctx, cond);
            }
            if let Some(post) = post {
                check_stmt(ctx, post);
            }
            check_block(ctx, body);
            ctx.pop_scope();
        }
        Stmt::RangeFor { names, subject, body, pos } => {
            // for文と同じ理由でヘッダ(range変数)とbodyは別スコープにする
            let subject_ty = infer_expr(ctx, subject);
            ctx.push_scope();
            // 配列/map/intのrangeは型と名前の個数まで検査する(TS版`statements.ts`の
            // rangeForケース。milestone 33で配列とint、milestone 34でmap)。ANY
            // (未対応の型・エラー回復)は従来どおり全ての名前をANYで宣言して素通りさせる
            match &subject_ty {
                Type::Array(elem) => {
                    if names.len() != 2 {
                        ctx.error(*pos, DiagnosticCode::RangeArity, "range over an array needs two names: 'for a, b := range ...' (use _ to ignore one)");
                    }
                    let elem = (**elem).clone();
                    ctx.declare(names.first().map(String::as_str).unwrap_or("_"), INT, *pos, false);
                    ctx.declare(names.get(1).map(String::as_str).unwrap_or("_"), elem, *pos, false);
                }
                Type::Map { key, value } => {
                    if names.len() != 2 {
                        ctx.error(*pos, DiagnosticCode::RangeArity, "range over a map needs two names: 'for a, b := range ...' (use _ to ignore one)");
                    }
                    let (k, v) = ((**key).clone(), (**value).clone());
                    ctx.declare(names.first().map(String::as_str).unwrap_or("_"), k, *pos, false);
                    ctx.declare(names.get(1).map(String::as_str).unwrap_or("_"), v, *pos, false);
                }
                t if types::type_equals(t, &INT) => {
                    if names.len() != 1 {
                        ctx.error(*pos, DiagnosticCode::RangeArity, "range over an int takes exactly one name: 'for i := range n'");
                    }
                    // **宣言するのは1つ目の名前だけ**(TS版`statements.ts`のintブランチは
                    // `stmt.names[0]`しか宣言しない)。余分な名前を宣言してしまうと、
                    // `for i, x := range 5 { print(x) }`でTS版が出す`undefined-name`を
                    // 見落とす(code reviewで発覚・両CLIで再現確認)
                    if let Some(first) = names.first() {
                        ctx.declare(first, INT, *pos, false);
                    }
                }
                Type::Any => {
                    for n in names {
                        ctx.declare(n, ANY, *pos, false);
                    }
                }
                other => {
                    ctx.error(subject.pos(), DiagnosticCode::NotRangeable, format!("cannot range over {}", types::type_to_string(other)));
                    for n in names {
                        ctx.declare(n, ANY, *pos, false);
                    }
                }
            }
            check_block(ctx, body);
            ctx.pop_scope();
        }
        Stmt::Break { .. } | Stmt::Continue { .. } => {}
        Stmt::Wait { body, .. } => check_block(ctx, body),
        Stmt::Send { channel, value, pos } => {
            // milestone 34: `ch <- v`の要素型検査(TS版`statements.ts`のsendケース)
            let ch = infer_expr(ctx, channel);
            let v = infer_expr(ctx, value);
            match &ch {
                Type::Chan(elem) => {
                    if is_fully_modeled(elem) && !types::assignable(&v, elem) {
                        ctx.error(*pos, DiagnosticCode::TypeMismatch, format!("cannot send {} to {}", types::type_to_string(&v), types::type_to_string(&ch)));
                    }
                }
                Type::Any => {}
                other => ctx.error(channel.pos(), DiagnosticCode::NotAChannel, format!("cannot send to non-channel type {}", types::type_to_string(other))),
            }
        }
        Stmt::DeferStmt { call, .. } => {
            infer_expr(ctx, call);
        }
    }
}

fn check_assign_target(ctx: &mut FullCheckerCtx, target: &Expr, value_ty: &Type, stmt_pos: Pos, compound_op: Option<&TokenType>) {
    // milestone 33/34: 配列要素・mapエントリへの代入(`xs[0] = v` / `m["k"] = v`)は、
    // 添字読みと同じ経路(`infer_index`)で型を得て値を照合する(TS版`statements.ts`の
    // assignケース)。**mapの代入先の型は読みの`V | none`ではなく`V`**。
    // **位置は代入文自身**(TS版は`stmt.pos`で報告する)——targetの位置を使うと
    // `a, xs[0] = 2, "no"`のような複数代入でTS版と列がずれる(code reviewで発覚)
    if let Expr::Index { target: container, index, pos } = target {
        let info = infer_index(ctx, container, index, *pos);
        // milestone 34: mapへの複合代入(`m[k] += 1`)は禁止——キーが存在しないかもしれず
        // (読みは常に`V | none`)、「現在値 op 右辺」を無条件に計算すると欠損キーが
        // 黙って壊れた値になるため(TS版`compound-assign-on-map`)
        if let Some(op) = compound_op
            && matches!(info.container, Type::Map { .. })
        {
            // 文言は**実際に書かれた演算子**で組み立てる(TS版`statements.ts`も
            // `stmt.compoundOp`を埋め込む)——`+=`固定にすると`-=`等でTS版と食い違う
            // (code reviewで発覚・両CLIで再現確認)
            ctx.error(
                stmt_pos,
                DiagnosticCode::CompoundAssignOnMap,
                format!("cannot use '{op}=' on a map entry — the key may not exist yet; write 'm[k] = (m[k] or fallback) {op} value' instead"),
            );
            return;
        }
        if is_fully_modeled(&info.write) && !types::assignable(value_ty, &info.write) {
            ctx.error(stmt_pos, DiagnosticCode::TypeMismatch, format!("cannot assign {} to {}", types::type_to_string(value_ty), types::type_to_string(&info.write)));
        }
        return;
    }
    let Expr::Ident { name, pos } = target else {
        infer_expr(ctx, target); // member代入(フィールド)はinfer_member側で検証済み
        return;
    };
    match ctx.lookup(name) {
        None => {
            infer_expr(ctx, target); // 共通経路でundefined-nameを報告
        }
        Some(b) if !b.mutable => {
            ctx.error(*pos, DiagnosticCode::ImmutableAssignment, format!("'{name}' was declared without 'mut' and cannot be reassigned"));
        }
        Some(b) if !types::assignable(value_ty, &b.ty) => {
            let declared_ty_str = types::type_to_string(&b.ty);
            ctx.error(*pos, DiagnosticCode::TypeMismatch, format!("cannot assign a value of type '{}' to '{name}' of type '{declared_ty_str}'", types::type_to_string(value_ty)));
        }
        _ => {}
    }
}

// C風forのヘッダ変数は暗黙に可変(TS版`checker/statements.ts`の`for`ケース、B-4決定:
// デフォルト不変の唯一の構造的例外)。`i++`という後置文が書けるようにするための特例で、
// ここだけAST上の`mutable`フラグ(通常`mut`を書かない限りfalse)を無視してtrue扱いにする
fn check_for_init(ctx: &mut FullCheckerCtx, init: &Stmt) {
    if let Stmt::ShortVarDecl { names, values, pos, .. } = init {
        for (name, value) in names.iter().zip(values.iter()) {
            // forヘッダ変数は(:=で書いても)常にmutable扱い——codegenも常にletで出し、
            // postで書き換わる。ShortVarDeclのmut分岐と同じくリテラル型を広げないと、
            // milestone 25でExpr::StringをType::Literalにした結果、
            // `for s := "a"; s != "z"; s = "b"` のsがLiteral("a")のままになり、
            // 比較(incomparable-types)・再代入(type-mismatch)が誤検知になる
            // (code reviewで発見した回帰)
            let ty = types::widen_literal(infer_expr(ctx, value));
            ctx.declare(name, ty, *pos, true);
        }
    } else {
        check_stmt(ctx, init);
    }
}

// milestone 34: `if v is closed { ... }`の絞り込み。条件が「裸の識別子 is T」の形のときだけ
// 対応する(TS版のnarrowingは任意のstable pathを追うが、full_checkerでは実際のexampleが
// 使う形——`<-ch`やmap読みをローカル変数へ束縛してからisで振り分ける——をカバーすれば足りる)。
// 型「解決」層は既存の`checker::narrow_for_is`(codegen側milestone 7の資産)を再利用する。
// **channel/mapのモデル化とセットで必要になった**——recvが`T | closed`、map読みが`V | none`を
// 返すようになったため、絞り込みが無いと`if v is closed { break }`の後で`total + v`が
// 誤って`invalid-operation`になる(実装中にexamplesで検出)
// milestone 39: match式。TS版`checker/match-select.ts`の`inferMatch`の移植。
//
// この一歩の主眼は診断そのものより**アーム本体を走査すること**にある——それまで
// `infer_expr`の`_ => ANY`に落ちていたため、`match`の中に書かれたコード(Meshでは
// union路線なので処理の大半がここに集まる)が丸ごと未検査だった。
//
// **誤検知を避けるための方針**: パターン系の診断(impossible/unreachable/網羅性)は
// subjectのunionメンバーが`safe_to_compare`(=中身が解決済みで`type_equals`に
// かけても安全)なときだけ出す。full_checkerは未モデル化の型をANYへ縮退させるので、
// 「unionのはずなのにANY」は日常的に起きる——そこで診断を出すと必ず誤検知になる。
fn infer_match(ctx: &mut FullCheckerCtx, subject_expr: &Expr, arms: &[MatchArm], pos: Pos) -> Type {
    let subject = infer_expr(ctx, subject_expr);
    if arms.is_empty() {
        ctx.error(pos, DiagnosticCode::EmptyMatch, "match must have at least one arm");
        return ANY;
    }
    // `union-required`は「unionでもANYでもない」なら出す。**full_checkerが型を諦めるときの
    // 行き先は必ずANY**(resolve_type_annもwiden_field_typeもANYへ畳む)なので、ANY以外の
    // 具体的な型が来たならそれは本当にunionではない——`is_fully_modeled`で更に絞ると、
    // 関数値(`f := add`のType::Fn)のような**正確に分かっている型**まで見逃す
    // (code reviewで発覚・両CLIで再現確認。`is_fully_modeled`は「レジストリ側の宣言型と
    // 突き合わせてよいか」を答える述語で、ここで訊きたい問いとは別物だった)
    if !matches!(subject, Type::Union { .. } | Type::Any) {
        ctx.error(subject_expr.pos(), DiagnosticCode::UnionRequired, format!("match subject must be a union type, got {}", types::type_to_string(&subject)));
    }
    // パターン照合に使えるunionメンバー。未解決/比較すると危険なメンバーが1つでもあれば
    // Noneにして、パターン系の診断も絞り込みもまるごと諦める(検出漏れ側)
    let members: Option<Vec<Type>> = match &subject {
        Type::Union { body } => body.get().map(|b| b.members.clone()).filter(|ms| ms.iter().all(safe_to_compare)),
        _ => None,
    };
    // TS版`stablePath`相当。full_checkerが絞り込めるのは**裸の不変な識別子**だけ
    // (`n.next`のようなフィールドパスは対象外——絞り込まないぶんには検出漏れ側に倒れる)。
    // `mut`を除くのは`check_if`と同じ理由(milestone 34のcode review参照)
    let narrow_name = match subject_expr {
        Expr::Ident { name, .. } => ctx.lookup(name).filter(|b| !b.mutable).map(|_| name.clone()),
        _ => None,
    };

    let mut covered: Vec<Type> = Vec::new();
    let mut arm_types: Vec<Type> = Vec::new();
    let mut saw_wildcard = false;

    for arm in arms {
        if saw_wildcard {
            // TS版と同じく、到達しないアームは本体を検査しない(continue)
            ctx.error(arm.pos, DiagnosticCode::UnreachablePattern, "unreachable arm — '_' already matches everything before this");
            continue;
        }
        let mut pattern_types: Vec<Type> = Vec::new();
        // このアームのパターンに「解決できなかった型」が混じっていたら、絞り込みは行わない
        // (誤った型でアーム本体を検査すると、そこから誤検知が連鎖する)
        let mut undecidable = false;
        for p in &arm.patterns {
            match p {
                MatchPattern::Wildcard { pos: wpos } => {
                    if arm.patterns.len() > 1 {
                        ctx.error(*wpos, DiagnosticCode::WildcardNotAlone, "'_' must be the only pattern in its arm");
                    }
                    saw_wildcard = true;
                }
                MatchPattern::Type(node) => {
                    let pt = crate::checker::resolve_type_node(&ctx.type_ctx, node);
                    if !safe_to_compare(&pt) {
                        undecidable = true;
                        continue;
                    }
                    // 判別可能union: `{ kind: "ok" }`のような部分構造パターンは、書かれた
                    // フィールドが一致するunionメンバー(具体的な形)へ解決してから通常の
                    // 型パターンと同じに扱う(1個のパターンが複数メンバーに一致することもある)
                    let resolved: Vec<Type> = match (&pt, &members) {
                        (Type::Struct { .. }, Some(ms)) => ms.iter().filter(|m| struct_pattern_matches(m, &pt)).cloned().collect(),
                        (_, Some(ms)) if !ms.iter().any(|m| types::type_equals(m, &pt)) => Vec::new(),
                        _ => vec![pt.clone()],
                    };
                    if members.is_some() && resolved.is_empty() {
                        ctx.error(arm.pos, DiagnosticCode::ImpossiblePattern, format!("{} can never be {}", types::type_to_string(&subject), types::type_to_string(&pt)));
                    }
                    for rp in resolved {
                        if members.is_some() && covered.iter().any(|c| types::type_equals(c, &rp)) {
                            ctx.error(arm.pos, DiagnosticCode::UnreachablePattern, format!("unreachable pattern — {} is already covered", types::type_to_string(&rp)));
                        }
                        covered.push(rp.clone());
                        pattern_types.push(rp);
                    }
                }
            }
        }

        // アーム内での対象の型: 型パターンならその union、`_`なら「まだカバーされていない残り」。
        // **TS版との意図的な差**: 残りが空(=`_`より前で全メンバーを尽くしている)のとき、
        // TS版は`unionOf([])`=voidへ絞り込むが、ここではsubjectのままにする。voidを配って
        // アーム本体を検査すると、本来の型で書かれた正当なコードが軒並みtype-mismatchに
        // なる(そのアームが到達不能なのは`unreachable-pattern`側の話)——誤検知を避ける
        let narrowed = if saw_wildcard && pattern_types.is_empty() {
            match &members {
                Some(ms) => {
                    let rest: Vec<Type> = ms.iter().filter(|m| !covered.iter().any(|c| types::type_equals(c, m))).cloned().collect();
                    if rest.is_empty() { subject.clone() } else { types::union_of(rest) }
                }
                None => subject.clone(),
            }
        } else if pattern_types.is_empty() {
            // このアームのパターンが**全て不可能**(union実メンバーへ1つも解決できなかった)。
            // TS版は`unionOf([ANY])`=ANYへ絞り込む——既に`impossible-pattern`で報告済みの
            // 死んだアームなので、その中身は追加で咎めない。ここでsubject(unionのまま)を
            // 配ると`r + 1`のような本体がunion相手の`invalid-operation`という**TS版に無い
            // 誤検知**になる(code reviewで発覚・両CLIで再現確認)
            ANY
        } else {
            types::union_of(pattern_types)
        };

        ctx.push_scope();
        // **絞り込むのは`members`が取れたときだけ**。取れなかった(subjectがANYへ縮退して
        // いる)ときのpattern_typesは「union実メンバー」ではなく**書かれたパターンそのもの**
        // なので、それで絞り込むと部分構造パターンの`{ kind: "ok" }`が
        // 「kindしか持たないstruct」として居座り、アーム本体の`res.user`が
        // unknown-fieldの誤検知になる(実装中にexamplesとの突き合わせで発覚)。
        // union注釈は`resolve_type_ann`がANYへ縮退させるので、**注釈付き引数を
        // matchするコードでは絞り込みが効かない**——検出漏れ側の既知の限界
        if let Some(name) = &narrow_name
            && members.is_some()
            && !undecidable
        {
            ctx.narrow(&name.clone(), narrowed);
        }
        arm_types.push(infer_expr(ctx, &arm.body));
        ctx.pop_scope();
    }

    // 網羅性検査: unionの全メンバーがカバーされているか(`_`があれば常に網羅)
    if let Some(ms) = &members
        && !saw_wildcard
    {
        let missing: Vec<String> = ms.iter().filter(|m| !covered.iter().any(|c| types::type_equals(c, m))).map(types::type_to_string).collect();
        if !missing.is_empty() {
            ctx.error(pos, DiagnosticCode::MatchNotExhaustive, format!("match is not exhaustive — missing: {} (add arms for them, or a '_' arm)", missing.join(", ")));
        }
    }

    arms_result_type(ctx, arm_types, pos, "match")
}

// milestone 39: select式。TS版`inferSelect`の移植。matchと違い、アームは型パターンでは
// なく「どのチャネル操作が先に終わったか」で選ばれる——受信値は`T | closed`として
// アームのスコープに束縛する
fn infer_select(ctx: &mut FullCheckerCtx, arms: &[SelectArm], default_arm: Option<&Expr>, pos: Pos) -> Type {
    if arms.is_empty() && default_arm.is_none() {
        ctx.error(pos, DiagnosticCode::EmptySelect, "select must have at least one arm");
        return ANY;
    }
    let mut arm_types: Vec<Type> = Vec::new();
    for arm in arms {
        let ch = infer_expr(ctx, &arm.channel);
        let binding_ty = match &ch {
            // 要素型が未解決なら`union_of`が内部で`.expect()`するので、安全な場合だけ組む
            Type::Chan(elem) if safe_to_compare(elem) => types::union_of(vec![(**elem).clone(), types::CLOSED]),
            Type::Chan(_) | Type::Any => ANY,
            other => {
                ctx.error(arm.channel.pos(), DiagnosticCode::NotAChannel, format!("select arm requires a channel, got {}", types::type_to_string(other)));
                ANY
            }
        };
        ctx.push_scope();
        ctx.declare(&arm.name, binding_ty, arm.pos, false);
        arm_types.push(infer_expr(ctx, &arm.body));
        ctx.pop_scope();
    }
    if let Some(def) = default_arm {
        arm_types.push(infer_expr(ctx, def));
    }
    arms_result_type(ctx, arm_types, pos, "select")
}

// match/selectの結果型(TS版の末尾3行が両方同じ形なので共通化): 全アームがvoidなら
// void(文として使う)、混ざっていれば`mixed-void-arms`、それ以外はアームのunion
fn arms_result_type(ctx: &mut FullCheckerCtx, arm_types: Vec<Type>, pos: Pos, kind: &str) -> Type {
    if arm_types.is_empty() {
        return ANY;
    }
    let voids = arm_types.iter().filter(|t| types::type_equals(t, &VOID)).count();
    if voids == arm_types.len() {
        return VOID;
    }
    if voids > 0 {
        ctx.error(pos, DiagnosticCode::MixedVoidArms, format!("{kind} arms mix values and void — all arms must return a value, or none"));
        return ANY;
    }
    // アームの型に未解決unionが混じっていると`union_of`が内部で`.expect()`する
    if !arm_types.iter().all(safe_to_compare) {
        return ANY;
    }
    types::union_of(arm_types)
}

// TS版`narrowing.ts`の`structPatternMatches`の移植。**codegen側の
// `checker::pattern_matches_member`とは別物**——あちらはTypeNode(未解決のパターン)を
// 受け取り、名前付きstructパターンを名前で照合する。TS版のmatchは解決済みの型どうしを
// 構造的に照合するので、こちらを使わないとTS版と結果が食い違う
fn struct_pattern_matches(member: &Type, pattern: &Type) -> bool {
    let (Type::Struct { fields: mf, .. }, Type::Struct { fields: pf, .. }) = (member, pattern) else {
        return types::type_equals(member, pattern);
    };
    let (Some(mfs), Some(pfs)) = (mf.get(), pf.get()) else { return false };
    pfs.iter().all(|p| mfs.iter().find(|m| m.name == p.name).is_some_and(|m| types::type_equals(&m.type_, &p.type_)))
}

// `types::type_equals`/`types::union_of`へ渡しても**パニックしない**かの判定。
// どちらも「unionのbody・structのfieldsは解決済み」を前提に`.expect()`するが、
// `resolve_type_decls`は裸のunion循環などで解決に失敗すると中身が空のまま登録を残す
// (milestone 37で`mesh test`がこれでクラッシュした)。判定できないときは診断を
// 出さない=検出漏れ側に倒す、という既定方針のための門番。
// 再帰structで無限に潜らないよう、訪問済みのfieldsセルを持ち回る(`type_equals`が
// 同じ問題を`seen`で解いているのと同じ手)
fn safe_to_compare(t: &Type) -> bool {
    fn go(t: &Type, seen: &mut Vec<*const OnceCell<Vec<types::StructField>>>) -> bool {
        match t {
            Type::Union { body } => match body.get() {
                Some(b) => b.members.iter().all(|m| go(m, seen)),
                None => false,
            },
            Type::Struct { fields, .. } => {
                let ptr = Rc::as_ptr(fields);
                if seen.contains(&ptr) {
                    return true; // 自己参照(既に検査中)——ここで打ち切ってよい
                }
                let Some(fs) = fields.get() else { return false };
                seen.push(ptr);
                let ok = fs.iter().all(|f| go(&f.type_, seen));
                seen.pop();
                ok
            }
            Type::Array(e) | Type::Chan(e) => go(e, seen),
            Type::Map { key, value } => go(key, seen) && go(value, seen),
            Type::Fn { params, ret } => params.iter().all(|p| go(p, seen)) && go(ret, seen),
            _ => true,
        }
    }
    go(t, &mut Vec::new())
}

fn check_if(ctx: &mut FullCheckerCtx, if_stmt: &IfStmt) {
    infer_expr(ctx, &if_stmt.cond);
    // 条件が `ident is T` なら then/else 側の型を計算しておく
    let narrowing = match &if_stmt.cond {
        Expr::Is { operand, target, .. } => match &**operand {
            // **`mut`な束縛は絞り込まない**(TS版`narrowing.ts`の`stablePath`が
            // `binding && !binding.mutable ? name : null`で不変な束縛だけを対象にするのと同じ)。
            // 再代入で型が変わりうる変数を絞り込むと、絞り込んだ型が「宣言された型」として
            // 居座り、後続の正当な再代入をtype-mismatchで誤って弾く一方、本来出るべき
            // 診断を見落とす(code reviewで発覚・両CLIで再現確認)
            // `safe_to_compare`は milestone 39 で追加した門番——`narrow_for_is`は
            // union bodyが解決済み前提で`.expect()`するので、裸のunion循環などで
            // 未解決のまま残った型を渡すとパニックする(milestone 37で`mesh test`が
            // 同じ形でクラッシュした前例。絞り込まない=検出漏れ側に倒す)
            Expr::Ident { name, .. } => ctx.lookup(name).filter(|b| !b.mutable && safe_to_compare(&b.ty)).map(|b| {
                let (then_ty, else_ty) = crate::checker::narrow_for_is(&ctx.type_ctx, &b.ty, target);
                (name.clone(), then_ty, else_ty)
            }),
            _ => None,
        },
        _ => None,
    };

    ctx.push_scope();
    if let Some((name, then_ty, _)) = &narrowing {
        ctx.narrow(name, then_ty.clone());
    }
    check_block(ctx, &if_stmt.then);
    ctx.pop_scope();

    match if_stmt.else_.as_deref() {
        Some(ElseClause::If(nested)) => {
            ctx.push_scope();
            if let Some((name, _, else_ty)) = &narrowing {
                ctx.narrow(name, else_ty.clone());
            }
            check_if(ctx, nested);
            ctx.pop_scope();
        }
        Some(ElseClause::Block(b)) => {
            ctx.push_scope();
            if let Some((name, _, else_ty)) = &narrowing {
                ctx.narrow(name, else_ty.clone());
            }
            check_block(ctx, b);
            ctx.pop_scope();
        }
        // else節が無く、then節が必ず終端する(return/break/continue)なら、**後続の同じ
        // ブロック**は絞り込んだ「残り」の型で読める(codegen側`gen_if`と同じフォールスルー
        // 処理)。`if v is closed { break }` の直後に v を int として使う形がこれ
        None => {
            if let Some((name, _, else_ty)) = &narrowing
                && block_always_terminates(&if_stmt.then)
            {
                ctx.narrow(&name.clone(), else_ty.clone());
            }
        }
    }
}

// ブロックが必ず終端する(return/break/continueで終わる)かの単純判定。codegen.rsの
// 同名関数と同じ単純化(最後の文だけを見る)——実際のexampleはこの形で足りる
fn block_always_terminates(block: &Block) -> bool {
    matches!(block.stmts.last(), Some(Stmt::Return { .. }) | Some(Stmt::Break { .. }) | Some(Stmt::Continue { .. }))
}

// 関数のシグネチャ型(Type::Fn)。milestone 26でcheck_programがトップレベル関数を
// この型で登録するようになり、呼び出し側の個数・型照合(argument-count/type-mismatch)が
// 効くようになった。params/retはスカラースコープで解決する(union/struct/配列/pkg修飾型は
// resolve_type_annでANYへ潰れる(struct型はmilestone 29から解決される)——個数照合は
// 常に効き、型照合はスカラー+structで効く)。
// TS版`checkPackage`もトップレベル関数を通常のdeclareBindingでFn型として登録する
fn fn_signature(tc: &crate::checker::CheckerCtx, f: &FnDecl) -> Type {
    let params = f.params.iter().map(|p| resolve_type_ann(tc, &p.type_node)).collect();
    let ret = f.ret.as_ref().map(|r| resolve_type_ann(tc, r)).unwrap_or(VOID);
    Type::Fn { params, ret: Box::new(ret) }
}

// `fn (u: User) describe() string {...}`のシグネチャをメソッド表へ登録する
// (TS版`checker/functions.ts`の`declareMethod`の移植)。milestone 30では登録だけを
// 行っていたが、milestone 31でTS版と同じ4つの宣言時検査を追加した——順序もTS版どおりで、
// どれかに引っかかったら登録せずに戻る(レシーバ非struct→builtin名→フィールド衝突→重複)。
// **型空間の使い分け**: 「レシーバがstructか」の判定と診断メッセージだけは`checker.rs`の
// 完全解決(`resolve_type_node`、TS版`resolveType`に対応)を使う——`fn (x: int)`/
// `fn (x: []int)`のときの"got int"/"got int[]"という文言をTS版と一致させるため。
// 一方メソッドのパラメータ/戻り値型は従来どおりfull_checker側の縮退解決
// (`resolve_type_ann`)で作る:呼び出し側の引数の型も縮退するので、両者を同じ型空間へ
// 揃えないと引数照合(milestone 31)が誤検知する(milestone 29/30と同じ配慮)。
// **フィールドが勝つ**のは参照時のlookup順(infer_member/Callアームがフィールドを先に見る)
// だが、そもそも同名のフィールドとメソッドはここでmethod-field-conflictとして弾かれる。
// **既知の限界**: 未知の型名のレシーバ(`fn (y: Bogus) f()`)は`resolve_type_node`が
// 「空フィールドの殻struct」へフォールバックするため、TS版が出す`unknown-type`+
// `invalid-receiver-type`のどちらも出ない(full_checkerは型注釈のunknown-type検査自体が
// 未移植——誤検知ではなく検出漏れ側に倒す既定方針どおり。下記のlookup_structガード参照)
fn declare_method(ctx: &mut FullCheckerCtx, f: &FnDecl) {
    let Some(recv) = &f.receiver else { return };
    let recv_full = crate::checker::resolve_type_node(&ctx.type_ctx, &recv.type_node);
    let Type::Struct { name: sname, fields: fcell, .. } = &recv_full else {
        ctx.error(
            recv.pos,
            DiagnosticCode::InvalidReceiverType,
            format!("method receiver must be a struct type, got {}", types::type_to_string(&recv_full)),
        );
        return;
    };
    let sname = sname.clone();
    // **殻structのフォールバックを弾く**(code reviewで発覚・再現確認済み): `resolve_type_node`は
    // 未知の型名・pkg修飾型を「空フィールドの殻struct」へフォールバックさせる(checker.rs)ため、
    // 上のstruct判定だけでは`fn (y: Bogus) f()`のような未宣言のレシーバも通ってしまう。
    // レジストリに実在するstructでなければ、**登録も残りの診断もせず素通りさせる**——
    // milestone 30の「レシーバが未宣言/非struct/pkg修飾なら登録しない(誤った名前で登録しない
    // ため)」というガードを、resolve_type_node化の後も維持するもの。これが無いと、存在しない
    // structに同名メソッドを2つ書いたときに`duplicate-method`という**TS版には無い誤検知**が出る
    // (TS版はunknown-type+invalid-receiver-type。full_checkerはunknown-type未移植なので、
    // 誤検知ではなく検出漏れ側に倒す既定方針どおり無診断にする)。
    // 型aliasが名前付きstructを指す場合は解決後の実名で引くので通る
    if ctx.type_ctx.lookup_struct(&sname).is_none() {
        return;
    }
    let field_names: Vec<String> = fcell.get().map(|fs| fs.iter().map(|fd| fd.name.clone()).collect()).unwrap_or_default();
    // 組み込み関数名はメソッド名にも使えない(TS版はここだけ「cannot be used as a method
    // name」という専用の文言。declare()の"cannot be redeclared"とは別)
    if crate::checker::is_builtin(&f.name) {
        ctx.error(f.pos, DiagnosticCode::BuiltinRedeclared, format!("'{}' is a builtin function and cannot be used as a method name", f.name));
        return;
    }
    if field_names.contains(&f.name) {
        ctx.error(f.pos, DiagnosticCode::MethodFieldConflict, format!("{sname} already has a field named '{}'", f.name));
        return;
    }
    if ctx.type_ctx.lookup_method(&sname, &f.name).is_some() {
        ctx.error(f.pos, DiagnosticCode::DuplicateMethod, format!("{sname} already has a method named '{}'", f.name));
        return;
    }
    let mut params = vec![resolve_type_ann(&ctx.type_ctx, &recv.type_node)];
    params.extend(f.params.iter().map(|p| resolve_type_ann(&ctx.type_ctx, &p.type_node)));
    let ret = f.ret.as_ref().map(|r| resolve_type_ann(&ctx.type_ctx, r)).unwrap_or(VOID);
    // mainパッケージ単一ファイルなので struct 名は無修飾のまま(declare_methodの
    // qualify_struct_nameはmain時に無修飾——lookup側も無修飾のstruct.nameで引く)
    ctx.type_ctx.declare_method(&sname, &f.name, Type::Fn { params, ret: Box::new(ret) });
}

fn check_fn(ctx: &mut FullCheckerCtx, f: &FnDecl) {
    // パラメータ用の外側スコープ+本体用のネストしたスコープ(check_block)という
    // 2段構成(TS版`checker/functions.ts`と同じ——pushScope〈パラメータ〉の後、
    // 本体はcheckBlockが自分でpushScopeする)。1段に潰すと、本体でパラメータ名を
    // 再宣言したとき「同じスコープ」と誤認してalready-declaredになってしまう
    // (正しくは外側スコープの再利用としてshadowing)
    ctx.push_scope();
    // milestone 31: メソッドのレシーバ(`fn (u: User) ...`の`u`)をパラメータと同じ
    // スコープへ、パラメータより先に宣言する(TS版`checkFn`と同じ順序)。これが無いと
    // メソッド本体の`u`がundefined-nameになるため、メソッド本体の検査とセットで入れた
    if let Some(recv) = &f.receiver {
        let rt = resolve_type_ann(&ctx.type_ctx, &recv.type_node);
        ctx.declare(&recv.name, rt, recv.pos, false);
    }
    for p in &f.params {
        let pt = resolve_type_ann(&ctx.type_ctx, &p.type_node);
        ctx.declare(&p.name, pt, p.pos, false);
    }
    ctx.ret_stack.push(f.ret.as_ref().map(|r| resolve_type_ann(&ctx.type_ctx, r)).unwrap_or(VOID));
    check_block(ctx, &f.body);
    ctx.ret_stack.pop();
    ctx.pop_scope();
}

// トップレベル定数(F-9c)。値の式はローカル変数宣言と同じくtype-mismatch/undefined-name
// の対象(見落とすと定数の初期値だけすり抜けてしまう)。最後にTS版`checker/modules.ts`と
// 同じ「型注釈があればそちら、無ければ値から推論」の優先順位でscopes[0]へdeclareする——
// これによりmilestone 23で追加した名前衝突検査(reserved-word/builtin-redeclared/
// already-declared/shadowing)がローカル変数と同じdeclare()経由で自動的に効く
fn check_top_level_const(ctx: &mut FullCheckerCtx, c: &ConstDecl) {
    let value_ty = infer_expr(ctx, &c.value);
    let final_ty = match &c.type_node {
        Some(type_node) => {
            let declared = resolve_type_ann(&ctx.type_ctx, type_node);
            if !types::assignable(&value_ty, &declared) {
                ctx.error(
                    c.pos,
                    DiagnosticCode::TypeMismatch,
                    format!("cannot assign a value of type '{}' to '{}' of type '{}'", types::type_to_string(&value_ty), c.name, types::type_to_string(&declared)),
                );
            }
            declared
        }
        None => value_ty,
    };
    ctx.declare(&c.name, final_ty, c.pos, false);
}

// このProgram(単一ファイル。milestone 22はimport/パッケージ対象外)を検査し、
// 見つかった診断を返す。空なら「(このスコープの範囲で)問題なし」の意味
pub fn check_program(program: &Program) -> Vec<Diagnostic> {
    check_program_opts(program, true)
}

// `require_main`はTS版`checkPackage`の同名オプションに対応する。mainパッケージ以外
// (importされたパッケージ)は`fn main`を持たないのが普通なので、そこへ`missing-main`を
// 出さないための切り替え(TS版は`pkg === "main" && !testMode`で渡す)。
// milestone 35でCLIが**全モジュールを1ファイルずつ検査する**ようになったときに追加した
// ——これが無いと、importされたパッケージ全部がmissing-mainの誤検知になる
pub fn check_program_opts(program: &Program, require_main: bool) -> Vec<Diagnostic> {
    check_package(&[("".to_string(), program)], require_main).into_iter().flat_map(|(_, d)| d).collect()
}

// **1パッケージぶん**(= 同じ名前空間を共有する複数ファイル)を検査し、ファイルごとの診断を返す。
// TS版`checker/modules.ts`の`checkPackage`に対応する。
//
// milestone 37で追加した。それまでは1ファイルずつ独立に検査していたため、**同じパッケージの
// 別ファイルにある関数を呼ぶと`undefined-name`の誤検知**になっていた(`mesh test`で
// `main_test.mesh`から`main.mesh`の関数を呼ぶ形と、複数ファイルのパッケージが内部で
// 相互参照する形の両方で発生。後者はmilestone 35のゲート統合以降`mesh check`/`run`/`build`が
// 正当なプログラムを弾いていた)。
//
// 順序はTS版と同じ: (1)全ファイルの型宣言を解決 → (2)メソッド表 → (3)import alias →
// (4)全ファイルの関数シグネチャを登録(前方参照・相互再帰・ファイル跨ぎの参照を許す) →
// (5)トップレベル定数を検査+登録 → (6)`fn main`の形 → (7)関数本体を検査。
// 診断は**それが出たファイル**へ振り分ける(ctx.diagnosticsは追記のみなので、各段階の
// 前後の長さで区間を切り出す)
pub fn check_package(files: &[(String, &Program)], require_main: bool) -> Vec<(String, Vec<Diagnostic>)> {
    let mut ctx = FullCheckerCtx::new();
    // どの区間の診断がどのファイルのものかを記録する(start, end, file index)
    let mut spans: Vec<(usize, usize, usize)> = Vec::new();
    // milestone 29: struct/union型のレジストリを既存の`checker.rs`のリゾルバで構築する
    // (再利用。knot-tying・自己参照・循環検出込み)。fn/const登録より前に済ませておく——
    // fn_signatureや型注釈がstruct名を解決できるようにするため。Err(裸union循環等)は
    // この一歩では診断化せず握り潰す(TS版の`type-alias-cycle`診断は将来のmilestone。
    // 稀なケースなので誤検知ではなく検出漏れ側に倒す)。**注意**: resolve_type_declsは
    // 最初のErrで走査全体を打ち切るため(checker.rsのループ設計)、壊れた型宣言1つで
    // **それ以降にファイル内で宣言されたstructも全て未登録=ANY扱い**になり、そのstructの
    // リテラル検証が無診断で素通りする(code reviewで波及範囲を明確化)。診断機構
    // (type-alias-cycle等)を入れる次段階でここも部分解決へ改善する候補
    let all_types: Vec<crate::ast::TypeDecl> = files.iter().flat_map(|(_, p)| p.types.iter().cloned()).collect();
    let _ = crate::checker::resolve_type_decls(&mut ctx.type_ctx, &all_types);
    // milestone 30/31: メソッド表(struct名→メソッド名→Type::Fn)を構築する
    // (`declare_method`。milestone 31から宣言時の4診断つき)
    // (メソッド表と関数シグネチャは同じループで登録する——下記参照)
    // milestone 30: import alias(`import "mathutil"`→`mathutil`、`import "mesh/json"`→`json`)を
    // ANYとしてscopes[0]へ登録する。full_checkerはimport/パッケージを解決しないが、登録しないと
    // `mathutil.add(...)`のtarget `mathutil`が(変数でも組み込みでもないため)undefined-nameに
    // 誤検知される——milestone 30でMember/メソッド呼び出しがtargetを推論するようになり顕在化。
    // pkg修飾アクセス自体はtargetがANYになり素通り(pkg修飾の中身の検査は次段階)。
    // **既知の限界**: `declare()`を通さず直接insertするため、import aliasと同名のfn/constが
    // あると、後続のfn/const登録が`declare()`で`already-declared`になる(TS版は専用の
    // `name-conflicts-with-package`診断を出すが未移植)。その際そのfn/constの実型が
    // 登録されず呼び出しの引数照合が素通りする副作用もある——稀な衝突(import名と同名の
    // fn/const)につき次段階でname-conflicts-with-package診断とセットで対応する候補
    for (_, program) in files {
        for imp in &program.imports {
            ctx.scopes[0].insert(imp.alias.clone(), Binding { ty: ANY, mutable: false });
        }
    }
    // TS版`checker/modules.ts`のcheckPackageと同じ順序: 先に全関数の名前をscopes[0]へ
    // 登録してから(前方参照・相互再帰を許すため——本体はまだ検査しない)、トップレベル
    // 定数を検査+登録し、最後に関数本体を検査する。
    // milestone 26以降、非ジェネリックの自由関数は`fn_signature`でシグネチャ付き
    // (`Type::Fn`)で登録し、呼び出し側の個数・型照合(argument-count/type-mismatch)を
    // 効かせる。**ジェネリック関数(`type_params`有り)だけは従来どおりANYで登録**——
    // TS版はジェネリック呼び出しを別経路(`inferGenericCall`、型パラメータ推論つき)で
    // 扱い、引数不足なら`generic-inference-failed`を出す。full_checkerはこの推論を
    // 未実装なので、`Type::Fn`で登録するとTS版と違って`argument-count`を出してしまう
    // (診断コードのTS非互換)。ANY登録で呼び出しを丸ごと対象外にしておく。
    // `program.fns`にはstructのメソッド(`f.receiver.is_some()`)も自由関数と同じ配列で
    // 混在している——TS版`checkPackage`はレシーバ付きなら`declareMethod`で別の
    // `methodTable`へ登録し、`scopes[0]`(自由関数と同じ名前空間)には絶対に入れない
    // (`src/checker/functions.ts`のコメント「グローバルscopeには置かない」参照。
    // メソッドの名前空間は自由関数と完全分離——異なるstructが同名メソッドを持てる)。
    // structはmilestone 22/23とも対象外なので、メソッドは名前登録・本体検査どちらも
    // 単純にスキップする(誤って自由関数と同じ扱いにすると、別々のstructの同名メソッドが
    // already-declaredの誤検知になる)
    // **1ファイルにつき1回のループ**でメソッド表と自由関数シグネチャの両方を登録する
    // (TS版`checkPackage`も`for (const fn of program.fns)`の中で`declareMethod`と
    // `declareBinding`へ振り分ける)。メソッドと自由関数で2周に分けると、同じファイルに
    // 両方の宣言時診断があるとき**報告順がソース順と食い違う**(code reviewで発覚)
    for (i, (_, program)) in files.iter().enumerate() {
        let start = ctx.diagnostics.len();
        for f in &program.fns {
            if f.receiver.is_some() {
                declare_method(&mut ctx, f);
            } else {
                let ty = if f.type_params.is_empty() { fn_signature(&ctx.type_ctx, f) } else { ANY };
                ctx.declare(&f.name, ty, f.pos, false);
            }
        }
        spans.push((start, ctx.diagnostics.len(), i));
    }
    for (i, (_, program)) in files.iter().enumerate() {
        let start = ctx.diagnostics.len();
        for c in &program.consts {
            check_top_level_const(&mut ctx, c);
        }
        spans.push((start, ctx.diagnostics.len(), i));
    }
    // エントリポイント検査(TS版`checker/modules.ts`のrequireMain分岐)。TS版では
    // `requireMain = pkg === "main" && !testMode`だが、full_checkerは単一ファイル
    // (=mainパッケージ)専用でテストモードも未対応なので、ここでは常に要求する
    // (依存先パッケージ・`mesh test`はスコープ外——足すときにフラグ化する)。
    // レシーバ無しの`fn main`を探し、無ければ1:1(ファイル先頭)を指してmissing-main、
    // あれば引数ありor戻り値ありでinvalid-main-signature(エントリポイントは
    // 引数を取らず何も返さない)
    // **`require_main`が偽なら`fn main`の検査自体を丸ごと飛ばす**(TS版`checker/modules.ts`も
    // `if (opts.requireMain) { ... }`でブロック全体を包む)。有無だけを飛ばして形の検査を
    // 残すと、mainパッケージ以外にある`main`という名前のただの関数
    // (`export fn main(n: int) int`——importして`util.main(3)`と呼ぶのは正当)が
    // `invalid-main-signature`で弾かれる誤検知になる(code reviewで発覚・両CLIで再現確認)
    let main_start = ctx.diagnostics.len();
    // `fn main`を宣言しているファイル(TS版は`invalid-main-signature`をそのファイルに紐づける)。
    // 見つからない場合(missing-main)はパッケージ全体の性質なので先頭ファイルに属させる
    let main_file_idx = files.iter().position(|(_, p)| p.fns.iter().any(|f| f.name == "main" && f.receiver.is_none())).unwrap_or(0);
    match files.iter().flat_map(|(_, p)| p.fns.iter()).find(|f| f.name == "main" && f.receiver.is_none()) {
        _ if !require_main => {}
        None => ctx.error(
            Pos { line: 1, col: 1 },
            DiagnosticCode::MissingMain,
            "missing 'fn main()' — Mesh programs start from main",
        ),
        Some(main) => {
            if !main.params.is_empty() || main.ret.is_some() {
                ctx.error(
                    main.pos,
                    DiagnosticCode::InvalidMainSignature,
                    "'fn main()' must take no parameters and return nothing",
                );
            }
        }
    }
    // milestone 31: メソッド本体もここで検査する(TS版`checkPackage`の
    // `for (const fn of program.fns) checkFn(ctx, fn)`と同じ——レシーバの有無で分けない)。
    // milestone 30まではメソッド本体を丸ごとスキップしていたため、メソッドの中の
    // 未定義名・型不一致・引数不一致が一切検出されない大きな穴になっていた
    // (`check_fn`がレシーバをスコープへ宣言するようになったのとセット)
    spans.push((main_start, ctx.diagnostics.len(), main_file_idx));
    for (i, (_, program)) in files.iter().enumerate() {
        let start = ctx.diagnostics.len();
        for f in &program.fns {
            check_fn(&mut ctx, f);
        }
        spans.push((start, ctx.diagnostics.len(), i));
    }

    // 区間をファイルごとに畳み直す(ファイルの並び順を保つ)
    let mut per_file: Vec<(String, Vec<Diagnostic>)> = files.iter().map(|(f, _)| (f.clone(), Vec::new())).collect();
    for (start, end, i) in spans {
        for d in &ctx.diagnostics[start..end] {
            per_file[i].1.push(d.clone());
        }
    }
    per_file.retain(|(_, d)| !d.is_empty());
    per_file
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse;

    fn check(src: &str) -> Vec<Diagnostic> {
        let program = parse(src).expect("test source must parse");
        check_program(&program)
    }

    // codegen経路と同じくjson structのデコーダを合成してから検査する(mesh checkのrun_checkが
    // やるのと同じ前処理)。json struct由来の合成関数を検査対象に含めたいテスト用
    fn check_with_json(src: &str) -> Vec<Diagnostic> {
        let mut program = parse(src).expect("test source must parse");
        crate::json_decode::synthesize_json_decoders(&mut program).expect("synthesis must succeed");
        check_program(&program)
    }

    #[test]
    fn 正しいスカラープログラムは診断を出さない() {
        let diags = check(
            "fn helper() int { return 42 }\n\
             fn main() {\n\
                 mut total := helper()\n\
                 total = total + 1\n\
                 if total > 0 {\n\
                     x := \"ok\"\n\
                     print(x)\n\
                 }\n\
             }\n",
        );
        assert_eq!(diags, vec![]);
    }


    // --- milestone 39: match / select -------------------------------------------------

    #[test]
    fn matchのアーム本体を走査する() {
        // このmilestoneの本題。今までアーム本体は`_ => ANY`で素通りしていた
        let diags = check("fn f() int | error {\n    return 1\n}\n\nfn main() {\n    r := f()\n    print(match r {\n        int => \"${bogus}\"\n        error => \"err\"\n    })\n}\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, DiagnosticCode::UndefinedName);
    }

    #[test]
    fn アーム内でsubjectが絞り込まれる() {
        // `int`アームの中では r は int(unionのまま扱うと invalid-operation を見落とす)
        let diags = check("fn f() int | error {\n    return 1\n}\n\nfn main() {\n    r := f()\n    n := match r {\n        int => r + \"x\"\n        error => 0\n    }\n    print(str(n))\n}\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, DiagnosticCode::InvalidOperation);
        // 正しく書けば無診断
        assert_eq!(check("fn f() int | error {\n    return 1\n}\n\nfn main() {\n    r := f()\n    n := match r {\n        int => r + 1\n        error => 0\n    }\n    print(str(n))\n}\n"), vec![]);
    }

    #[test]
    fn matchの網羅性を検査する() {
        let diags = check("fn f() int | error {\n    return 1\n}\n\nfn main() {\n    r := f()\n    print(match r {\n        int => \"int\"\n    })\n}\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, DiagnosticCode::MatchNotExhaustive);
        assert_eq!(diags[0].message, "match is not exhaustive — missing: error (add arms for them, or a '_' arm)");
        // `_`があれば網羅
        assert_eq!(check("fn f() int | error {\n    return 1\n}\n\nfn main() {\n    r := f()\n    print(match r {\n        int => \"int\"\n        _ => \"rest\"\n    })\n}\n"), vec![]);
    }

    #[test]
    fn 到達しないパターンを報告する() {
        // unionに存在しない型
        let impossible = check("fn f() int | error {\n    return 1\n}\n\nfn main() {\n    r := f()\n    print(match r {\n        int => \"int\"\n        string => \"s\"\n        error => \"e\"\n    })\n}\n");
        assert_eq!(impossible.len(), 1);
        assert_eq!(impossible[0].code, DiagnosticCode::ImpossiblePattern);
        assert_eq!(impossible[0].message, "int | error can never be string");
        // 同じパターンの重複
        let dup = check("fn f() int | error {\n    return 1\n}\n\nfn main() {\n    r := f()\n    print(match r {\n        int => \"int\"\n        int => \"again\"\n        error => \"e\"\n    })\n}\n");
        assert_eq!(dup.len(), 1);
        assert_eq!(dup[0].code, DiagnosticCode::UnreachablePattern);
        // `_`の後ろのアーム
        let after = check("fn f() int | error {\n    return 1\n}\n\nfn main() {\n    r := f()\n    print(match r {\n        _ => \"any\"\n        int => \"int\"\n    })\n}\n");
        assert_eq!(after.len(), 1);
        assert_eq!(after[0].code, DiagnosticCode::UnreachablePattern);
    }

    #[test]
    fn ワイルドカードは単独でなければならない() {
        let diags = check("fn f() int | error {\n    return 1\n}\n\nfn main() {\n    r := f()\n    print(match r {\n        int, _ => \"int\"\n    })\n}\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, DiagnosticCode::WildcardNotAlone);
    }

    #[test]
    fn union以外をmatchするとunion_requiredになる() {
        let diags = check("fn main() {\n    x := 1\n    print(match x {\n        int => \"int\"\n    })\n}\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, DiagnosticCode::UnionRequired);
        assert_eq!(diags[0].message, "match subject must be a union type, got int");
    }

    #[test]
    fn 値のアームとvoidのアームは混ぜられない() {
        let diags = check("fn f() int | error {\n    return 1\n}\n\nfn main() {\n    x := match f() {\n        int => 1\n        error => print(\"boom\")\n    }\n    print(str(x))\n}\n");
        assert!(diags.iter().any(|d| d.code == DiagnosticCode::MixedVoidArms), "{diags:?}");
        // 全アームvoidなら文として使えて無診断
        assert_eq!(check("fn f() int | error {\n    return 1\n}\n\nfn main() {\n    r := f()\n    match r {\n        int => print(\"int\")\n        error => print(\"err\")\n    }\n}\n"), vec![]);
    }

    #[test]
    fn selectのアーム本体も走査する() {
        // 受信値は`T | closed`として束縛され、本体の中の未定義名も見つかる
        let diags = check("fn main() {\n    ch := chan<int>(1)\n    select {\n        v := <-ch => print(str(bogus))\n    }\n}\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, DiagnosticCode::UndefinedName);
        // チャネルでないものを待つ
        let not_chan = check("fn main() {\n    n := 1\n    select {\n        v := <-n => print(\"got\")\n    }\n}\n");
        assert_eq!(not_chan.len(), 1);
        assert_eq!(not_chan[0].code, DiagnosticCode::NotAChannel);
    }


    #[test]
    fn 不可能なパターンのアーム本体では追加の診断を出さない() {
        // 回帰(code reviewで発覚): 死んだアームにsubject(unionのまま)を配っていたため、
        // 本体の`r + 1`がTS版に無い`invalid-operation`の誤検知になっていた。
        // TS版と同じくANYへ絞り、impossible-patternだけを報告する
        let diags = check("fn f() int | error {\n    return 1\n}\n\nfn main() {\n    r := f()\n    match r {\n        string => print(str(r + 1))\n        int => print(\"int\")\n        error => print(\"err\")\n    }\n}\n");
        assert_eq!(diags.len(), 1, "{diags:?}");
        assert_eq!(diags[0].code, DiagnosticCode::ImpossiblePattern);
    }

    #[test]
    fn 関数値をmatchしてもunion_requiredを出す() {
        // 回帰(code reviewで発覚): `is_fully_modeled`で門番していたため、関数型のように
        // **正確に分かっている**型まで見逃していた
        let diags = check("fn add(a: int, b: int) int {\n    return a + b\n}\n\nfn main() {\n    f := add\n    match f {\n        int => print(\"int\")\n    }\n}\n");
        assert_eq!(diags.len(), 1, "{diags:?}");
        assert_eq!(diags[0].code, DiagnosticCode::UnionRequired);
        assert_eq!(diags[0].message, "match subject must be a union type, got fn(int, int) int");
    }

    #[test]
    fn mutなsubjectは絞り込まない() {
        // 回帰(milestone 34のcode reviewと同じ理由): 再代入されうる変数を絞り込むと、
        // 絞り込んだ型が居座って後続の正当な再代入を誤って弾く
        assert_eq!(
            check("fn f() int | error {\n    return 1\n}\n\nfn main() {\n    mut r := f()\n    print(match r {\n        int => \"int\"\n        error => \"err\"\n    })\n    r = f()\n}\n"),
            vec![]
        );
    }

    #[test]
    fn subjectがany縮退しているときはパターンで絞り込まない() {
        // 回帰(実装中にexamples/discriminated_union.meshで発覚): pkg修飾型など
        // full_checkerがANYへ縮退させる型をmatchすると、`{ kind: "ok" }`のような部分構造
        // パターンで絞り込んでしまい、アーム本体の`r.user`がunknown-fieldの誤検知になっていた。
        // ここでは「型注釈がANYになる」形(未知の型名)で同じ状況を作る
        let diags = check("fn main() {\n    r := unknownThing()\n    print(match r {\n        { kind: \"ok\" } => \"${r.user}\"\n        _ => \"other\"\n    })\n}\n");
        // undefined-name(unknownThing)だけが出る——r.userのunknown-fieldは出ない
        assert_eq!(diags.len(), 1, "{diags:?}");
        assert_eq!(diags[0].code, DiagnosticCode::UndefinedName);
    }

    #[test]
    fn 解決できないunionをmatchしてもパニックしない() {
        // 回帰: 裸のunion循環があると`resolve_type_decls`が失敗し、レジストリには
        // 中身が空のunionが残る。そこへ`type_equals`/`union_of`を通すと`.expect()`で
        // パニックする(milestone 37で`mesh test`が同じ形でクラッシュした)。
        // 判定できないので診断は出さない(検出漏れ側)
        let diags = check("type A = B | int\ntype B = A | string\n\nfn f() A {\n    return 1\n}\n\nfn main() {\n    r := f()\n    print(match r {\n        int => \"int\"\n        _ => \"other\"\n    })\n    if r is int {\n        print(\"int\")\n    }\n}\n");
        assert!(diags.iter().all(|d| d.code != DiagnosticCode::MatchNotExhaustive), "{diags:?}");
    }

    #[test]
    fn 未定義の変数参照はundefined_nameを報告する() {
        let diags = check("fn main() {\n    print(missing)\n}\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, DiagnosticCode::UndefinedName);
    }

    #[test]
    fn mutなしの変数への再代入はimmutable_assignmentを報告する() {
        let diags = check("fn main() {\n    total := 1\n    total = 2\n}\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, DiagnosticCode::ImmutableAssignment);
    }

    #[test]
    fn 同じスコープでの再宣言はalready_declaredを報告する() {
        let diags = check("fn main() {\n    x := 1\n    x := 2\n}\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, DiagnosticCode::AlreadyDeclared);
    }

    #[test]
    fn 外側スコープの名前を再利用するとshadowingを報告する() {
        let diags = check("fn main() {\n    x := 1\n    if true {\n        x := 2\n    }\n}\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, DiagnosticCode::Shadowing);
    }

    #[test]
    fn 型注釈と値の不一致はtype_mismatchを報告する() {
        let diags = check("fn main() {\n    x: int = \"hi\"\n}\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, DiagnosticCode::TypeMismatch);
    }

    #[test]
    fn 再代入時の型不一致もtype_mismatchを報告する() {
        let diags = check("fn main() {\n    mut x := 1\n    x = \"hi\"\n}\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, DiagnosticCode::TypeMismatch);
    }

    #[test]
    fn 予約語での宣言はreserved_wordを報告する() {
        let diags = check("fn main() {\n    await := 1\n}\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, DiagnosticCode::ReservedWord);
    }

    #[test]
    fn 組み込み関数名での宣言はbuiltin_redeclaredを報告する() {
        let diags = check("fn main() {\n    print := 1\n}\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, DiagnosticCode::BuiltinRedeclared);
    }

    #[test]
    fn intはfloatへ暗黙に広げられ型不一致にならない() {
        let diags = check("fn main() {\n    x: float = 1\n}\n");
        assert_eq!(diags, vec![]);
    }

    #[test]
    fn トップレベル関数名を再宣言するとshadowingになる() {
        let diags = check("fn helper() int { return 1 }\nfn main() {\n    helper := 5\n    print(helper)\n}\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, DiagnosticCode::Shadowing);
    }

    #[test]
    fn 重複したトップレベル関数宣言はalready_declaredになる() {
        let diags = check("fn helper() int { return 1 }\nfn helper() int { return 2 }\nfn main() {\n    print(helper())\n}\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, DiagnosticCode::AlreadyDeclared);
    }

    #[test]
    fn 組み込み関数名のトップレベル関数宣言はbuiltin_redeclaredになる() {
        let diags = check("fn print() {\n}\nfn main() {\n}\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, DiagnosticCode::BuiltinRedeclared);
    }

    #[test]
    fn 予約語のトップレベル関数宣言はreserved_wordになる() {
        let diags = check("fn await() {\n}\nfn main() {\n}\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, DiagnosticCode::ReservedWord);
    }

    #[test]
    fn トップレベル定数とトップレベル関数の名前衝突もalready_declaredになる() {
        let diags = check("helper := 1\nfn helper() int { return 2 }\nfn main() {\n    print(helper)\n}\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, DiagnosticCode::AlreadyDeclared);
    }

    #[test]
    fn 異なるstructの同名メソッドはalready_declaredの誤検知にならない() {
        // 回帰テスト: program.fnsには自由関数とstructのメソッド(receiver付き)が
        // 同じ配列に混在している。メソッドの名前空間は自由関数と完全分離(TS版
        // `checker/functions.ts`の`declareMethod`はscopes[0]に触れない)なので、
        // 異なるstructが同名メソッドを持つのは正当。フィルタし忘れるとここが
        // already-declaredの誤検知になる
        let diags = check(
            "struct Todo {\n    title: string\n}\n\nfn (t: Todo) describe() string {\n    return t.title\n}\n\nstruct User {\n    name: string\n}\n\nfn (u: User) describe() string {\n    return u.name\n}\n\nfn main() {\n}\n",
        );
        assert_eq!(diags, vec![]);
    }

    #[test]
    fn ローカル変数がメソッド名と同じでもshadowingの誤検知にならない() {
        let diags = check(
            "struct Todo {\n    title: string\n}\n\nfn (t: Todo) describe() string {\n    return t.title\n}\n\nfn main() {\n    describe := 5\n    print(describe)\n}\n",
        );
        assert_eq!(diags, vec![]);
    }

    #[test]
    fn トップレベル定数の型が参照側でも正しく伝播する() {
        // milestone 22時点ではトップレベル定数を参照する式は常にANYへフォールバック
        // していたが、milestone 23でconstをctx.declare()経由で本物のBindingとして
        // 登録するようになった副次効果で、参照側でも実際の型と照合されるようになった
        let diags = check("limit: int = 10\nfn main() {\n    x: string = limit\n}\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, DiagnosticCode::TypeMismatch);
    }

    #[test]
    fn 戻り値の型不一致はtype_mismatchを報告する() {
        // main無しだとmissing-mainが上乗せされるため、type-mismatchを切り出せるよう
        // 空のmainを添える(TS版でもmain無し単一ファイルはmissing-mainを併発する)
        let diags = check("fn helper() int {\n    return \"oops\"\n}\nfn main() {}\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, DiagnosticCode::TypeMismatch);
    }

    #[test]
    fn トップレベル定数の型不一致もtype_mismatchを報告する() {
        let diags = check("x: int = \"oops\"\nfn main() {\n    print(x)\n}\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, DiagnosticCode::TypeMismatch);
    }

    #[test]
    fn トップレベル定数内の未定義名もundefined_nameを報告する() {
        let diags = check("x := missing\nfn main() {\n    print(x)\n}\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, DiagnosticCode::UndefinedName);
    }

    #[test]
    fn 比較演算子の結果はboolになり型不一致を誤検知しない() {
        // 回帰テスト: Binaryの結果型を「左辺の型をそのまま伝播」で済ませると、
        // `a > b`(int同士の比較)がintのまま返ってしまい、boolへの代入が
        // type-mismatchの誤検知になっていた
        let diags = check("fn main() {\n    ok: bool = 1 > 0\n    print(ok)\n}\n");
        assert_eq!(diags, vec![]);
    }

    #[test]
    fn 論理演算子の結果もboolになり型不一致を誤検知しない() {
        let diags = check("fn main() {\n    mut ok := true\n    ok = 1 > 0 && 2 > 1\n}\n");
        assert_eq!(diags, vec![]);
    }

    #[test]
    fn トップレベル関数呼び出しはundefined_nameを誤検知しない() {
        // 回帰テスト: トップレベル関数名をscopeに登録し忘れると、他の関数を呼ぶだけの
        // 正しいコードがundefined-nameになってしまう
        let diags = check("fn helper() int { return 1 }\nfn main() {\n    x := helper()\n    print(x)\n}\n");
        assert_eq!(diags, vec![]);
    }

    #[test]
    fn 関数本体でパラメータ名を再宣言するとshadowingになる() {
        // 回帰テスト: パラメータのスコープと本体のスコープを1段に潰すと、本体側の
        // 再宣言が(shadowingではなく)already-declaredに誤判定されていた
        let diags = check("fn f(x: int) {\n    x := 5\n    print(x)\n}\nfn main() {\n    f(1)\n}\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, DiagnosticCode::Shadowing);
    }

    #[test]
    fn forボディでヘッダのinit変数を再宣言するとshadowingになる() {
        let diags = check("fn main() {\n    for i := 0; i < 3; i++ {\n        i := 5\n        print(i)\n    }\n}\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, DiagnosticCode::Shadowing);
    }

    #[test]
    fn range_forボディでヘッダの変数を再宣言するとshadowingになる() {
        let diags = check("fn main() {\n    for i := range 3 {\n        i := 5\n        print(i)\n    }\n}\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, DiagnosticCode::Shadowing);
    }

    #[test]
    fn forヘッダのinit変数はbody内から参照できる() {
        let diags = check("fn main() {\n    for i := 0; i < 3; i++ {\n        print(i)\n    }\n}\n");
        assert_eq!(diags, vec![]);
    }

    #[test]
    fn main無しはmissing_mainを報告する() {
        let diags = check("fn helper() int { return 1 }\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, DiagnosticCode::MissingMain);
        // ファイル先頭(1:1)を指す
        assert_eq!(diags[0].pos, Pos { line: 1, col: 1 });
    }

    #[test]
    fn mainに引数があるとinvalid_main_signatureを報告する() {
        let diags = check("fn main(x: int) {\n    print(x)\n}\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, DiagnosticCode::InvalidMainSignature);
    }

    #[test]
    fn mainに戻り値があるとinvalid_main_signatureを報告する() {
        let diags = check("fn main() int {\n    return 0\n}\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, DiagnosticCode::InvalidMainSignature);
    }

    #[test]
    fn 正しい空のmainはエントリポイント診断を出さない() {
        let diags = check("fn main() {}\n");
        assert_eq!(diags, vec![]);
    }

    #[test]
    fn レシーバ付きのmainはエントリポイントとみなされずmissing_mainになる() {
        // メソッドの名前空間は自由関数と完全分離——`fn (r: R) main()`は
        // エントリポイントの`main`ではない(TS版も`!f.receiver`で除外する)。
        // structはfull_checkerのスコープ外なのでメソッド本体は検査されないが、
        // 名前だけは「自由関数のmainが無い」と判定されmissing-mainになるべき
        let diags = check("fn (r: R) main() {}\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, DiagnosticCode::MissingMain);
    }

    // ---- milestone 25: 演算子の妥当性検査 ----

    #[test]
    fn 型不一致な算術はinvalid_operationを報告する() {
        let diags = check("fn main() {\n    x := true - false\n    print(x)\n}\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, DiagnosticCode::InvalidOperation);
        assert_eq!(diags[0].message, "invalid operation: bool - bool");
    }

    #[test]
    fn 文字列連結でない加算のstrヒント() {
        // op=='+' かつ片側がstring**本体**のときだけTS版はstr()ヒントを添える。
        // 文字列リテラル("a")はリテラル型でtype_equals(_, STRING)がfalseなのでヒント無し
        // (TS版も同じ挙動——mut宣言でwidenされたstring型のときだけヒントが出る)。
        // ここではmut変数(string型)を使ってヒントが出る側を検証する
        let diags = check("fn main() {\n    mut m := \"b\"\n    x := 1 + m\n    print(x)\n}\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, DiagnosticCode::InvalidOperation);
        assert_eq!(diags[0].message, "invalid operation: int + string (hint: use str() to convert values to string)");
    }

    #[test]
    fn 加算の右辺が文字列リテラルのときはヒントを出さない() {
        // 上のテストの対(リテラルはstring本体ではないのでヒント無し、TS版と一致)
        let diags = check("fn main() {\n    x := 1 + \"a\"\n    print(x)\n}\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].message, "invalid operation: int + \"a\"");
    }

    #[test]
    fn 比較不能な型の比較はincomparable_typesを報告する() {
        // 文字列リテラルの型は`string`ではなく`"a"`と表示される(TS版と一致)
        let diags = check("fn main() {\n    b := 1 < \"a\"\n    print(b)\n}\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, DiagnosticCode::IncomparableTypes);
        assert_eq!(diags[0].message, "cannot compare int with \"a\"");
    }

    #[test]
    fn 比較不能な型の等価比較もincomparable_typesを報告する() {
        let diags = check("fn main() {\n    b := 1 == \"a\"\n    print(b)\n}\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, DiagnosticCode::IncomparableTypes);
    }

    #[test]
    fn noneとの等価比較はuse_is_noneを報告する() {
        let diags = check("fn main() {\n    if 1 == none {\n        print(\"x\")\n    }\n}\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, DiagnosticCode::UseIsNone);
    }

    #[test]
    fn bool以外への論理演算子はnot_boolを報告する() {
        let diags = check("fn main() {\n    b := 1 && true\n    print(b)\n}\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, DiagnosticCode::NotBool);
        assert_eq!(diags[0].message, "'&&' requires bool operands, got int");
    }

    #[test]
    fn 単項否定のbool以外はnot_boolを報告する() {
        let diags = check("fn main() {\n    b := !5\n    print(b)\n}\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, DiagnosticCode::NotBool);
        assert_eq!(diags[0].message, "'!' requires bool, got int");
    }

    #[test]
    fn 単項マイナスのnumeric以外はinvalid_operationを報告する() {
        let diags = check("fn main() {\n    x := -\"a\"\n    print(x)\n}\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, DiagnosticCode::InvalidOperation);
        assert_eq!(diags[0].message, "unary '-' requires int or float, got \"a\"");
    }

    #[test]
    fn numeric以外のインクリメントはinvalid_operationを報告する() {
        let diags = check("fn main() {\n    mut s := \"a\"\n    s++\n}\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, DiagnosticCode::InvalidOperation);
        assert_eq!(diags[0].message, "'++' requires int or float, got string");
    }

    #[test]
    fn リテラルのゼロ除算はdivision_by_zeroを報告する() {
        let diags = check("fn main() {\n    x := 1 / 0\n    print(x)\n}\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, DiagnosticCode::DivisionByZero);
        assert_eq!(diags[0].message, "integer division by zero");
    }

    #[test]
    fn リテラルのゼロ剰余もdivision_by_zeroを報告する() {
        let diags = check("fn main() {\n    x := 1 % 0\n    print(x)\n}\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, DiagnosticCode::DivisionByZero);
        assert_eq!(diags[0].message, "integer modulo by zero");
    }

    #[test]
    fn forヘッダの文字列変数はwidenされ誤検知しない() {
        // 回帰(code reviewで発見): milestone 25でExpr::StringをType::Literalにした際、
        // ShortVarDeclはmut時にwiden_literalしたがcheck_for_init(常にmutableなforヘッダ
        // 変数)を見落としていた。widenしないとsがLiteral("a")のままになり、`s != "z"`が
        // incomparable-types、`s = "b"`がtype-mismatchの誤検知になる(TS版は無診断)
        let diags = check("fn main() {\n    for s := \"a\"; s != \"z\"; s = \"b\" {\n        print(s)\n    }\n}\n");
        assert_eq!(diags, vec![]);
    }

    #[test]
    fn 正当な演算は診断を出さない() {
        // 算術・文字列連結・浮動小数点・論理混在・比較・単項・mut文字列再代入・inc/decを一通り
        let diags = check(
            "fn main() {\n\
            \x20   a := 1 + 2 * 3\n\
            \x20   b := \"x\" + \"y\"\n\
            \x20   c := 1.5 / 2.0\n\
            \x20   d := true && false || !true\n\
            \x20   e := a < 10\n\
            \x20   f := b < \"z\"\n\
            \x20   mut s := \"hi\"\n\
            \x20   s = \"bye\"\n\
            \x20   mut n := 0\n\
            \x20   n++\n\
            \x20   n = -n\n\
            \x20   print(\"${a} ${c} ${d} ${e} ${f} ${s} ${n}\")\n\
            }\n",
        );
        assert_eq!(diags, vec![]);
    }

    // ---- milestone 26: argument-count(ユーザー定義関数の引数個数・型) ----

    #[test]
    fn 引数不足はargument_countを報告する() {
        let diags = check("fn add(a: int, b: int) int {\n    return a + b\n}\nfn main() {\n    x := add(1)\n    print(x)\n}\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, DiagnosticCode::ArgumentCount);
        assert_eq!(diags[0].message, "expected 2 argument(s), got 1");
    }

    #[test]
    fn 引数過多もargument_countを報告する() {
        let diags = check("fn greet(name: string) {\n    print(name)\n}\nfn main() {\n    greet(\"a\", \"b\")\n}\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, DiagnosticCode::ArgumentCount);
        assert_eq!(diags[0].message, "expected 1 argument(s), got 2");
    }

    #[test]
    fn 引数の型不一致はtype_mismatchを報告する() {
        let diags = check("fn sq(n: int) int {\n    return n * n\n}\nfn main() {\n    x := sq(\"hi\")\n    print(x)\n}\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, DiagnosticCode::TypeMismatch);
        assert_eq!(diags[0].message, "argument 1: cannot use \"hi\" as int");
    }

    #[test]
    fn 正しい引数の呼び出しは診断を出さない() {
        let diags = check("fn add(a: int, b: int) int {\n    return a + b\n}\nfn main() {\n    print(\"${add(2, 3)}\")\n}\n");
        assert_eq!(diags, vec![]);
    }

    #[test]
    fn 前方参照の呼び出しでも個数検査が効く() {
        // 本体検査より前に全関数を登録するので、後で定義する関数の呼び出しも照合できる
        let diags = check("fn main() {\n    later(1, 2)\n}\nfn later(a: int) {\n    print(a)\n}\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, DiagnosticCode::ArgumentCount);
    }

    #[test]
    fn 関数の戻り値型が呼び出し側に伝播する() {
        // addはintを返すのでstringへの代入はtype-mismatch(呼び出し結果がANYに潰れない)
        let diags = check("fn add(a: int, b: int) int {\n    return a + b\n}\nfn main() {\n    x: string = add(1, 2)\n    print(x)\n}\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, DiagnosticCode::TypeMismatch);
    }

    #[test]
    fn 組み込み関数の呼び出しはargument_count対象外で誤検知しない() {
        // milestone 27で組み込みはintercept経由で検査するようになったが、printは可変長
        // なのでarity検査せず、複数引数でも誤検知しない(milestone 26時点では「calleeがANY」
        // が理由だったが、milestone 27でinfer_builtin_callがprintをVOIDとして素通しする形に変わった)
        let diags = check("fn main() {\n    print(1, 2, 3)\n}\n");
        assert_eq!(diags, vec![]);
    }

    #[test]
    fn ジェネリック関数の呼び出しはargument_count対象外で誤検知しない() {
        // ジェネリック関数はANY登録なので個数検査されない——TS版は引数不足時に
        // generic-inference-failedを出すが、full_checkerは型パラメータ推論が未実装のため
        // 意図的に対象外(argument-countをTS非互換に出してしまうのを避ける)
        let diags = check("fn identity<T>(x: T) T {\n    return x\n}\nfn main() {\n    identity(1, 2, 3)\n    print(1)\n}\n");
        assert_eq!(diags, vec![]);
    }

    #[test]
    fn ローカル変数に束縛した関数値の呼び出しも個数検査される() {
        // `f := add` で f の型は Type::Fn として伝播するので、f(1) は argument-count
        // (TS版 checkCallOfValue も同じ経路——値呼び出しは対象外ではない)
        let diags = check("fn add(a: int, b: int) int {\n    return a + b\n}\nfn main() {\n    f := add\n    x := f(1)\n    print(x)\n}\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, DiagnosticCode::ArgumentCount);
    }

    // ---- milestone 27: 組み込み関数の検査 ----

    #[test]
    fn 組み込み関数の個数不一致はargument_countを報告する() {
        let diags = check("fn main() {\n    x := str(1, 2)\n    print(x)\n}\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, DiagnosticCode::ArgumentCount);
        assert_eq!(diags[0].message, "str() expects 1 argument(s), got 2");
    }

    #[test]
    fn lenへの非文字列スカラーはbuiltin_arg_typeを報告する() {
        let diags = check("fn main() {\n    x := len(5)\n    print(x)\n}\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, DiagnosticCode::BuiltinArgType);
        assert_eq!(diags[0].message, "len() requires string, array or map, got int");
    }

    #[test]
    fn roundへの非floatはbuiltin_arg_typeを報告する() {
        let diags = check("fn main() {\n    x := round(3)\n    print(x)\n}\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, DiagnosticCode::BuiltinArgType);
        assert_eq!(diags[0].message, "round() requires a float, got int");
    }

    #[test]
    fn コレクション組み込みにスカラーを渡すとbuiltin_arg_typeを報告する() {
        // contains(5, x): 第1引数が配列でなく具体スカラー→"requires an array"(TS版と一致)
        let diags = check("fn main() {\n    b := contains(5, 3)\n    print(b)\n}\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, DiagnosticCode::BuiltinArgType);
        assert_eq!(diags[0].message, "contains() requires an array, got int");
    }

    #[test]
    fn 正しい組み込み呼び出しは診断を出さない() {
        // 文字列組み込み・可変長print・スカラー戻り値の合成
        let diags = check("fn main() {\n    s := trim(\"  hi  \")\n    u := upper(s)\n    parts := split(u, \",\")\n    print(len(s), u, parts)\n}\n");
        assert_eq!(diags, vec![]);
    }

    #[test]
    fn 配列map引数の組み込みは誤検知しない() {
        // milestone 33/34でコレクションをモデル化した後は「ANYに潰れるから無診断」ではなく、
        // `xs: int[]`に対して要素型まで合っているから無診断
        let diags = check("fn main() {\n    mut xs: int[] = [1, 2, 3]\n    push(xs, 4)\n    b := contains(xs, 2)\n    n := len(xs)\n    print(\"${b} ${n}\")\n}\n");
        assert_eq!(diags, vec![]);
    }

    #[test]
    fn 組み込み引数の未定義名も検出する() {
        // arityが合っていても引数式は推論されるので未定義名を拾う
        let diags = check("fn main() {\n    x := str(missing)\n    print(x)\n}\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, DiagnosticCode::UndefinedName);
    }

    #[test]
    fn any由来の文字列を文字列組み込みへ渡しても誤検知しない() {
        // 回帰(code reviewで発覚): ジェネリック関数の結果(ANYに潰れる)は実際にはstringでも
        // full_checkerではANY。trim/toInt/split/joinの文字列引数検査がANYを免除していなかったため、
        // `trim(s)`(sはANY由来の実string)を誤ってbuiltin-arg-typeにしていた。TS版は無診断。
        let diags = check("fn identity<T>(x: T) T {\n    return x\n}\nfn main() {\n    s := identity(\"hi\")\n    t := trim(s)\n    parts := split(s, \",\")\n    n := toInt(s)\n    print(\"${t} ${parts} ${n}\")\n}\n");
        assert_eq!(diags, vec![]);
    }

    #[test]
    fn reduceの戻り値はaccumulator型になり後続の型検査に効く() {
        // 回帰(code reviewで発覚): reduceは常にANYを返していたが、accumulator型は
        // callback(Type::Fn、milestone 26で追跡済み)から計算できるスカラー。
        // reduce(xs, add, 0)→int を round に渡すとTS版と同じくbuiltin-arg-typeが出る
        let diags = check("fn add(acc: int, x: int) int {\n    return acc + x\n}\nfn main() {\n    mut xs: int[] = [1, 2, 3]\n    total := reduce(xs, add, 0)\n    y := round(total)\n    print(y)\n}\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, DiagnosticCode::BuiltinArgType);
        assert_eq!(diags[0].message, "round() requires a float, got int");
    }

    // ---- milestone 28: mesh checkでもjson structデコーダを合成する ----

    #[test]
    fn json構造体の合成デコーダ呼び出しは合成後undefined_nameにならない() {
        // 生成デコーダは json.field 等を呼ぶので mesh/json のimportが要る(合成の前提)
        let src = "import \"mesh/json\"\njson struct User { name: string }\nfn main() {\n    u := decodeUser(5)\n    print(u)\n}\n";
        // 合成しない生の検査では decodeUser が未定義扱い(バグの再現)
        let raw = check(src);
        assert!(raw.iter().any(|d| d.code == DiagnosticCode::UndefinedName), "合成前はundefined-nameになるはず");
        // 合成後(run_check/codegenと同じ前処理)は decodeUser が登録され誤検知しない
        let synthed = check_with_json(src);
        assert_eq!(synthed, vec![]);
    }

    // ---- milestone 29: 名前付きstructリテラルのフィールド検証 ----

    const USER: &str = "struct User {\n    name: string\n    age: int\n}\n";

    #[test]
    fn 未知のフィールドはunknown_fieldを報告する() {
        // typo `nmae` は unknown-field と(name欠落による)missing-fields の2件を出す
        // ——診断を積んで継続する(TS版とbyte-for-byte一致、実機確認済み)
        let diags = check(&format!("{USER}fn main() {{\n    u := User{{nmae: \"a\", age: 1}}\n    print(u)\n}}\n"));
        assert_eq!(diags.len(), 2);
        assert_eq!(diags[0].code, DiagnosticCode::UnknownField);
        assert_eq!(diags[0].message, "User has no field 'nmae' (fields: name, age)");
        assert_eq!(diags[1].code, DiagnosticCode::MissingFields);
    }

    #[test]
    fn フィールド値の型不一致はtype_mismatchを報告する() {
        let diags = check(&format!("{USER}fn main() {{\n    u := User{{name: 5, age: 1}}\n    print(u)\n}}\n"));
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, DiagnosticCode::TypeMismatch);
        assert_eq!(diags[0].message, "field 'name': cannot use int as string");
    }

    #[test]
    fn フィールド欠落はmissing_fieldsを報告する() {
        let diags = check(&format!("{USER}fn main() {{\n    u := User{{name: \"a\"}}\n    print(u)\n}}\n"));
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, DiagnosticCode::MissingFields);
        assert_eq!(diags[0].message, "missing field(s) in User: age");
    }

    #[test]
    fn 重複フィールドはduplicate_fieldを報告する() {
        let diags = check(&format!("{USER}fn main() {{\n    u := User{{name: \"a\", name: \"b\", age: 1}}\n    print(u)\n}}\n"));
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, DiagnosticCode::DuplicateField);
    }

    #[test]
    fn 正しいstructリテラルは診断を出さない() {
        let diags = check(&format!("{USER}fn main() {{\n    u := User{{name: \"a\", age: 1}}\n    print(u)\n}}\n"));
        assert_eq!(diags, vec![]);
    }

    #[test]
    fn struct型はコレクション組み込みで正しく弾かれる() {
        // milestone 27のinvariant(コレクションはANY)を保ったまま、structは具体型として
        // len等に弾かれる(TS版と一致)——struct modelingがbuiltinを壊さないことの回帰
        let diags = check(&format!("{USER}fn main() {{\n    n := len(User{{name: \"a\", age: 1}})\n    print(n)\n}}\n"));
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, DiagnosticCode::BuiltinArgType);
        assert_eq!(diags[0].message, "len() requires string, array or map, got User");
    }

    #[test]
    fn struct型のパラメータ不一致はtype_mismatchを報告する() {
        // milestone 26のargument checkがstruct型パラメータでも効く(paramがANYでなくなった)
        let diags = check(&format!("{USER}fn greet(u: User) {{\n    print(u.name)\n}}\nfn main() {{\n    greet(5)\n}}\n"));
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, DiagnosticCode::TypeMismatch);
        assert_eq!(diags[0].message, "argument 1: cannot use int as User");
    }

    #[test]
    fn 関数型やコレクション型のフィールドは誤検知しない() {
        // 回帰(code reviewで発覚): `cb: fn(int[]) int`のフィールドに名前付き関数を代入すると、
        // full_checkerのfn_signatureはparamをANYに縮退する一方、レジストリ側の宣言型は
        // int[]を具体解決するため、Fn{[any],int} vs Fn{[int[]],int}で誤ってtype-mismatchに
        // なっていた。縮退する型(配列/map/chan/fn/union)のフィールドは検査スキップで解消
        let src = "struct Box {\n    cb: fn(int[]) int\n    items: int[]\n}\nfn sum(xs: int[]) int {\n    return 0\n}\nfn main() {\n    b := Box{cb: sum, items: [1, 2]}\n    print(b)\n}\n";
        assert_eq!(check(src), vec![]);
    }

    #[test]
    fn structのメソッド呼び出しとフィールドアクセスは誤検知しない() {
        // フィールドアクセス(u.name)・メソッド呼び出し(u.describe())はまだANY扱いで
        // 診断しない(milestone 29はリテラル構築の検証のみ。メソッド/フィールド読みは次段階)——
        // 正当なコードで誤検知が出ないことを確認
        let src = format!(
            "{USER}fn (u: User) describe() string {{\n    return u.name\n}}\nfn main() {{\n    u := User{{name: \"a\", age: 1}}\n    print(u.name)\n    print(u.describe())\n}}\n"
        );
        assert_eq!(check(&src), vec![]);
    }

    // ---- milestone 30: structのフィールドアクセス + メソッド ----

    // User + describe()メソッド付き
    const USER_M: &str = "struct User {\n    name: string\n    age: int\n}\nfn (u: User) describe() string {\n    return u.name\n}\n";

    #[test]
    fn 未知のフィールド読みはunknown_fieldを報告する() {
        let diags = check(&format!("{USER_M}fn main() {{\n    u := User{{name: \"a\", age: 1}}\n    print(u.nmae)\n}}\n"));
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, DiagnosticCode::UnknownField);
        assert_eq!(diags[0].message, "User has no field 'nmae' (fields: name, age)");
    }

    #[test]
    fn メソッドを呼ばずに参照するとmethod_not_calledを報告する() {
        let diags = check(&format!("{USER_M}fn main() {{\n    u := User{{name: \"a\", age: 1}}\n    x := u.describe\n    print(x)\n}}\n"));
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, DiagnosticCode::MethodNotCalled);
        assert_eq!(diags[0].message, "'describe' is a method — call it like describe(...)");
    }

    #[test]
    fn 存在しないメソッド呼び出しはunknown_fieldを報告する() {
        let diags = check(&format!("{USER_M}fn main() {{\n    u := User{{name: \"a\", age: 1}}\n    u.bogus()\n}}\n"));
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, DiagnosticCode::UnknownField);
        assert_eq!(diags[0].message, "User has no field or method 'bogus' (fields: name, age)");
    }

    #[test]
    fn 正しいフィールド読みとメソッド呼び出しは診断を出さない() {
        let diags = check(&format!("{USER_M}fn main() {{\n    u := User{{name: \"a\", age: 1}}\n    print(u.name)\n    print(u.age)\n    print(u.describe())\n}}\n"));
        assert_eq!(diags, vec![]);
    }

    #[test]
    fn フィールドの型が読み取りに伝播する() {
        // u.name は string 型として伝播——`u.name + 1`(string + int)はinvalid-operationになる
        let diags = check(&format!("{USER_M}fn main() {{\n    u := User{{name: \"a\", age: 1}}\n    x := u.name + 1\n    print(x)\n}}\n"));
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, DiagnosticCode::InvalidOperation);
    }

    #[test]
    fn メソッド呼び出しのtargetは二重評価されない() {
        // 回帰(code reviewで発覚): Member calleeのメソッド呼び出しがフォールバック経路で
        // targetを二重推論し、`undef.foo()`のundefined-nameが2回出ていた。Member calleeを
        // intercept内で完結させて解消(TS版calls.tsの「targetは一度だけ評価」)
        let diags = check("fn main() {\n    x := undef.foo()\n    print(x)\n}\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, DiagnosticCode::UndefinedName);
    }

    #[test]
    fn pkg修飾アクセスは誤検知しない() {
        // import alias(mathやjson)はANYとして登録されるので、pkg.func(...) のtargetが
        // undefined-nameにならない(pkg修飾の中身の検査は次段階)
        let diags = check("import \"mathutil\"\nfn main() {\n    x := mathutil.add(1, 2)\n    print(x)\n}\n");
        assert_eq!(diags, vec![]);
    }

    // ---- milestone 31: メソッド呼び出しの引数照合・メソッド本体の検査・宣言時診断 ----

    // User + 引数を1つ取るメソッド(引数照合のテスト用)
    const USER_G: &str = "struct User {\n    name: string\n    age: int\n}\nfn (u: User) greet(prefix: string) string {\n    return prefix + u.name\n}\n";

    #[test]
    fn メソッド呼び出しの引数個数が違うとargument_countを報告する() {
        // レシーバはparams[0]なので照合対象から外れる(「1個」であって2個ではない)
        let diags = check(&format!("{USER_G}fn main() {{\n    u := User{{name: \"a\", age: 1}}\n    print(u.greet())\n}}\n"));
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, DiagnosticCode::ArgumentCount);
        assert_eq!(diags[0].message, "expected 1 argument(s), got 0");
    }

    #[test]
    fn メソッド呼び出しの引数型が違うとtype_mismatchを報告する() {
        let diags = check(&format!("{USER_G}fn main() {{\n    u := User{{name: \"a\", age: 1}}\n    print(u.greet(3))\n}}\n"));
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, DiagnosticCode::TypeMismatch);
        assert_eq!(diags[0].message, "argument 1: cannot use int as string");
    }

    #[test]
    fn 正しい引数のメソッド呼び出しは診断を出さない() {
        // struct型の引数・メソッドチェーン(戻り値がstruct)も含めて誤検知しないこと
        let src = "struct Point {\n    x: int\n}\nfn (p: Point) plus(other: Point) int {\n    return p.x + other.x\n}\nfn (p: Point) scaled(k: int) Point {\n    return Point{x: p.x * k}\n}\nfn main() {\n    p := Point{x: 1}\n    print(p.plus(p))\n    print(p.scaled(2).plus(p))\n}\n";
        assert_eq!(check(src), vec![]);
    }

    #[test]
    fn メソッド本体の未定義名を検出する() {
        // milestone 30まではメソッド本体を丸ごと検査していなかったため無診断だった
        let diags = check(&format!("{USER_G}fn (u: User) broken() string {{\n    return u.name + missingName\n}}\nfn main() {{\n    print(\"x\")\n}}\n"));
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, DiagnosticCode::UndefinedName);
        // 文言はTS版(expressions.ts)と同じ`undefined: 'x'`
        assert_eq!(diags[0].message, "undefined: 'missingName'");
    }

    #[test]
    fn メソッド本体でレシーバとパラメータが参照できる() {
        // レシーバをスコープへ宣言していないと`u`自身がundefined-nameになる
        assert_eq!(check(&format!("{USER_G}fn main() {{\n    print(\"x\")\n}}\n")), vec![]);
    }

    #[test]
    fn メソッド本体の中のメソッド呼び出しも照合される() {
        let src = "struct Counter {\n    n: int\n}\nfn (c: Counter) plus(other: Counter) int {\n    return c.n + other.n\n}\nfn (c: Counter) callsOther() int {\n    return c.plus(3)\n}\nfn main() {\n    print(\"x\")\n}\n";
        let diags = check(src);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, DiagnosticCode::TypeMismatch);
        assert_eq!(diags[0].message, "argument 1: cannot use int as Counter");
    }

    #[test]
    fn 非structのレシーバはinvalid_receiver_typeを報告する() {
        let diags = check("fn (x: int) scalarRecv() {\n}\nfn main() {\n    print(\"x\")\n}\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, DiagnosticCode::InvalidReceiverType);
        assert_eq!(diags[0].message, "method receiver must be a struct type, got int");
    }

    #[test]
    fn フィールドと同名のメソッドはmethod_field_conflictを報告する() {
        let diags = check("struct User {\n    name: string\n}\nfn (u: User) name() string {\n    return u.name\n}\nfn main() {\n    print(\"x\")\n}\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, DiagnosticCode::MethodFieldConflict);
        assert_eq!(diags[0].message, "User already has a field named 'name'");
    }

    #[test]
    fn 同名メソッドの重複はduplicate_methodを報告する() {
        let diags = check(&format!(
            "{USER_G}fn (u: User) greet(prefix: string) string {{\n    return prefix\n}}\nfn main() {{\n    print(\"x\")\n}}\n"
        ));
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, DiagnosticCode::DuplicateMethod);
        assert_eq!(diags[0].message, "User already has a method named 'greet'");
    }

    #[test]
    fn 組み込み関数名のメソッドはbuiltin_redeclaredを報告する() {
        // メソッド名のときだけTS版は専用の文言("cannot be used as a method name")を使う
        let diags = check("struct User {\n    name: string\n}\nfn (u: User) len() int {\n    return 1\n}\nfn main() {\n    print(\"x\")\n}\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, DiagnosticCode::BuiltinRedeclared);
        assert_eq!(diags[0].message, "'len' is a builtin function and cannot be used as a method name");
    }

    #[test]
    fn 未宣言のレシーバ型にはメソッドを登録せず誤検知も出さない() {
        // 回帰(code reviewで発覚・両CLIで再現確認): `resolve_type_node`が未知の型名を
        // 「空フィールドの殻struct」へフォールバックさせるため、殻を本物のstructと見なして
        // メソッド表へ登録してしまい、同名メソッドを2つ書くと存在しないstructについて
        // `duplicate-method`という**TS版には無い誤検知**が出ていた(TS版はunknown-type+
        // invalid-receiver-type)。lookup_structガードで無診断(検出漏れ側)に戻した
        let src = "fn (a: Bogus) f() int {\n    return 1\n}\nfn (b: Bogus) f() int {\n    return 2\n}\nfn main() {\n    print(\"x\")\n}\n";
        assert_eq!(check(src), vec![]);
    }

    #[test]
    fn 戻り値なし関数でのreturn値はvoid_used_as_valueを報告する() {
        // milestone 22以来、TS版が`void-used-as-value`を出すケースでtype-mismatchを
        // 出していた(位置・文言もTS版とずれていた)——milestone 31で揃えた
        let diags = check("fn nothing() {\n    return 1\n}\nfn main() {\n    nothing()\n}\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, DiagnosticCode::VoidUsedAsValue);
        assert_eq!(diags[0].message, "this function has no return value");
        // 位置は`return`ではなく値の式(TS版statements.ts)
        assert_eq!(diags[0].pos.col, 12);
    }

    #[test]
    fn 戻り値の型不一致の文言と位置を揃えた() {
        let diags = check("fn f() int {\n    return \"no\"\n}\nfn main() {\n    print(f())\n}\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, DiagnosticCode::TypeMismatch);
        assert_eq!(diags[0].message, "cannot return \"no\" as int");
        assert_eq!(diags[0].pos.col, 12);
    }

    // ---- milestone 32: 判別可能unionの構築検証 ----

    // User + 無名メンバー3個の判別可能union(タグは`kind`)
    const RESP: &str = "struct User {\n    name: string\n}\ntype Resp =\n    { kind: \"ok\", user: User }\n    | { kind: \"notFound\" }\n    | { kind: \"error\", message: string }\n";
    // 名前付きstruct同士のunion(無名メンバー無し)。AとBはフィールド集合が同一で曖昧
    const SHAPES: &str = "struct A {\n    x: int\n}\nstruct B {\n    x: int\n}\nstruct C {\n    y: string\n}\ntype AB = A | B\ntype AC = A | C\n";

    #[test]
    fn 判別可能unionのタグ未指定はtag_missingを報告する() {
        let diags = check(&format!("{RESP}fn main() {{\n    u := User{{name: \"a\"}}\n    r := Resp{{user: u}}\n    print(r)\n}}\n"));
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, DiagnosticCode::DiscriminatedUnionTagMissing);
        assert_eq!(diags[0].message, "'Resp{...}' needs its tag field 'kind' set to select a member (e.g. Resp{ kind: \"...\", ... })");
    }

    #[test]
    fn 判別可能unionの未知のタグ値はno_matchを報告する() {
        let diags = check(&format!("{RESP}fn main() {{\n    r := Resp{{kind: \"bogus\"}}\n    print(r)\n}}\n"));
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, DiagnosticCode::DiscriminatedUnionNoMatch);
        assert_eq!(diags[0].message, "no member of 'Resp' has kind: \"bogus\" (valid kind values: \"ok\" | \"notFound\" | \"error\")");
    }

    #[test]
    fn 判別可能unionの絞り込み後にフィールドを検証する() {
        // 絞り込んだメンバー({kind:"ok", user: User})に対して未知/欠落/型不一致を検査し、
        // 診断に出す名前は(無名メンバーではなく)書かれたunion名になる
        let missing = check(&format!("{RESP}fn main() {{\n    r := Resp{{kind: \"ok\"}}\n    print(r)\n}}\n"));
        assert_eq!(missing.len(), 1);
        assert_eq!(missing[0].code, DiagnosticCode::MissingFields);
        assert_eq!(missing[0].message, "missing field(s) in Resp: user");

        let unknown = check(&format!("{RESP}fn main() {{\n    u := User{{name: \"a\"}}\n    r := Resp{{kind: \"ok\", user: u, extra: 1}}\n    print(r)\n}}\n"));
        assert_eq!(unknown.len(), 1);
        assert_eq!(unknown[0].code, DiagnosticCode::UnknownField);
        assert_eq!(unknown[0].message, "Resp has no field 'extra' (fields: kind, user)");

        let mismatch = check(&format!("{RESP}fn main() {{\n    r := Resp{{kind: \"error\", message: 42}}\n    print(r)\n}}\n"));
        assert_eq!(mismatch.len(), 1);
        assert_eq!(mismatch[0].code, DiagnosticCode::TypeMismatch);
        assert_eq!(mismatch[0].message, "field 'message': cannot use int as string");
    }

    #[test]
    fn 名前付きstruct同士のunionはフィールド集合で解決する() {
        // ACはフィールド集合が重ならないので一意に決まる(無診断)
        assert_eq!(check(&format!("{SHAPES}fn main() {{\n    v := AC{{y: \"s\"}}\n    print(v)\n}}\n")), vec![]);

        // ABは同じ形の2メンバーなので曖昧
        let ambiguous = check(&format!("{SHAPES}fn main() {{\n    v := AB{{x: 1}}\n    print(v)\n}}\n"));
        assert_eq!(ambiguous.len(), 1);
        assert_eq!(ambiguous[0].code, DiagnosticCode::DiscriminatedUnionAmbiguous);
        assert_eq!(ambiguous[0].message, "ambiguous — multiple members of 'AB' match the field(s) {x}");

        // どのメンバーの形とも一致しない
        let no_match = check(&format!("{SHAPES}fn main() {{\n    v := AC{{z: 1}}\n    print(v)\n}}\n"));
        assert_eq!(no_match.len(), 1);
        assert_eq!(no_match[0].code, DiagnosticCode::DiscriminatedUnionNoMatch);
        assert_eq!(no_match[0].message, "no member of 'AC' matches the field(s) {z} (union members: { x } | { y })");
    }

    #[test]
    fn 正しいunion構築は診断を出さない() {
        let diags = check(&format!("{RESP}fn main() {{\n    u := User{{name: \"a\"}}\n    print(Resp{{kind: \"ok\", user: u}})\n    print(Resp{{kind: \"notFound\"}})\n    print(Resp{{kind: \"error\", message: \"boom\"}})\n}}\n"));
        assert_eq!(diags, vec![]);
    }

    #[test]
    fn unionメンバーのコレクション_関数型フィールドは誤検知しない() {
        // milestone 29と同じ配慮: 縮退する型(配列/map/chan/fn/union)のフィールドは
        // 値側の型がANYへ潰れるため型検査をスキップする(unionメンバー側でも同じ)
        let src = "struct Item {\n    id: int\n}\ntype Page =\n    { kind: \"list\", items: Item[], render: fn(int) string }\n    | { kind: \"empty\" }\nfn label(n: int) string {\n    return \"item\"\n}\nfn main() {\n    print(Page{kind: \"list\", items: [Item{id: 1}], render: label})\n    print(Page{kind: \"empty\"})\n}\n";
        assert_eq!(check(src), vec![]);
    }

    #[test]
    fn 名前付きstruct_unionの絞り込みは縮退フィールドで候補を落とさない() {
        // 回帰(code reviewで発覚・両CLIで再現確認): 2段目の絞り込みが`assignable`を
        // 生で使っていたため、縮退する型(fn/配列/map/union)のフィールドで**全候補が
        // 脱落**していた。(1) 同じ形の2メンバー: TS版はambiguousだがRust版はno-matchという
        // 誤ったコード・文言を出していた → 絞り切れないので黙って諦める(検出漏れ側)。
        // (2) 型だけが違う2メンバー(TS版なら型で一意に決まる正当なコード): Rust版は
        // no-matchの**誤検知**を出していた → 無診断になる(TS版と一致)
        let same_shape = "struct A {\n    cb: fn(int[]) int\n}\nstruct B {\n    cb: fn(int[]) int\n}\ntype AB = A | B\nfn sum(xs: int[]) int {\n    return 0\n}\nfn main() {\n    print(AB{cb: sum})\n}\n";
        assert_eq!(check(same_shape), vec![]);
        let distinct_types = "struct A {\n    cb: fn(int[]) int\n}\nstruct B {\n    cb: fn(string) int\n}\ntype AB = A | B\nfn sum(xs: int[]) int {\n    return 0\n}\nfn main() {\n    print(AB{cb: sum})\n}\n";
        assert_eq!(check(distinct_types), vec![]);
        // (3) 値の型がトップレベルANYへ潰れる場合(配列リテラル)も同じ穴の別の現れ方——
        // `assignable(ANY, _)`は常にtrueなので候補が**全て残り**、TS版が一意に解決する
        // 正当なコードにambiguousの誤検知を出していた(別のレビューエージェントが指摘)
        let array_fields = "struct A {\n    items: int[]\n}\nstruct B {\n    items: string[]\n}\ntype U = A | B\nfn main() {\n    print(U{items: [1, 2, 3]})\n}\n";
        assert_eq!(check(array_fields), vec![]);
    }

    #[test]
    fn union構築の式全体の型はunion自身になる() {
        // TS版と同じく、絞り込んだメンバーではなくunion自身を返す——union値への算術は
        // invalid-operation(milestone 25の演算子検査)になる
        let diags = check(&format!("{RESP}fn main() {{\n    x := Resp{{kind: \"notFound\"}} + 1\n    print(x)\n}}\n"));
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, DiagnosticCode::InvalidOperation);
    }

    // ---- milestone 33: 配列型のモデル化 ----

    #[test]
    fn 配列リテラルの要素型不一致はtype_mismatchを報告する() {
        // 要素型は先頭要素をwidenしたもの(`["a","b"]`は`"a"[]`ではなく`string[]`)
        let diags = check("fn main() {\n    xs := [1, \"a\"]\n    print(xs)\n}\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, DiagnosticCode::TypeMismatch);
        assert_eq!(diags[0].message, "array element must be int, got \"a\"");
    }

    #[test]
    fn 添字の型と添字できない対象を検査する() {
        let bad_index = check("fn main() {\n    xs := [1, 2]\n    print(xs[\"k\"])\n}\n");
        assert_eq!(bad_index.len(), 1);
        assert_eq!(bad_index[0].code, DiagnosticCode::InvalidIndexType);
        assert_eq!(bad_index[0].message, "index must be int, got \"k\"");

        let not_indexable = check("fn main() {\n    n := 5\n    print(n[0])\n}\n");
        assert_eq!(not_indexable.len(), 1);
        assert_eq!(not_indexable[0].code, DiagnosticCode::NotIndexable);
        assert_eq!(not_indexable[0].message, "cannot index into int");

        // 文字列の添字は文字列を返す(TS版と同じ)——誤検知しない
        assert_eq!(check("fn main() {\n    s := \"ab\"\n    print(s[0])\n}\n"), vec![]);
    }

    #[test]
    fn mapの添字は誤検知しない() {
        // milestone 34でmapをモデル化した後は、mapの添字は`invalid-index-type`ではなく
        // キー型(`map key must be K`)で検査される——文字列キーは当然通る。
        // (milestone 33時点では「ANYコンテナでは添字の型検査をしない」ことで通していた)
        assert_eq!(check("fn main() {\n    m := map<string, int>{\"a\": 1}\n    print(m[\"a\"] or 0)\n}\n"), vec![]);
    }

    #[test]
    fn 配列要素への代入の型を検査する() {
        let diags = check("fn main() {\n    mut xs := [1, 2]\n    xs[0] = \"no\"\n    print(xs)\n}\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, DiagnosticCode::TypeMismatch);
        assert_eq!(diags[0].message, "cannot assign \"no\" to int");
    }

    #[test]
    fn 配列を取る組み込みが要素型を検査する() {
        let push = check("fn main() {\n    xs := [1]\n    push(xs, \"no\")\n}\n");
        assert_eq!(push.len(), 1);
        assert_eq!(push[0].code, DiagnosticCode::TypeMismatch);
        assert_eq!(push[0].message, "cannot push \"no\" into int[]");

        let contains = check("fn main() {\n    xs := [1]\n    print(contains(xs, \"no\"))\n}\n");
        assert_eq!(contains.len(), 1);
        assert_eq!(contains[0].message, "contains() second argument must be int, got \"no\"");

        let sort_bools = check("fn main() {\n    print(sort([true, false]))\n}\n");
        assert_eq!(sort_bools.len(), 1);
        assert_eq!(sort_bools[0].code, DiagnosticCode::BuiltinArgType);
        assert_eq!(sort_bools[0].message, "sort() requires int[], float[] or string[], got bool[]");
    }

    #[test]
    fn 高階組み込みのコールバック署名を検査する() {
        let filter_ret = check("fn double(n: int) int {\n    return n * 2\n}\nfn main() {\n    print(filter([1], double))\n}\n");
        assert_eq!(filter_ret.len(), 1);
        assert_eq!(filter_ret[0].code, DiagnosticCode::CallbackSignatureMismatch);
        assert_eq!(filter_ret[0].message, "filter() callback must return bool, got int");

        let map_param = check("fn twoArgs(a: string, b: int) string {\n    return a\n}\nfn main() {\n    print(map([1], twoArgs))\n}\n");
        assert_eq!(map_param.len(), 1);
        assert_eq!(map_param[0].code, DiagnosticCode::CallbackSignatureMismatch);
        assert_eq!(map_param[0].message, "map() callback must take a single int parameter");
    }

    #[test]
    fn 高階組み込みの戻り値型が伝播する() {
        // map()の要素型はコールバックの戻り値型——`sort(map(xs, label))`はstring[]なのでOK、
        // `round(...)`のようなスカラー検査には配列として弾かれる
        assert_eq!(
            check("fn label(n: int) string {\n    return str(n)\n}\nfn main() {\n    print(join(sort(map([1, 2], label)), \",\"))\n}\n"),
            vec![]
        );
        let joined_ints = check("fn main() {\n    print(join([1, 2], \",\"))\n}\n");
        assert_eq!(joined_ints.len(), 1);
        assert_eq!(joined_ints[0].message, "join() requires string[], got int[]");
    }

    #[test]
    fn range_forは配列の添字と要素を宣言する() {
        // 要素型が伝播するので`v + 1`(string + int)がinvalid-operationになる
        let diags = check("fn main() {\n    for i, v := range [\"a\"] {\n        print(v + 1)\n        print(i)\n    }\n}\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, DiagnosticCode::InvalidOperation);

        let arity = check("fn main() {\n    for i := range [1, 2] {\n        print(i)\n    }\n}\n");
        assert_eq!(arity.len(), 1);
        assert_eq!(arity[0].code, DiagnosticCode::RangeArity);

        let not_rangeable = check("fn main() {\n    for i, v := range \"str\" {\n        print(i)\n        print(v)\n    }\n}\n");
        assert_eq!(not_rangeable.len(), 1);
        assert_eq!(not_rangeable[0].code, DiagnosticCode::NotRangeable);
        assert_eq!(not_rangeable[0].message, "cannot range over \"str\""); // 文字列リテラル型のまま(TS版と一致)
    }

    #[test]
    fn 配列型の引数照合が効く() {
        let diags = check("fn take(xs: int[]) int {\n    return len(xs)\n}\nfn main() {\n    print(take([\"a\"]))\n}\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, DiagnosticCode::TypeMismatch);
        assert_eq!(diags[0].message, "argument 1: cannot use string[] as int[]");
    }

    #[test]
    fn 空配列リテラルはcannot_infer_typeを報告する() {
        let diags = check("fn main() {\n    xs := []\n    print(xs)\n}\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, DiagnosticCode::CannotInferType);
        assert_eq!(diags[0].message, "cannot infer a complete type for 'xs' (got any[]) — add a type annotation, e.g. 'xs: T[] = []'");
        // 型注釈があれば当然OK
        assert_eq!(check("fn main() {\n    xs: int[] = []\n    push(xs, 1)\n    print(len(xs))\n}\n"), vec![]);
    }

    #[test]
    fn 要素が縮退する配列は突き合わせない() {
        // 内側にANY縮退を含む型(ここでは関数型の配列——`fn`は引き続き縮退する)は、
        // レジストリ側の完全解決された型と突き合わせると誤検知する——is_fully_modeledで
        // 再帰的に判定してスキップする。**milestone 34でmapもモデル化されたので、
        // `map<string,int>[]`はもう縮退しない**(この例は正しく検査される側に移った)
        let degenerate = "struct Bag {\n    cbs: fn(int[]) int[]\n}\nfn pick(xs: int[]) int {\n    return 0\n}\nfn main() {\n    b := Bag{cbs: [pick]}\n    print(len(b.cbs))\n}\n";
        assert_eq!(check(degenerate), vec![]);
        let modeled = "struct Bag {\n    rows: map<string, int>[]\n}\nfn main() {\n    b := Bag{rows: [map<string, int>{\"a\": 1}]}\n    print(len(b.rows))\n}\n";
        assert_eq!(check(modeled), vec![]);
    }

    // ---- milestone 37: パッケージ単位の検査 ----

    #[test]
    fn 同じパッケージの別ファイルの関数を参照できる() {
        // 回帰(milestone 35のゲート統合以降、複数ファイルのパッケージが内部で相互参照すると
        // undefined-nameの誤検知になっていた。`mesh test`のテストファイルからも同じ形で踏む)
        let a = parse("export fn twice(n: int) int {\n    return double(n)\n}\n").unwrap();
        let b = parse("fn double(n: int) int {\n    return n * 2\n}\n").unwrap();
        let out = check_package(&[("a.mesh".to_string(), &a), ("b.mesh".to_string(), &b)], false);
        assert_eq!(out, vec![]);
    }

    #[test]
    fn パッケージ検査でも診断は出たファイルへ振り分けられる() {
        let a = parse("fn main() {\n    print(undefinedA)\n}\n").unwrap();
        let b = parse("fn helper() {\n    print(undefinedB)\n}\n").unwrap();
        let out = check_package(&[("a.mesh".to_string(), &a), ("b.mesh".to_string(), &b)], true);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].0, "a.mesh");
        assert_eq!(out[0].1[0].message, "undefined: 'undefinedA'");
        assert_eq!(out[1].0, "b.mesh");
        assert_eq!(out[1].1[0].message, "undefined: 'undefinedB'");
    }

    #[test]
    fn require_mainが偽ならmainの形は検査しない() {
        // 回帰(code reviewで発覚・両CLIで再現確認): `require_main`は「無くてもよい」だけを
        // 制御しており、`main`という名前の**ただの関数**(依存パッケージが
        // `export fn main(n: int) int`を持ち`util.main(3)`と呼ぶのは正当)まで
        // invalid-main-signatureで弾いていた。TS版は`if (opts.requireMain)`でブロック全体を包む
        let p = parse("export fn main(n: int) int {\n    return n * 2\n}\n").unwrap();
        assert_eq!(check_package(&[("util.mesh".to_string(), &p)], false), vec![]);
        // mainパッケージ(require_main=true)なら従来どおり形を検査する
        let out = check_package(&[("main.mesh".to_string(), &p)], true);
        assert_eq!(out[0].1[0].code, DiagnosticCode::InvalidMainSignature);
    }

    #[test]
    fn 宣言時診断の順序はソース順になる() {
        // 回帰(code reviewで発覚): メソッド表と自由関数シグネチャを2周に分けて登録していたため、
        // 同じファイルに両方の宣言時診断があると報告順がソース順と食い違っていた
        // (TS版は1つのループで振り分ける)
        let p = parse("fn helper() int {\n    return 1\n}\nfn helper() int {\n    return 2\n}\nfn (n: int) bad() {\n}\nfn main() {\n}\n").unwrap();
        let out = check_package(&[("a.mesh".to_string(), &p)], true);
        let codes: Vec<DiagnosticCode> = out[0].1.iter().map(|d| d.code).collect();
        assert_eq!(codes, vec![DiagnosticCode::AlreadyDeclared, DiagnosticCode::InvalidReceiverType]);
    }

    #[test]
    fn invalid_main_signatureはmainを宣言したファイルに紐づく() {
        // 回帰(code reviewで発覚): パッケージ全体の性質として一律に先頭ファイルへ
        // 紐づけていたため、mainが2番目以降のファイルにあると無関係なファイルが指された
        // (TS版は`withMain.file`に紐づける)
        let a = parse("fn helper() int {\n    return 1\n}\n").unwrap();
        let b = parse("fn main(n: int) {\n}\n").unwrap();
        let out = check_package(&[("a.mesh".to_string(), &a), ("b.mesh".to_string(), &b)], true);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].0, "b.mesh");
        assert_eq!(out[0].1[0].code, DiagnosticCode::InvalidMainSignature);
    }

    #[test]
    fn mainはパッケージのどのファイルにあってもよい() {
        let a = parse("fn helper() int {\n    return 1\n}\n").unwrap();
        let b = parse("fn main() {\n    print(helper())\n}\n").unwrap();
        assert_eq!(check_package(&[("a.mesh".to_string(), &a), ("b.mesh".to_string(), &b)], true), vec![]);
        // 無ければ(require_mainのとき)missing-main
        let only_a = check_package(&[("a.mesh".to_string(), &a)], true);
        assert_eq!(only_a.len(), 1);
        assert_eq!(only_a[0].1[0].code, DiagnosticCode::MissingMain);
    }

    // ---- milestone 34: map/channelのモデル化 + isによる絞り込み ----

    #[test]
    fn mapリテラルのキーと値の型を検査する() {
        let diags = check("fn main() {\n    m := map<string, int>{\"a\": \"x\", 4: 5}\n    print(m)\n}\n");
        assert_eq!(diags.len(), 2);
        assert_eq!(diags[0].code, DiagnosticCode::TypeMismatch);
        assert_eq!(diags[0].message, "map value must be int, got \"x\"");
        assert_eq!(diags[1].message, "map key must be string, got int");
    }

    #[test]
    fn map読みはキー型を検査し成功側とnoneのunionを返す() {
        let bad_key = check("fn main() {\n    m := map<string, int>{}\n    print(m[7] or 0)\n}\n");
        assert_eq!(bad_key.len(), 1);
        assert_eq!(bad_key[0].message, "map key must be string, got int");

        // 読みは`V | none`——そのまま算術に使うとinvalid-operation(TS版と同じ)
        let unnarrowed = check("fn main() {\n    m := map<string, int>{}\n    x := m[\"a\"]\n    print(x + 1)\n}\n");
        assert_eq!(unnarrowed.len(), 1);
        assert_eq!(unnarrowed[0].code, DiagnosticCode::InvalidOperation);

        // `or`で成功側だけ取り出せば通る(orは両辺を推論しつつ成功側の型を返す)
        assert_eq!(check("fn main() {\n    m := map<string, int>{}\n    x := m[\"a\"] or 0\n    print(x + 1)\n}\n"), vec![]);
    }

    #[test]
    fn mapへの代入と複合代入を検査する() {
        // 代入先の型は読みの`V | none`ではなく`V`
        let assign = check("fn main() {\n    mut m := map<string, int>{}\n    m[\"a\"] = \"no\"\n}\n");
        assert_eq!(assign.len(), 1);
        assert_eq!(assign[0].code, DiagnosticCode::TypeMismatch);
        assert_eq!(assign[0].message, "cannot assign \"no\" to int");

        let compound = check("fn main() {\n    mut m := map<string, int>{}\n    m[\"a\"] += 1\n}\n");
        assert_eq!(compound.len(), 1);
        assert_eq!(compound[0].code, DiagnosticCode::CompoundAssignOnMap);

        // 正しい代入は無診断
        assert_eq!(check("fn main() {\n    mut m := map<string, int>{}\n    m[\"a\"] = 1\n    print(len(m))\n}\n"), vec![]);
    }

    #[test]
    fn map組み込みのキー型と戻り値型が効く() {
        let del = check("fn main() {\n    mut m := map<string, int>{}\n    delete(m, 7)\n}\n");
        assert_eq!(del.len(), 1);
        assert_eq!(del[0].message, "map key must be string, got int");

        // keys/valuesは`K[]`/`V[]`を返す——sort(keys(m))は string[] なのでOK、
        // values(m)はint[]なのでjoinは弾かれる
        assert_eq!(check("fn main() {\n    m := map<string, int>{}\n    print(join(sort(keys(m)), \",\"))\n}\n"), vec![]);
        let joined = check("fn main() {\n    m := map<string, int>{}\n    print(join(values(m), \",\"))\n}\n");
        assert_eq!(joined.len(), 1);
        assert_eq!(joined[0].message, "join() requires string[], got int[]");
    }

    #[test]
    fn range_forはmapのキーと値を宣言する() {
        // 値の型が伝播する(`v + 1`はstring + intでinvalid-operation)
        let diags = check("fn main() {\n    m := map<string, string>{}\n    for k, v := range m {\n        print(k)\n        print(v + 1)\n    }\n}\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, DiagnosticCode::InvalidOperation);

        let arity = check("fn main() {\n    m := map<string, int>{}\n    for k := range m {\n        print(k)\n    }\n}\n");
        assert_eq!(arity.len(), 1);
        assert_eq!(arity[0].code, DiagnosticCode::RangeArity);
        assert_eq!(arity[0].message, "range over a map needs two names: 'for a, b := range ...' (use _ to ignore one)");
    }

    #[test]
    fn channelの送受信と容量を検査する() {
        let send = check("fn main() {\n    ch := chan<int>(1)\n    ch <- \"no\"\n}\n");
        assert_eq!(send.len(), 1);
        assert_eq!(send[0].code, DiagnosticCode::TypeMismatch);
        assert_eq!(send[0].message, "cannot send \"no\" to chan<int>");

        let capacity = check("fn main() {\n    ch := chan<int>(\"two\")\n    close(ch)\n}\n");
        assert_eq!(capacity.len(), 1);
        assert_eq!(capacity[0].message, "channel capacity must be int or none, got \"two\"");

        let not_chan = check("fn main() {\n    n := 5\n    n <- 1\n    v := <-n\n    print(v)\n}\n");
        assert_eq!(not_chan.len(), 2);
        assert_eq!(not_chan[0].code, DiagnosticCode::NotAChannel);
        assert_eq!(not_chan[0].message, "cannot send to non-channel type int");
        assert_eq!(not_chan[1].message, "cannot receive from non-channel type int");

        // 受信は`T | closed`——絞り込まずに使うとinvalid-operation(TS版と同じ)
        let unnarrowed = check("fn main() {\n    ch := chan<int>(1)\n    v := <-ch\n    print(v + 1)\n}\n");
        assert_eq!(unnarrowed.len(), 1);
        assert_eq!(unnarrowed[0].code, DiagnosticCode::InvalidOperation);
    }

    #[test]
    fn isによる絞り込みが効く() {
        // then節が必ず終端する場合、後続は「残り」の型で読める(フォールスルー)
        assert_eq!(
            check("fn main() {\n    ch := chan<int>(1)\n    mut total := 0\n    for {\n        v := <-ch\n        if v is closed {\n            break\n        }\n        total = total + v\n    }\n    print(total)\n}\n"),
            vec![]
        );
        // else節でも絞り込む
        assert_eq!(
            check("fn main() {\n    m := map<string, int>{}\n    v := m[\"a\"]\n    if v is none {\n        print(\"none\")\n    } else {\n        print(v + 1)\n    }\n}\n"),
            vec![]
        );
        // then節では絞り込んだ型になる(noneに算術→invalid-operation)
        let then_narrowed = check("fn main() {\n    m := map<string, int>{}\n    v := m[\"a\"]\n    if v is none {\n        print(v + 1)\n    }\n}\n");
        assert_eq!(then_narrowed.len(), 1);
        assert_eq!(then_narrowed[0].code, DiagnosticCode::InvalidOperation);
        assert_eq!(then_narrowed[0].message, "invalid operation: none + int");
    }

    #[test]
    fn mutな束縛は絞り込まない() {
        // 回帰(code reviewで発覚・両CLIで再現確認): TS版`narrowing.ts`の`stablePath`は
        // **不変な束縛だけ**を絞り込み対象にする。mutを絞り込むと、絞り込んだ型が
        // 「宣言された型」として居座り、後続の正当な再代入を誤って弾く一方(誤検知)、
        // 本来出るべき`invalid-operation`を見落とす(検出漏れ)
        let src = "fn drain(ch: chan<int>) int {\n    mut v := <-ch\n    if v is closed {\n        return 0\n    }\n    v = <-ch\n    return v + 1\n}\nfn main() {\n    ch := chan<int>(1)\n    print(drain(ch))\n}\n";
        let diags = check(src);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, DiagnosticCode::InvalidOperation);
        assert_eq!(diags[0].message, "invalid operation: int | closed + int");
    }

    #[test]
    fn 複合代入の診断は実際の演算子を使う() {
        // 回帰(code reviewで発覚): `+=`固定の文言だったため`-=`等でTS版と食い違っていた
        let diags = check("fn main() {\n    mut m := map<string, int>{\"a\": 1}\n    m[\"a\"] -= 1\n    print(len(m))\n}\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, DiagnosticCode::CompoundAssignOnMap);
        assert_eq!(diags[0].message, "cannot use '-=' on a map entry — the key may not exist yet; write 'm[k] = (m[k] or fallback) - value' instead");
    }

    #[test]
    fn map_channelを含む実用的なコードは誤検知しない() {
        // struct フィールドのmap/chan・入れ子map・map<K, V[]>・or束縛・keys/sort・
        // メソッド越しのrange——milestone 34で誤検知が出やすい組み合わせ
        let src = "struct Store {\n    counts: map<string, int>\n    rows: map<string, int[]>\n    inbox: chan<string>\n}\nfn (s: Store) total() int {\n    mut sum := 0\n    for _, v := range s.counts {\n        sum = sum + v\n    }\n    return sum\n}\nfn main() {\n    mut m := map<string, int>{}\n    m[\"a\"] = (m[\"a\"] or 0) + 1\n    s := Store{counts: m, rows: map<string, int[]>{}, inbox: chan<string>(1)}\n    print(s.total())\n    print(join(sort(keys(s.counts)), \",\"))\n    s.inbox <- \"hi\"\n    close(s.inbox)\n}\n";
        assert_eq!(check(src), vec![]);
    }

    #[test]
    fn int_rangeは1つ目の名前しか宣言しない() {
        // 回帰(code reviewで発覚・両CLIで再現確認): 余分な名前まで宣言していたため、
        // TS版が出す`undefined-name`を見落としていた(TS版statements.tsのintブランチは
        // `stmt.names[0]`しか宣言しない)
        let diags = check("fn main() {\n    for i, x := range 5 {\n        print(x)\n        print(i)\n    }\n}\n");
        assert_eq!(diags.len(), 2);
        assert_eq!(diags[0].code, DiagnosticCode::RangeArity);
        assert_eq!(diags[1].code, DiagnosticCode::UndefinedName);
        assert_eq!(diags[1].message, "undefined: 'x'");
    }

    #[test]
    fn cannot_infer_typeの抑止は文単位で判定する() {
        // 回帰(code reviewで発覚・両CLIで再現確認): `already_errored`を名前ごとに
        // 取り直していたため、先の値のundefined-nameがあっても後の名前に
        // cannot-infer-typeを重ねて出していた(TS版は文単位の`alreadyErrored`で抑止)
        let diags = check("fn main() {\n    a, b := undefinedThing, []\n    print(a)\n    print(b)\n}\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, DiagnosticCode::UndefinedName);
    }

    #[test]
    fn 配列要素代入の診断位置は代入文自身() {
        // 回帰(code reviewで発覚): targetの位置で報告していたため、複数代入で
        // TS版(stmt.pos)と列がずれていた
        let diags = check("fn main() {\n    mut a := 1\n    mut xs := [1, 2]\n    a, xs[0] = 2, \"no\"\n    print(a)\n    print(xs)\n}\n");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, DiagnosticCode::TypeMismatch);
        assert_eq!(diags[0].pos.col, 5); // `a, xs[0] = ...`の先頭(TS版と一致)
    }

    #[test]
    fn structのフィールドとメソッド越しの配列も型が伝播する() {
        let src = "struct Row {\n    cells: string[]\n}\nfn (r: Row) first() string {\n    return r.cells[0]\n}\nfn main() {\n    r := Row{cells: [\"x\"]}\n    print(r.first())\n    push(r.cells, 1)\n}\n";
        let diags = check(src);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, DiagnosticCode::TypeMismatch);
        assert_eq!(diags[0].message, "cannot push int into string[]");
    }

    #[test]
    fn structメンバーを持たないunionは素通りする() {
        // `type Status = "active" | "banned"`のようなリテラルunionはTS版も無診断で
        // ANYへフォールバックする(cannot-infer-typeは未移植の別診断)
        assert_eq!(check("type Status = \"active\" | \"banned\"\nfn main() {\n    s := Status{foo: 1}\n    print(s)\n}\n"), vec![]);
    }
}
