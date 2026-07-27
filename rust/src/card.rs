// 言語カード(`mesh card`)。AIエージェントのコンテキストへ貼る前提の圧縮仕様書で、
// **本文はTS版`src/card.ts`をそのまま使う**——`include_str!`で埋め込み、テンプレート
// リテラルの中身だけを取り出す(codegen.rsの`prelude()`がruntime.tsに対してやっているのと
// 同じ手法)。本文をRust側へ複製しないのは、カードの主張がTS側のテストで実装と
// 突き合わせられているため(`tests/card-completeness.test.ts`)——複製するとそこから
// 外れて古くなる。
//
// `mesh card --for <file.mesh>...`(F-13後半)はTS版`src/card-subset.ts`の移植。渡された
// ソースが実際に使っている機能のセクションだけに絞る(トークン節約)。**カード本文は
// 書き換えず**`## 見出し`の境界で分割して振り分けるだけなので、本文を編集してもここは古く
// ならない。未知の見出しは安全側で常に含める。

use std::collections::HashSet;

const CARD_TS: &str = include_str!("../embedded/card.ts");

// テンプレートリテラルの中身を取り出し、JSのエスケープ(`\\`・`` \` ``・`\$`)を解決する。
// カード本文にはこの3種しか現れない(他のエスケープが増えたらここも足す必要がある——
// prelude()が`\\`の解決漏れで`toInt`を壊した前例があるので、素朴な部分文字列抽出にしない)
pub fn language_card() -> String {
    // **宣言位置を起点にする**——`prelude()`(runtime.ts)のように「ファイル最初のバッククォート」
    // で切ると、card.ts冒頭の日本語コメントに含まれる`` `mesh card` ``を拾ってしまう
    // (実装中に踏んだ)。`LANGUAGE_CARD = ` の後ろの最初のバッククォートから、
    // ファイル末尾の最後のバッククォートまでが本文
    const MARKER: &str = "LANGUAGE_CARD = ";
    let decl = CARD_TS.find(MARKER).expect("card.ts should declare LANGUAGE_CARD") + MARKER.len();
    let start = decl + CARD_TS[decl..].find('`').expect("card.ts should wrap LANGUAGE_CARD in a template literal") + 1;
    let end = CARD_TS.rfind('`').expect("card.ts should wrap LANGUAGE_CARD in a template literal");
    unescape_template(&CARD_TS[start..end])
}

