// `mesh test`のハーネス(ランタイムの`__runTests`)が最終行に書くJSONの読み書き。
//
// **なぜ手書きなのか**: このクレートは依存ゼロで運用している(2026-07-25にkanayamaと確認)。
// 読む対象は**自分たちのランタイムが出す固定の形**だけ:
//
//   {"ok":true,"tests":[{"name":"testX","file":"a_test.mesh","pass":true},
//                       {"name":"testY","file":"a_test.mesh","pass":false,"message":"..."}]}
//
// 汎用のJSONパーサではなく、この形に必要な範囲(オブジェクト・配列・文字列・真偽値)だけを
// 読む。**文字列のエスケープ解釈だけは正しく書く必要がある**(テストの失敗メッセージには
// 改行や引用符が入りうるため)ので、そこはテストで固めている。
// 想定外の形(ハーネス自体のバグ・実行が途中で落ちた等)では`None`を返し、呼び出し元が
// 生の出力を見せてエラーにする。

#[derive(Debug, Clone, PartialEq)]
pub struct TestResult {
    pub name: String,
    pub file: String,
    pub pass: bool,
    pub message: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TestReport {
    pub ok: bool,
    pub tests: Vec<TestResult>,
}

pub fn parse(line: &str) -> Option<TestReport> {
    let mut p = Reader { chars: line.chars().collect(), i: 0 };
    p.skip_ws();
    let report = p.report()?;
    p.skip_ws();
    // 末尾にゴミが続いていたら「想定した形ではない」と判断する
    if p.i != p.chars.len() {
        return None;
    }
    Some(report)
}

// `mesh test --json`の出力(TS版は`JSON.stringify(report)`をそのまま出す = 整形なし)
pub fn to_json(report: &TestReport) -> String {
    let tests: Vec<String> = report
        .tests
        .iter()
        .map(|t| {
            let mut fields = format!("\"name\":{},\"file\":{},\"pass\":{}", quote(&t.name), quote(&t.file), t.pass);
            if let Some(m) = &t.message {
                fields.push_str(&format!(",\"message\":{}", quote(m)));
            }
            format!("{{{fields}}}")
        })
        .collect();
    format!("{{\"ok\":{},\"tests\":[{}]}}", report.ok, tests.join(","))
}

fn quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

struct Reader {
    chars: Vec<char>,
    i: usize,
}

impl Reader {
    fn peek(&self) -> Option<char> {
        self.chars.get(self.i).copied()
    }

    fn skip_ws(&mut self) {
        while matches!(self.peek(), Some(c) if c.is_whitespace()) {
            self.i += 1;
        }
    }

    // 期待する1文字を消費する。違えばNone(= 想定した形ではない)
    fn expect(&mut self, c: char) -> Option<()> {
        self.skip_ws();
        if self.peek() == Some(c) {
            self.i += 1;
            Some(())
        } else {
            None
        }
    }

    fn eat(&mut self, c: char) -> bool {
        self.skip_ws();
        if self.peek() == Some(c) {
            self.i += 1;
            true
        } else {
            false
        }
    }

    fn literal(&mut self, word: &str) -> Option<()> {
        self.skip_ws();
        if self.chars[self.i..].starts_with(&word.chars().collect::<Vec<_>>()[..]) {
            self.i += word.chars().count();
            Some(())
        } else {
            None
        }
    }

    fn bool_value(&mut self) -> Option<bool> {
        if self.literal("true").is_some() {
            Some(true)
        } else if self.literal("false").is_some() {
            Some(false)
        } else {
            None
        }
    }

    fn string(&mut self) -> Option<String> {
        self.expect('"')?;
        let mut out = String::new();
        loop {
            let c = self.peek()?;
            self.i += 1;
            match c {
                '"' => return Some(out),
                '\\' => {
                    let esc = self.peek()?;
                    self.i += 1;
                    match esc {
                        '"' => out.push('"'),
                        '\\' => out.push('\\'),
                        '/' => out.push('/'),
                        'n' => out.push('\n'),
                        'r' => out.push('\r'),
                        't' => out.push('\t'),
                        'b' => out.push('\u{8}'),
                        'f' => out.push('\u{c}'),
                        'u' => {
                            // \uXXXX(サロゲートペアは😀のように2つ続く)
                            let hi = self.hex4()?;
                            let ch = if (0xD800..0xDC00).contains(&hi) {
                                // 上位サロゲート——続く\uXXXXと合わせて1文字にする
                                self.expect('\\')?;
                                self.expect('u')?;
                                let lo = self.hex4()?;
                                let combined = 0x10000 + (((hi - 0xD800) as u32) << 10) + (lo - 0xDC00) as u32;
                                char::from_u32(combined)?
                            } else {
                                char::from_u32(hi as u32)?
                            };
                            out.push(ch);
                        }
                        _ => return None,
                    }
                }
                c => out.push(c),
            }
        }
    }

