// ブラウザ向けのwasmエントリポイント(TS撤去 段階4)。
//
// playgroundはこれまで `import { compile } from "../src/compiler"` でTS実装を直接読んでいた。
// **TS版を撤去するとplaygroundが動かなくなる**ので、Rust版をwasmとして公開する。
//
// **`wasm-bindgen`は使わない**——この移植は依存クレート0で来ており、
// playgroundのために依存とビルドツール(`wasm-bindgen-cli`)を増やす釣り合いが取れないため。
// 必要なのは「文字列を渡して文字列を受け取る」だけなので、手書きのABIで足りる:
//
//   1. JS側が `mesh_alloc(len)` でwasm線形メモリ上にバッファを確保する
//   2. JS側がそこへUTF-8のソースを書き込み、`mesh_compile(ptr, len)` を呼ぶ
//   3. 戻り値は結果文字列の**先頭ポインタ**。長さは `mesh_last_len()` で取る
//      (wasmの戻り値は1つしか返せないため。2値を返すのに構造体を使うと
//      ABIがツールチェーン依存になるので避けた)
//   4. JS側が読み終わったら `mesh_free(ptr, len)` で解放する
//
// 結果文字列の形式は **1行目が `ok` か `error`、2行目以降が本体**。
// JSONにしないのは、シリアライザを持ち込まずに済ませるため(診断は`format_diagnostics`が
// 既に人間可読な整形を持っており、playgroundはそれをそのまま表示すればよい)。

use crate::codegen;
use crate::full_checker;
use crate::json_decode::{synthesize_json_decoders, synthesize_json_encoders};
use crate::parser::parse;

// 直前に返した文字列の長さ。`mesh_compile`が返せるのはポインタ1つだけなので、
// 長さはここへ置いてJS側が読みに来る(呼び出しは常に compile → last_len の順で、
// wasmはシングルスレッドなので競合しない)
static mut LAST_LEN: usize = 0;

/// JS側がソースを書き込むためのバッファを確保する。
///
/// # Safety
/// 返ったポインタは `mesh_free(ptr, len)` で解放すること。
#[unsafe(no_mangle)]
pub extern "C" fn mesh_alloc(len: usize) -> *mut u8 {
    let mut buf = Vec::<u8>::with_capacity(len);
    let ptr = buf.as_mut_ptr();
    std::mem::forget(buf);
    ptr
}

/// `mesh_alloc` / `mesh_compile` が返したバッファを解放する。
///
/// # Safety
/// `ptr`/`len` は同じ組でこのモジュールが返したものであること。
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mesh_free(ptr: *mut u8, len: usize) {
    if !ptr.is_null() {
        drop(unsafe { Vec::from_raw_parts(ptr, len, len) });
    }
}

/// 直前に `mesh_compile` が返した文字列の長さ。
#[unsafe(no_mangle)]
pub extern "C" fn mesh_last_len() -> usize {
    unsafe { LAST_LEN }
}

fn leak_string(s: String) -> *mut u8 {
    let mut bytes = s.into_bytes();
    bytes.shrink_to_fit();
    let ptr = bytes.as_mut_ptr();
    unsafe { LAST_LEN = bytes.len() };
    std::mem::forget(bytes);
    ptr
}

/// ソース(UTF-8)を1本コンパイルする。戻り値は結果文字列の先頭ポインタで、
/// 長さは `mesh_last_len()` で取る。
///
/// # Safety
/// `ptr`/`len` は `mesh_alloc` で確保したUTF-8バイト列を指していること。
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mesh_compile(ptr: *const u8, len: usize) -> *mut u8 {
    let bytes = unsafe { std::slice::from_raw_parts(ptr, len) };
    let source = match std::str::from_utf8(bytes) {
        Ok(s) => s,
        Err(_) => return leak_string("error\nsource is not valid UTF-8".to_string()),
    };
    leak_string(compile_source(source))
}

// **CLIの単一ファイル経路と同じ順序**でコンパイルする(parse → json struct合成 →
// full_checker → codegen)。playgroundはimportを扱わない(ブラウザにファイルシステムが
// 無いので`modules.rs`は使えない)——TS版playgroundも単一ソースだけを扱っていた
fn compile_source(source: &str) -> String {
    let mut program = match parse(source) {
        Ok(p) => p,
        Err(errors) => {
            let body: Vec<String> = errors
                .iter()
                .map(|e| format!("playground.mesh:{}:{}: error[{}]: {}", e.pos.line, e.pos.col, e.code, e.message))
                .collect();
            return format!("error\n{}", body.join("\n"));
        }
    };
    // `json struct` からデコーダ/エンコーダを合成する(CLIと同じ前処理)
    if let Err(e) = synthesize_json_decoders(&mut program) {
        return format!("error\nplayground.mesh:{}:{}: error[{}]: {}", e.pos.line, e.pos.col, e.code, e.message);
    }
    if let Err(e) = synthesize_json_encoders(&mut program) {
        return format!("error\nplayground.mesh:{}:{}: error[{}]: {}", e.pos.line, e.pos.col, e.code, e.message);
    }

    let diagnostics = full_checker::check_program(&program);
    if !diagnostics.is_empty() {
        let body: Vec<String> = diagnostics
            .iter()
            .map(|d| format!("playground.mesh:{}:{}: error[{}]: {}", d.pos.line, d.pos.col, d.code, d.message))
            .collect();
        return format!("error\n{}", body.join("\n"));
    }

    match codegen::generate(&program, "playground.mesh") {
        Ok(js) => format!("ok\n{js}"),
        Err(e) => format!("error\nplayground.mesh: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::compile_source;

    #[test]
    fn 正しいソースはokとjsを返す() {
        let out = compile_source("fn main() {\n\tprint(1)\n}\n");
        let (head, body) = out.split_once('\n').expect("1行目に判定が入る");
        assert_eq!(head, "ok");
        assert!(body.contains("async function main()"), "{body}");
    }

    #[test]
    fn 型エラーはerrorと診断を返す() {
        let out = compile_source("fn main() {\n\tprint(1 + \"s\")\n}\n");
        let (head, body) = out.split_once('\n').expect("1行目に判定が入る");
        assert_eq!(head, "error");
        assert!(body.contains("error[invalid-operation]"), "{body}");
        assert!(body.contains("playground.mesh:2:"), "位置が付くこと: {body}");
    }

    #[test]
    fn 構文エラーもerrorで返る() {
        let out = compile_source("fn main() {\n\tx := \n}\n");
        assert!(out.starts_with("error\n"), "{out}");
        assert!(out.contains("error[syntax-error]"), "{out}");
    }

    #[test]
    fn json_structのデコーダとエンコーダが合成される() {
        // CLIと同じ前処理を通していることの確認——通していないと`decodeUser`が
        // undefined-nameになる
        let out = compile_source("import \"mesh/json\"\njson struct User {\n\tname: string\n}\nfn main() {\n\tprint(1)\n}\n");
        assert!(out.starts_with("ok\n"), "{out}");
        assert!(out.contains("decodeUser"), "デコーダが出ていない");
        assert!(out.contains("encodeUser"), "エンコーダが出ていない");
    }
}