fn unescape_template(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('\\') => out.push('\\'),
            Some('`') => out.push('`'),
            Some('$') => out.push('$'),
            // 想定外のエスケープはそのまま残す(消すより残す方が安全)
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Feature {
    Generics,
    DiscriminatedUnions,
    StructuredErrors,
    Structs,
    Arrays,
    Concurrency,
    Modules,
    Defer,
    HttpServer,
}

// 見出し文字列は`src/card.ts`の`## ...`行と完全一致させること(ズレると常時「含める」側に
// 落ちる——FEATURE_HEADINGSに無い見出しは含める仕様なので実害は無いが、意図した絞り込みが
// 効かなくなる)。TS版`FEATURE_HEADINGS`と同じ並び・同じ文字列
const FEATURE_HEADINGS: &[(Feature, &str)] = &[
    (Feature::Generics, "Generic functions"),
    (Feature::DiscriminatedUnions, "Discriminated unions (tagged struct shapes)"),
    (Feature::StructuredErrors, "Structured errors (discriminated unions that '?'/'or' can propagate)"),
    (Feature::Structs, "Structs, maps & methods"),
    (Feature::Arrays, "Arrays"),
    (Feature::Concurrency, "Concurrency (structured — every task has an owner, leaks are impossible)"),
    (Feature::Modules, "Modules (import / export)"),
    (Feature::Defer, "defer (run a call when the enclosing function returns)"),
    (Feature::HttpServer, "Standard library: mesh/http (C-6: server-only, v1)"),
];

// ソース文字列に対する簡易パターン検出(字句解析まではしない——誤検出より見逃しの方が
// 安全〈=そのセクションが余分に残るだけ〉なので、多少broadな判定で十分。TS版の
// `FEATURE_PATTERNS`の正規表現と同じ条件を、依存ゼロのまま手書きの走査で実装する)
fn detect_features(source: &str) -> HashSet<Feature> {
    let mut found = HashSet::new();
    // /\bfn\s+\w+\s*</
    if kw_ident_then(source, "fn", Some('<')) {
        found.insert(Feature::Generics);
    }
    // /\btype\s+\w+\s*=\s*\{/
    if type_decl_with_brace(source) {
        found.insert(Feature::DiscriminatedUnions);
    }
    // /\berror\s+(type|struct)\b/
    if error_type_or_struct(source) {
        found.insert(Feature::StructuredErrors);
    }
    // /\bstruct\s+\w+/
    if kw_ident_then(source, "struct", None) {
        found.insert(Feature::Structs);
    }
    // /\[\s*\]|\w+\[\]/
    if has_array_syntax(source) {
        found.insert(Feature::Arrays);
    }
    // /\b(spawn|detach|chan|select|wait)\b/
    if ["spawn", "detach", "chan", "select", "wait"].iter().any(|w| contains_word(source, w)) {
        found.insert(Feature::Concurrency);
    }
    // /\b(import|export)\b/
    if ["import", "export"].iter().any(|w| contains_word(source, w)) {
        found.insert(Feature::Modules);
    }
    if contains_word(source, "defer") {
        found.insert(Feature::Defer);
    }
    if source.contains("\"mesh/http\"") {
        found.insert(Feature::HttpServer);
    }
    found
}

// `\w` = [A-Za-z0-9_](JSの正規表現と同じ定義。Unicodeの語構成文字は含めない)
fn is_word(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

// `\bword\b`
fn contains_word(source: &str, word: &str) -> bool {
    let bytes: Vec<char> = source.chars().collect();
    let target: Vec<char> = word.chars().collect();
    for i in 0..bytes.len() {
        if bytes[i..].starts_with(&target[..])
            && (i == 0 || !is_word(bytes[i - 1]))
            && bytes.get(i + target.len()).map(|c| !is_word(*c)).unwrap_or(true)
        {
            return true;
        }
    }
    false
}

// `\bKW\s+\w+` (+ 続けて`\s*`のあと指定文字があること)
fn kw_ident_then(source: &str, kw: &str, then: Option<char>) -> bool {
    let chars: Vec<char> = source.chars().collect();
    let target: Vec<char> = kw.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if !chars[i..].starts_with(&target[..]) || (i > 0 && is_word(chars[i - 1])) {
            i += 1;
            continue;
        }
        let mut j = i + target.len();
        // `\s+`
        let ws_start = j;
        while j < chars.len() && chars[j].is_whitespace() {
            j += 1;
        }
        if j == ws_start {
            i += 1;
            continue;
        }
        // `\w+`
        let id_start = j;
        while j < chars.len() && is_word(chars[j]) {
            j += 1;
        }
        if j == id_start {
            i += 1;
            continue;
        }
        match then {
            None => return true,
            Some(expected) => {
                while j < chars.len() && chars[j].is_whitespace() {
                    j += 1;
                }
                if chars.get(j) == Some(&expected) {
                    return true;
                }
            }
        }
        i += 1;
    }
    false
}

// `\btype\s+\w+\s*=\s*\{`
fn type_decl_with_brace(source: &str) -> bool {
    let chars: Vec<char> = source.chars().collect();
    let target: Vec<char> = "type".chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if !chars[i..].starts_with(&target[..]) || (i > 0 && is_word(chars[i - 1])) {
            i += 1;
            continue;
        }
        let mut j = i + target.len();
        let ws_start = j;
        while j < chars.len() && chars[j].is_whitespace() {
            j += 1;
        }
        if j == ws_start {
            i += 1;
            continue;
        }
        let id_start = j;
        while j < chars.len() && is_word(chars[j]) {
            j += 1;
        }
        if j == id_start {
            i += 1;
            continue;
        }
        while j < chars.len() && chars[j].is_whitespace() {
            j += 1;
        }
        if chars.get(j) != Some(&'=') {
            i += 1;
            continue;
        }
        j += 1;
        while j < chars.len() && chars[j].is_whitespace() {
            j += 1;
        }
        if chars.get(j) == Some(&'{') {
            return true;
        }
        i += 1;
    }
    false
}

