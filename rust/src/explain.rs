// `mesh explain <code>`(F-13前半)。診断コードごとの「この種類のエラーは何を意味するか」を
// 出力する。
//
// **説明文はTS版`src/diagnostic-codes.ts`の`DIAGNOSTIC_EXPLANATIONS`をそのまま使う**——
// card.rsがカード本文をTS版から取り出しているのと同じ方針(`include_str!`で埋め込み、
// 必要な部分だけ取り出す)。Rust側へ本文を複製すると、TS側の文言が直ったときに片方だけ
// 古くなる。説明文は「一般論」なので実装の進捗とは独立していて、複製する価値が無い。
//
// **出す範囲はRust版が実際に出せる診断コードだけ**(2026-07-25にkanayamaさんと確認)。
// TS版は107種すべてを説明できるが、Rust版のfull_checkerが出すのはその一部
// (milestone 38時点で36種、milestone 39で44種)。存在しない検査の説明を並べると
// 「Rust版でもその診断が出る」という誤解を招くので、`DiagnosticCode::ALL`
// (=実装済みの検査)に絞る。したがって:
//   - `mesh explain`(引数無し)の件数行はTS版の107にならない(意図的な差)
//   - Rust版が未実装のコード(例: `generic-inference-failed`)は`unknown diagnostic code`になる
//     (**この例は診断を移植するたび古くなる**——milestone 47で`narrow-required`を
//     移植したときに差し替えた。下のテストも同じ理由で毎回更新が要る)
// 検査を足すたびにDiagnosticCodeへ変体が増え、ここも自動的に追随する。
//
// 抽出が壊れていないことは`すべての診断コードに説明文がある`テストが担保する——TS側の
// 整形が変わって読めなくなれば`cargo test`が即座に落ちる(実行時に初めて気づく、を避ける)。

use crate::diagnostic_codes::DiagnosticCode;

const CODES_TS: &str = include_str!("../embedded/diagnostic-codes.ts");

// `mesh explain <code>`の本文。未実装コード・未知のコードはNone
pub fn explanation(code: &str) -> Option<String> {
    if !DiagnosticCode::ALL.iter().any(|c| c.as_str() == code) {
        return None;
    }
    parse_explanations().into_iter().find(|(k, _)| k == code).map(|(_, v)| v)
}

// 引数無しの一覧用。TS版は`Object.keys(...).sort()`なので辞書順(ASCII)
pub fn all_codes() -> Vec<&'static str> {
    let mut codes: Vec<&'static str> = DiagnosticCode::ALL.iter().map(|c| c.as_str()).collect();
    codes.sort_unstable();
    codes
}

// `DIAGNOSTIC_EXPLANATIONS`のオブジェクトリテラルを読む。対象は自分たちが書いた
// prettier整形済みの固定の形:
//
//   "code-name":
//     "文の前半 " +
//     "後半",
//
// なので汎用のTSパーサは要らない(依存ゼロを保つ)。読めない形に出会ったらそこで
// 打ち切る——**panicはしない**(`mesh explain`が落ちるより、説明が出ない方がまし)。
// 打ち切りはテストが検知する。
fn parse_explanations() -> Vec<(String, String)> {
    const MARKER: &str = "DIAGNOSTIC_EXPLANATIONS: Record<DiagnosticCode, string> = {";
    let mut out = Vec::new();
    let Some(start) = CODES_TS.find(MARKER) else { return out };
    let chars: Vec<char> = CODES_TS[start + MARKER.len()..].chars().collect();
    let mut i = 0;
    loop {
        skip_ws(&chars, &mut i);
        // オブジェクトの終わり(あるいは読めない形)
        if chars.get(i) != Some(&'"') {
            return out;
        }
        let Some(key) = read_string(&chars, &mut i) else { return out };
        skip_ws(&chars, &mut i);
        if chars.get(i) != Some(&':') {
            return out;
        }
        i += 1;
        // `"..." + "..." + ...` の連結
        let mut value = String::new();
        loop {
            skip_ws(&chars, &mut i);
            let Some(part) = read_string(&chars, &mut i) else { return out };
            value.push_str(&part);
            skip_ws(&chars, &mut i);
            if chars.get(i) == Some(&'+') {
                i += 1;
                continue;
            }
            break;
        }
        out.push((key, value));
        if chars.get(i) == Some(&',') {
            i += 1;
        }
    }
}

fn skip_ws(chars: &[char], i: &mut usize) {
    while matches!(chars.get(*i), Some(c) if c.is_whitespace()) {
        *i += 1;
    }
}

// `"..."`を1つ読む。JSのエスケープ規則に従い、未知のエスケープは後続の文字そのものを返す
// (説明文に実際に現れるのは`\"`だけだが、増えても壊れないようにしておく)
fn read_string(chars: &[char], i: &mut usize) -> Option<String> {
    if chars.get(*i) != Some(&'"') {
        return None;
    }
    *i += 1;
    let mut out = String::new();
    loop {
        let c = *chars.get(*i)?;
        *i += 1;
        match c {
            '"' => return Some(out),
            '\\' => {
                let esc = *chars.get(*i)?;
                *i += 1;
                out.push(match esc {
                    'n' => '\n',
                    't' => '\t',
                    'r' => '\r',
                    other => other,
                });
            }
            c => out.push(c),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn すべての診断コードに説明文がある() {
        // TS側の整形が変わって抽出が壊れたらここで落ちる(=実行時に気づくのを避ける)
        for code in DiagnosticCode::ALL {
            let text = explanation(code.as_str());
            assert!(text.is_some(), "{} の説明文が取り出せない", code.as_str());
            assert!(!text.unwrap().trim().is_empty(), "{} の説明文が空", code.as_str());
        }
    }

    #[test]
    fn 連結された文字列がつながる() {
        // TS版は`"..." + "..."`の連結で書かれている。区切りが落ちたり余計な空白が入ったり
        // しないこと(TS版の出力とbyte-for-byteで一致させるため)
        let text = explanation("division-by-zero").unwrap();
        assert_eq!(
            text,
            "Integer division or modulo by the literal 0 is caught at compile time — it would always panic at \
             runtime, so there's no reason to wait until then to report it."
        );
    }

    #[test]
    fn エスケープを解釈する() {
        // `\"`を含む説明文(TSソース上はバックスラッシュ付きで書かれている)
        let text = explanation("discriminated-union-tag-missing").unwrap();
        assert!(text.contains("write 'kind: \"ok\"'"), "{text}");
    }

    #[test]
    fn 存在しないコードは説明しない() {
        // **milestone 62でTS版の107種を全部移植したので、「未実装だから説明しない」例は
        // もう作れない**(この関数はもともと`narrow-required`→`generic-inference-failed`と
        // 移植のたびに差し替えてきた)。残る不変条件は「TS版に無いコードは説明しない」だけ。
        // `interpolation-too-deep`はRust固有の安全弁(parser.rsのMAX_INTERP_DEPTH)で、
        // TS版に対応する説明文が無い——`DiagnosticCode`へ載せない理由そのもの
        assert_eq!(explanation("interpolation-too-deep"), None);
        assert_eq!(explanation("no-such-code"), None);
    }

    #[test]
    fn 一覧は辞書順() {
        let codes = all_codes();
        assert_eq!(codes.len(), DiagnosticCode::ALL.len());
        let mut sorted = codes.clone();
        sorted.sort_unstable();
        assert_eq!(codes, sorted);
    }
}
