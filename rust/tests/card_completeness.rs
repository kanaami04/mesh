// カード完全性の逆方向検査(F-13後半)。TS版`tests/card-completeness.test.ts`の移植
// (TS撤去 段階2)。
//
// **実装がカードからはみ出す**(=カードに書いていない名前を受け付けてしまう)ことを防ぐ。
// 「実装からその都度全部拾う」一般的な方法は無いので、`BUILTINS`/`RESERVED`という既存の
// 正典リストに新しい名前が足されたとき、カードの該当節に書き忘れていないかに絞っている。
// 過去の実例(`eval`/`arguments`をRESERVEDへ追加した際にカード追記を忘れかけた)が
// まさにこの形の見落としだった。
//
// **TS版はTS実装のリストを見ていたが、こちらはRust実装のリストを見る**——カードは
// 両実装で共通(`rust/embedded/card.ts`)なので、実装側の正典が変わったときに落ちる。
// TS実装を撤去した後もこの検査が残るようにするのが、この移植の目的。

use mesh::checker::BUILTINS;
use mesh::full_checker::RESERVED;

// `## <見出し>` で始まる節を取り出す
fn card_section(card: &str, heading: &str) -> String {
    let marker = format!("## {heading}\n");
    let start = card
        .find(&marker)
        .unwrap_or_else(|| panic!("カードに節が無い: \"{heading}\"(見出し文字列がcard.tsとズレていないか確認)"));
    let rest = &card[start + marker.len()..];
    let end = rest.find("\n## ").unwrap_or(rest.len());
    rest[..end].to_string()
}

// 単語境界つきで含まれるか(TS版の`\b<name>\b`に対応)。`in`が`instanceof`に
// 誤マッチするのを防ぐ——素の`contains`だと通ってしまい、検査が空振りする
fn contains_word(haystack: &str, word: &str) -> bool {
    haystack.match_indices(word).any(|(i, _)| {
        let before = haystack[..i].chars().next_back();
        let after = haystack[i + word.len()..].chars().next();
        let boundary = |c: Option<char>| c.is_none_or(|c| !c.is_alphanumeric() && c != '_');
        boundary(before) && boundary(after)
    })
}

#[test]
fn builtinsの全関数名がカードのbuiltins節に載っている() {
    let card = mesh::card::language_card();
    let section = card_section(&card, "Builtins (complete list)");
    let missing: Vec<&str> = BUILTINS.iter().copied().filter(|n| !contains_word(&section, n)).collect();
    assert!(missing.is_empty(), "カードのBuiltins節に載っていない組み込み関数: {missing:?}");
}

#[test]
fn reservedの全予約語がカードのbindings節に載っている() {
    let card = mesh::card::language_card();
    let section = card_section(&card, "Bindings (immutable by default)");
    let missing: Vec<&str> = RESERVED.iter().copied().filter(|n| !contains_word(&section, n)).collect();
    assert!(missing.is_empty(), "カードのBindings節に載っていない予約語: {missing:?}");
}

#[test]
fn 検査自体が空振りしていない() {
    // **この検査で一番怖いのは「節が取れずに全部通る」**こと。存在しない名前が
    // ちゃんと「載っていない」と判定されることを確かめて、空振りを防ぐ
    let card = mesh::card::language_card();
    let section = card_section(&card, "Builtins (complete list)");
    assert!(!section.is_empty(), "節が空(見出しの取り出しが壊れている)");
    assert!(!contains_word(&section, "definitelyNotABuiltin"), "存在しない名前が「載っている」と判定された");
    assert!(contains_word(&section, "print"), "実在する組み込みを拾えていない");
}