// `\berror\s+(type|struct)\b`
fn error_type_or_struct(source: &str) -> bool {
    let chars: Vec<char> = source.chars().collect();
    let target: Vec<char> = "error".chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if !chars[i..].starts_with(&target[..]) || (i > 0 && is_word(chars[i - 1])) {
            i += 1;
            continue;
        }
        let mut j = i + target.len();
        let ws_start = j;
        while j < chars.len() && chars[j].is_whitespace() {
            j += 1;
        }
        if j > ws_start {
            for kw in ["type", "struct"] {
                let k: Vec<char> = kw.chars().collect();
                if chars[j..].starts_with(&k[..]) && chars.get(j + k.len()).map(|c| !is_word(*c)).unwrap_or(true) {
                    return true;
                }
            }
        }
        i += 1;
    }
    false
}

// `\[\s*\]` または `\w+\[\]`
//
// **第2分岐(`\w+\[\]`)は実質デッドコード**——code review(2026-07-25、PR #63)の指摘で、
// 記録として残すことにした。第1分岐の`\s*`は0文字にマッチするので、`xs[]`の`[]`は
// そちらで既に拾える。**TS版の正規表現`/\[\s*\]|\w+\[\]/`が同じ理由で第2の選択肢を
// 持っているので、忠実移植としてそのまま残す**(消しても挙動は変わらないが、
// TS版と1対1で読み比べられなくなる方が移植中のコストは高い)。
// TS版の該当行を消すときは、ここも一緒に消すこと。
fn has_array_syntax(source: &str) -> bool {
    let chars: Vec<char> = source.chars().collect();
    for i in 0..chars.len() {
        if chars[i] == '[' {
            let mut j = i + 1;
            while j < chars.len() && chars[j].is_whitespace() {
                j += 1;
            }
            if chars.get(j) == Some(&']') {
                return true;
            }
            // `\w+\[\]`: `[`の直前が語構成文字で、直後が`]`
            if i > 0 && is_word(chars[i - 1]) && chars.get(i + 1) == Some(&']') {
                return true;
            }
        }
    }
    false
}

struct Section {
    heading: Option<String>, // None = 最初の`##`より前(タイトル+導入文)
    body: String,
}

// `^## `の直前で分割する(TS版の`card.split(/(?=^## )/m)`と同じ)
fn split_sections(card: &str) -> Vec<Section> {
    let mut sections: Vec<Section> = Vec::new();
    let mut current = String::new();
    let mut heading: Option<String> = None;
    for line in card.split_inclusive('\n') {
        if let Some(rest) = line.strip_prefix("## ") {
            if !current.is_empty() {
                sections.push(Section { heading: heading.take(), body: std::mem::take(&mut current) });
            }
            heading = Some(rest.trim_end().trim().to_string());
        }
        current.push_str(line);
    }
    if !current.is_empty() {
        sections.push(Section { heading, body: current });
    }
    sections
}

const SUBSET_DISCLAIMER: &str = "This card is a PROJECT-SCOPED SUBSET — it includes only the sections relevant to features \
detected in the source given to 'mesh card --for'. It is NOT a complete list of Mesh's \
features; if you need something not covered here, run 'mesh card' (no --for) for the full \
reference before concluding a feature doesn't exist.";

const FULL_CARD_CLAIM: &str = "This card is the COMPLETE reference —\nMesh has no features beyond what is listed here.";