    fn hex4(&mut self) -> Option<u16> {
        let s: String = self.chars.get(self.i..self.i + 4)?.iter().collect();
        self.i += 4;
        u16::from_str_radix(&s, 16).ok()
    }

    // {"ok":bool,"tests":[...]} — キーの順序は問わない(将来ハーネス側で並びが変わっても読める)
    fn report(&mut self) -> Option<TestReport> {
        self.expect('{')?;
        let mut ok = None;
        let mut tests = None;
        loop {
            if self.eat('}') {
                break;
            }
            let key = self.string()?;
            self.expect(':')?;
            match key.as_str() {
                "ok" => ok = Some(self.bool_value()?),
                "tests" => tests = Some(self.test_array()?),
                // 知らないキーは読み飛ばせないので想定外として扱う(この形は自分たちが出す)
                _ => return None,
            }
            if !self.eat(',') {
                self.expect('}')?;
                break;
            }
        }
        Some(TestReport { ok: ok?, tests: tests? })
    }

    fn test_array(&mut self) -> Option<Vec<TestResult>> {
        self.expect('[')?;
        let mut items = Vec::new();
        if self.eat(']') {
            return Some(items);
        }
        loop {
            items.push(self.test_result()?);
            if self.eat(',') {
                continue;
            }
            self.expect(']')?;
            return Some(items);
        }
    }

    fn test_result(&mut self) -> Option<TestResult> {
        self.expect('{')?;
        let (mut name, mut file, mut pass, mut message) = (None, None, None, None);
        loop {
            if self.eat('}') {
                break;
            }
            let key = self.string()?;
            self.expect(':')?;
            match key.as_str() {
                "name" => name = Some(self.string()?),
                "file" => file = Some(self.string()?),
                "pass" => pass = Some(self.bool_value()?),
                "message" => message = Some(self.string()?),
                _ => return None,
            }
            if !self.eat(',') {
                self.expect('}')?;
                break;
            }
        }
        Some(TestResult { name: name?, file: file?, pass: pass?, message })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 成功だけのレポートを読める() {
        let r = parse(r#"{"ok":true,"tests":[{"name":"testA","file":"a_test.mesh","pass":true}]}"#).unwrap();
        assert!(r.ok);
        assert_eq!(r.tests.len(), 1);
        assert_eq!(r.tests[0].name, "testA");
        assert_eq!(r.tests[0].message, None);
    }

    #[test]
    fn 失敗メッセージ付きを読める() {
        let r = parse(r#"{"ok":false,"tests":[{"name":"t","file":"f","pass":false,"message":"boom"}]}"#).unwrap();
        assert!(!r.ok);
        assert_eq!(r.tests[0].message.as_deref(), Some("boom"));
    }

    #[test]
    fn 文字列のエスケープを正しく解釈する() {
        // 失敗メッセージには改行・引用符・バックスラッシュが入りうる
        let r = parse(r#"{"ok":false,"tests":[{"name":"t","file":"f","pass":false,"message":"line1\nline2 \"q\" \\ \t"}]}"#).unwrap();
        assert_eq!(r.tests[0].message.as_deref(), Some("line1\nline2 \"q\" \\ \t"));
    }

    #[test]
    fn unicodeエスケープとサロゲートペアを読める() {
        let r = parse(r#"{"ok":true,"tests":[{"name":"あ😀","file":"f","pass":true}]}"#).unwrap();
        assert_eq!(r.tests[0].name, "あ😀");
    }

    #[test]
    fn テストが空でも読める() {
        let r = parse(r#"{"ok":true,"tests":[]}"#).unwrap();
        assert!(r.ok);
        assert!(r.tests.is_empty());
    }

    #[test]
    fn 想定外の形は読み取りに失敗する() {
        // JSONでない / 途中で切れている / 知らないキー / 末尾にゴミ
        assert_eq!(parse("cleanup ran"), None);
        assert_eq!(parse(r#"{"ok":true,"tests":["#), None);
        assert_eq!(parse(r#"{"ok":true,"tests":[],"extra":1}"#), None);
        assert_eq!(parse(r#"{"ok":true,"tests":[]} trailing"#), None);
        assert_eq!(parse(r#"{"tests":[]}"#), None); // okが無い
    }

    #[test]
    fn 書き出しと読み取りが往復する() {
        let report = TestReport {
            ok: false,
            tests: vec![
                TestResult { name: "testA".into(), file: "a_test.mesh".into(), pass: true, message: None },
                TestResult { name: "testB".into(), file: "a_test.mesh".into(), pass: false, message: Some("expected 1, got 2\n\"x\"".into()) },
            ],
        };
        assert_eq!(parse(&to_json(&report)).unwrap(), report);
    }
}
