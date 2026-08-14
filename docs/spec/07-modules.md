# 仕様 7章 — モジュール(Modules)

パッケージ・import・可視性・名前解決・トップレベル宣言を定める。根拠ADR: [0017](../adr/0017-modules-package-directory.md)(パッケージ=ディレクトリ・常に修飾)/ [0035](../adr/0035-statement-details.md)(未使用エラーの思想)/ [0038](../adr/0038-module-details.md)(トップレベルlet限定・定数式初期化・as別名・リーク禁止・ルートimport不可)。

エラーコードは `E07xx`。負例のテストIDは `tests/07-modules/` 配下。規則番号(M-n)は安定ID。本章は5章の申し送り(未定義名の解決規則・mainの配置)を消化する。

## 7.1 パッケージとimport

```ebnf
sourceFile   = { importDecl } { topLevelDecl }
importDecl   = "import" pathLit [ "as" identifier ]
pathLit      = stringLit      (* 字句は1章。補間・エスケープを含まないこと(M-2、E0713) *)
topLevelDecl = fnDecl | structDecl | errorStructDecl | typeDecl | topLet
topLet       = [ "export" ] "let" identifier [ ":" type ] "=" constExpr
constExpr    = expr           (* 文法は3章のexprを共用し、M-10が意味規則として構成要素を制限する *)
```

- **M-1**: パッケージは**ディレクトリ**である。ディレクトリ内のすべての `.mesh` ファイルは同一パッケージに属し、**宣言の名前空間**を共有する(宣言についてファイル分割は意味を持たない。importはファイル単位=各ファイルに書く)。プロジェクトルートのディレクトリが**ルートパッケージ**である。パッケージ名(ディレクトリ名)の小文字ケースはlintが強制する(ADR-0024)。〔正例テスト: `multi-file-package`〕
- **M-2**(importパス): パスは `/` 区切りのセグメント列で、各セグメントは**識別子として合法かつ非予約語**であること(補間・エスケープ・空セグメント・`.`/`..` は不可=エラー E0713。この制約により1章の識別子規則がディレクトリ名にも及ぶ)。パスは実ディレクトリ名と**大文字小文字まで厳密一致**すること。存在しないパス・`.mesh` を1つも含まないディレクトリはエラー E0702(近いパスを提示)。`std/` はツールチェーンの標準ライブラリ、`js` はFFI(10章)に予約し、**ルート直下の `std`・`js` ディレクトリはビルド開始時に検査してE0713**(プロジェクト全体の走査はせず、ルート直下のみ確認する)。importはファイル先頭のみ(位置違反はE0701)。〔負例: `import-position`、`import-not-found`、`import-bad-path`(補間・`..`・std直下ディレクトリの3形)〕
- **M-3**(参照名と別名): パッケージの既定の参照名はパスの**最終セグメント**。`as 識別子` で別名を付けられ(`_` は不可)、その場合は別名のみで参照する。同一ファイル内で参照名が衝突するimport、参照名が**同パッケージのトップレベル宣言と衝突する**import(ファイル間の順序が無いため、衝突は常にimport側のE0703として報告する。ローカル宣言側が後から衝突する形はS-4のE0402)、`as std`・`as js`、同一パッケージの重複import(別名でも)はエラー E0703 とし、必要に応じて `as` を案内すること。〔正例: `import-alias` / 負例: `import-name-collision`(参照名衝突・トップレベル宣言との衝突・重複importの3形)〕
- **M-4**: 未使用のimportはエラー E0704(ADR-0038)。〔負例: `unused-import`〕
- **M-14**(ルートパッケージ): サブパッケージからルートパッケージは**importできない**(依存は上→下の一方向。ADR-0038)。ルートを指すパスは存在しないため、ルートのディレクトリ名は識別子制約(M-2)の対象外。〔負例テストは不要(表現手段が無い)〕

## 7.2 可視性

- **M-5**: 他のパッケージから参照できるのは `export` を付けたトップレベル宣言のみであること。非exportシンボルへのパッケージ外からの参照はエラー E0705「`x` は `pkg` の内部です」。exportしたstructはフィールドも公開される(フィールド個別の可視性制御はv1に無い)。メソッドの可視性はそのfn宣言の `export` に従う。〔負例: `non-export-access`〕
- **M-6**(APIリークの禁止): export宣言のシグネチャ(関数の引数・戻り値、structのフィールド型、typeの右辺、**export letの型**=注釈または推論結果)に非exportの型を使うのはエラー E0706「公開APIに内部型 `T` が現れています」(ADR-0038)。〔負例: `private-type-leak`(export letの推論型経由の形を含む)〕
- **M-7**(修飾参照): 他パッケージのシンボルは常に `参照名.シンボル` で修飾して参照する(ADR-0017)。修飾は値位置(3章postfix)・型位置(2章primary)・structリテラル・matchパターン/`is` のmemberTypeで書ける(各章EBNFの `[ identifier "." ]`)。自パッケージ内は非修飾。〔正例: `qualified-reference`(値・型・リテラル・パターンの4形)〕

## 7.3 名前解決(5章申し送りの消化)