// 渡されたMeshソース(複数可)を見て、使われている機能のセクションだけに絞ったカードを返す
pub fn subset_card(sources: &[String]) -> String {
    let features = detect_features(&sources.join("\n"));
    let all = split_sections(&language_card());
    let kept: Vec<&Section> = all
        .iter()
        .filter(|s| match &s.heading {
            None => true,
            Some(h) => match FEATURE_HEADINGS.iter().find(|(_, name)| name == h) {
                // 未知の見出しは常に含める
                None => true,
                Some((f, _)) => features.contains(f),
            },
        })
        .collect();
    // 何も落ちていないならフルカードそのまま(「COMPLETE reference」の主張も正しいまま)。
    // 注記に置き換えるのは本当に何か削ったときだけ
    if kept.len() == all.len() {
        return language_card();
    }
    kept.iter()
        .map(|s| match s.heading {
            None => s.body.replacen(FULL_CARD_CLAIM, SUBSET_DISCLAIMER, 1),
            Some(_) => s.body.clone(),
        })
        .collect::<Vec<_>>()
        .join("")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn カード本文を取り出せる() {
        let card = language_card();
        assert!(card.starts_with("# Mesh Language Card"), "先頭: {:?}", card.lines().next());
        // エスケープが解決されている(ソース上は`\``で書かれている箇所)
        assert!(card.contains("`import`"), "バッククォートのエスケープが解決されていない");
        assert!(!card.contains("\\`"), "エスケープが残っている");
        // 文字列補間に見える箇所(ソース上は`\${`)もリテラルとして残る
        assert!(card.contains("${"), "補間記法の説明が消えている");
    }

    #[test]
    fn 機能検出はts版の正規表現と同じ条件() {
        assert!(detect_features("fn first<T>(xs: T[]) T | none {}").contains(&Feature::Generics));
        assert!(!detect_features("fn plain(a: int) int {}").contains(&Feature::Generics));
        assert!(detect_features("type R = { kind: \"ok\" }").contains(&Feature::DiscriminatedUnions));
        assert!(!detect_features("type R = none | error").contains(&Feature::DiscriminatedUnions));
        assert!(detect_features("error type E = {}").contains(&Feature::StructuredErrors));
        assert!(detect_features("error struct E {}").contains(&Feature::StructuredErrors));
        assert!(detect_features("struct User {}").contains(&Feature::Structs));
        assert!(detect_features("xs := []").contains(&Feature::Arrays));
        assert!(detect_features("xs: int[] = []").contains(&Feature::Arrays));
        assert!(detect_features("spawn f()").contains(&Feature::Concurrency));
        // 語境界: `waiting`は`wait`にマッチしない
        assert!(!detect_features("waiting := 1").contains(&Feature::Concurrency));
        assert!(detect_features("import \"util\"").contains(&Feature::Modules));
        assert!(detect_features("defer f()").contains(&Feature::Defer));
        assert!(!detect_features("deferred := 1").contains(&Feature::Defer));
        assert!(detect_features("import \"mesh/http\"").contains(&Feature::HttpServer));
    }

    #[test]
    fn 使っていない機能のセクションは落ちる() {
        let simple = vec!["fn main() {\n\tprint(1)\n}\n".to_string()];
        let card = subset_card(&simple);
        let full = language_card();
        assert!(card.len() < full.len(), "何も落ちていない");
        assert!(!card.contains("## Generic functions"), "使っていないセクションが残っている");
        assert!(card.contains(SUBSET_DISCLAIMER), "サブセットである旨の注記が無い");
        // 常に含めるセクションは残る
        assert!(card.contains("## Program structure"));
    }

    #[test]
    fn 全機能を使っていればフルカードのまま() {
        let all = vec![
            "import \"mesh/http\"\nstruct S {}\nerror type E = {}\ntype R = { kind: \"ok\" }\n\
             fn f<T>(xs: T[]) T | none {}\nfn main() {\n\tdefer g()\n\tspawn h()\n\txs := []\n}\n"
                .to_string(),
        ];
        assert_eq!(subset_card(&all), language_card());
    }
}