- **M-15**(パッケージ参照名の名前空間): import参照名は、そのファイル内で**値と型の両方の名前空間で名前を予約する**。参照名と同名のトップレベル宣言・ローカル宣言はシャドーイング禁止(S-4のE0402)の対象であり、逆に既存のトップレベル宣言と同名になるimportはE0703。この衝突禁止により、`foo.bar` の `foo` は「import参照名 → ローカル/トップレベルの値(X-30のフィールドアクセス)」の順で**一意に**解決できる。裸のパッケージ名を値として使う(`let p = json`)のはエラー E0707 のメッセージ変種「パッケージ名は値ではありません」。〔負例: `package-name-shadowing`、`package-as-value`〕
- **M-8**(未定義の名前): 値の位置の識別子がどのスコープでも解決できないとき、エラー E0707 を報告し、近い名前を候補として提示すること。修飾 `foo.bar` の `foo` が未解決の場合、プロジェクトに `foo` ディレクトリが存在するならE0702のメッセージ変種(importを案内)を**優先**し、存在しなければE0707。型名の未定義はT-1(E0201)。〔負例: `undefined-name`、`unimported-qualifier`〕
- **M-9**(可視範囲): ローカルの束縛は**宣言の後**からブロックの終わりまで可視。宣言前の使用、およびlet/mutの初期化式の中での自分自身の参照はE0707の変種(「この行ではまだ定義されていません」=F-1のローカル再帰不可の根拠)。トップレベルの `fn`・`struct`・`type`・`error struct`・**let** は、**関数本体の中からは**宣言順に関係なくパッケージ全体で可視(letはmainの前に初期化済み=M-10)。〔負例: `use-before-declaration`〕
- **M-11**: 循環import(直接・間接)はエラー E0709 とし、サイクルの経路を表示すること(ADR-0017)。〔負例: `circular-import`〕

## 7.4 トップレベル宣言

- **M-12**: トップレベルの変数宣言は `let`(不変)のみであること。`mut` はエラー E0710「トップレベルに可変状態は置けません。共有状態はmain内の `mut` とクロージャ捕捉で(5章F-13)」(ADR-0038)。名前は識別子のみ(`_` は不可)。トップレベルletはS-6(未使用エラー)の対象外。パッケージ修飾ターゲットへの代入(`config.limit = 5`)はE0404のメッセージ変種(letは不変)。〔負例: `toplevel-mut`、`assign-to-imported-let`〕
- **M-10**(定数式初期化。ADR-0038): トップレベルletの初期化式は**定数式**に限ること。定数式として許される構成要素は次の**閉リスト**である(3章expr文法の部分集合として意味規則で制限する): リテラル(3章literal)・単項/二項演算子・括弧・文字列補間(内側もconstExpr)・struct/listリテラル(要素がconstExpr)・**トップレベルletへの参照**(自他パッケージ。修飾可。そのフィールド/添字アクセスも可)・無名fn式(**本体は通常の関数本体で制約なし**。トップレベルに捕捉できるmutは存在しないため安全)。**それ以外**(関数・メソッド呼び出し、if式・match式、`or`・`is`・`?`・`error(...)`)はエラー E0712「トップレベルの初期値は定数式に限ります。動的な値はmainで組み立ててください」。〔負例: `toplevel-call-init`、`toplevel-nonconst-init`(if式・orの2形)/ 正例: `toplevel-const-init`(括弧・単項・修飾参照・フィールドアクセス・fn式を含む)〕
- **M-16**(コンパイル時評価): 定数式は**コンパイル時に依存順で評価**され、生成JSには評価済みの値が埋め込まれる(実行時の初期化順は存在せず観測不能。無名fn式は評価対象外=値としてそのまま定義される)。評価がX-5(ゼロ除算・安全整数域超過)または**T-16(list/stringの添字範囲外)**のpanic条件に該当したときは**コンパイルエラー E0714**(位置つき。実行時panicにはならない=6章H-7の一覧は不変)。let間の循環判定は**即時評価部分の参照のみ**を数える(無名fn式の本体内の参照は数えない=実行は初期化完了後のため、相互参照するfn値も合法)。自己参照を含む循環はエラー E0708。〔負例: `toplevel-forward-reference`(循環形)、`toplevel-const-eval-error`(`let x = 1 / 0` と添字範囲外の2形)/ 正例: `mutual-fn-const`(fn式本体の相互参照)〕
- map型のトップレベル定数はv1では構成できない(mapリテラルが無く呼び出しも不可のため)。mapの構成手段は11章へ申し送る。
- クロージャ工場のトップレベル束縛(`let counter = makeCounter()`)はE0712により書けない。これはグローバル可変状態の抜け道を封じるための意図的な制限である(ADR-0038)。

## 7.5 エントリポイントの配置

- **M-13**: 実行ビルドでは `fn main()`(F-16)が**ルートパッケージ**に1つだけ存在すること。無い・複数はエラー E0711(v1のビルド対象は実行アプリのみ。ライブラリビルドは12章の将来課題)。ルートパッケージ以外の `main` という名前の関数は通常の関数であり、エントリとして扱われない。〔負例: `missing-main`、`duplicate-main`〕

## conformance対応表

| テストID | 種別 | 規則 |
|---|---|---|
| multi-file-package | 正例 | M-1 |
| import-position / import-not-found / import-bad-path | 負例 | M-2 |
| import-alias | 正例 | M-3 |
| import-name-collision | 負例 | M-3 |
| unused-import | 負例 | M-4 |
| non-export-access | 負例 | M-5 |
| private-type-leak | 負例 | M-6 |
| qualified-reference | 正例 | M-7 |
| package-name-shadowing / package-as-value | 負例 | M-15 |
| undefined-name / unimported-qualifier | 負例 | M-8 |
| use-before-declaration | 負例 | M-9 |
| circular-import | 負例 | M-11 |
| toplevel-mut / assign-to-imported-let | 負例 | M-12 |
| toplevel-call-init / toplevel-nonconst-init | 負例 | M-10 |
| toplevel-const-init | 正例 | M-10 |
| toplevel-forward-reference / toplevel-const-eval-error | 負例 | M-16 |
| mutual-fn-const | 正例 | M-16 |
| missing-main / duplicate-main | 負例 | M-13 |
