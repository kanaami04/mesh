// `mesh/http` の実行時テスト(TS撤去 段階3)。TS版 `tests/http.test.ts` の移植。
//
// **移植前、Rust側にはmesh/httpを実際に動かすテストが1件も無かった**
// (`tests/parity/56-http-listen-signature/` が`mesh check`を見るだけ)。
// 生成JSはTS版とバイト一致しているので当面は壊れないが、**TS実装を撤去すると
// HTTPサーバーの挙動を誰も検証しなくなる**ため移した。
//
// HTTPクライアントは`std::net::TcpStream`で素書きする——この移植はここまで
// 依存クレート0で来ており、テストのためだけに依存を増やしたくないため。
// 必要なのは「リクエストを1本投げてステータス・ヘッダ・ボディを読む」だけなので、
// HTTP/1.1のごく一部で足りる(`Connection: close`で応答終端をEOFに任せる)。

use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

fn mesh_bin() -> PathBuf {
    // `cargo test` は target/debug/deps/ にテストバイナリを置く。CLIはその2つ上
    let mut p = std::env::current_exe().expect("テストバイナリのパス");
    p.pop();
    if p.ends_with("deps") {
        p.pop();
    }
    p.join("mesh")
}

// **`MESH_TEST_JS_RUNTIME`で明示指定できる**——bunとnodeでHTTPの応答形式が違い
// (nodeはchunked transfer-encodingで返す)、手元がbunだと**CIのnodeでだけ落ちる**
// 差を踏む。実際にこのテストの初版がそれで落ちた
fn js_runtime() -> Option<&'static str> {
    if let Ok(explicit) = std::env::var("MESH_TEST_JS_RUNTIME") {
        return ["bun", "node"].into_iter().find(|c| *c == explicit);
    }
    ["bun", "node"]
        .into_iter()
        .find(|c| Command::new(c).arg("--version").stdout(Stdio::null()).stderr(Stdio::null()).status().is_ok_and(|s| s.success()))
}

// 起動しっぱなしのサーバー。**Dropで必ずkillする**——テストが失敗しても
// 子プロセスが残ってポートを掴み続けないように(TS版のafterEachに相当)
struct Server {
    child: Child,
    dir: PathBuf,
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

impl Server {
    fn stderr(&mut self) -> String {
        // killした後に残りを読む(起動中はブロックするので呼ぶのは終了後)
        let _ = self.child.kill();
        let _ = self.child.wait();
        let mut s = String::new();
        if let Some(e) = self.child.stderr.as_mut() {
            let _ = e.read_to_string(&mut s);
        }
        s
    }
}

// Meshソースをビルドして起動し、リッスンし始めるまで待つ
fn start_server(name: &str, source: &str, port: u16) -> Server {
    let runtime = js_runtime().expect("bun/node が要る(呼び出し側で確認済みのはず)");
    let dir = std::env::temp_dir().join(format!("mesh-http-test-{name}-{port}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("一時ディレクトリを作れること");
    let src_path = dir.join("main.mesh");
    let js_path = dir.join("out.mjs");
    std::fs::write(&src_path, source).expect("ソースを書けること");

    let out = Command::new(mesh_bin())
        .args(["build", src_path.to_str().unwrap(), "-o", js_path.to_str().unwrap()])
        .output()
        .expect("mesh build を起動できること");
    assert!(out.status.success(), "build失敗: {}", String::from_utf8_lossy(&out.stderr));

    let child = Command::new(runtime)
        .arg(&js_path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("サーバーを起動できること");
    let mut server = Server { child, dir };

    // リッスン開始まで待つ(TS版と同じくreadiness probe方式)
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        if TcpStream::connect(("127.0.0.1", port)).is_ok() {
            return server;
        }
        if server.child.try_wait().is_ok_and(|s| s.is_some()) {
            panic!("サーバーが起動直後に落ちた:\n{}", server.stderr());
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    panic!("サーバーがリッスンを始めなかった:\n{}", server.stderr());
}

struct Res {
    status: u16,
    headers: Vec<(String, String)>,
    body: String,
}

impl Res {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
    }
}

// HTTP/1.1のリクエストを1本投げて応答を読む。`Connection: close`にして
// 応答本文の終端をEOFに任せる(chunked/keep-aliveの解釈を書かずに済む)
fn request(port: u16, method: &str, path: &str, headers: &[(&str, &str)], body: &[u8]) -> Res {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("サーバーへ接続できること");
    stream.set_read_timeout(Some(Duration::from_secs(30))).ok();
    let mut req = format!("{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n");
    for (k, v) in headers {
        req.push_str(&format!("{k}: {v}\r\n"));
    }
    req.push_str(&format!("Content-Length: {}\r\n\r\n", body.len()));
    stream.write_all(req.as_bytes()).expect("リクエストヘッダを送れること");
    stream.write_all(body).expect("リクエストボディを送れること");
    stream.flush().ok();

    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).expect("応答を読めること");
    let text = String::from_utf8_lossy(&raw).into_owned();
    let (head, body) = text.split_once("\r\n\r\n").unwrap_or_else(|| panic!("応答がHTTPの形をしていない: {text:?}"));
    let mut lines = head.split("\r\n");
    let status_line = lines.next().unwrap_or_default();
    let status: u16 = status_line.split(' ').nth(1).and_then(|s| s.parse().ok()).unwrap_or_else(|| panic!("ステータス行が読めない: {status_line:?}"));
    let headers: Vec<(String, String)> =
        lines.filter_map(|l| l.split_once(':').map(|(k, v)| (k.trim().to_string(), v.trim().to_string()))).collect();
    // **chunked transfer-encodingを解く**。`Connection: close`を送ってもサーバーは
    // chunkedで返してよく、実際にnodeはそうする(bunは素で返すため手元では素通りし、
    // **CIのnodeで初めて落ちた**)。ここを解かないとボディに長さ行が混ざる
    let chunked = headers.iter().any(|(k, v)| k.eq_ignore_ascii_case("transfer-encoding") && v.to_ascii_lowercase().contains("chunked"));
    let body = if chunked { decode_chunked(body) } else { body.to_string() };
    Res { status, headers, body }
}

// `<長さ(16進)>\r\n<データ>\r\n` の繰り返しを、長さ0の塊まで読む
fn decode_chunked(raw: &str) -> String {
    let mut out = String::new();
    let mut rest = raw;
    while let Some((size_line, after)) = rest.split_once("\r\n") {
        // 拡張(`;`以降)は捨てる
        let size_hex = size_line.split(';').next().unwrap_or("").trim();
        let Ok(size) = usize::from_str_radix(size_hex, 16) else { break };
        if size == 0 {
            break;
        }
        if after.len() < size {
            out.push_str(after); // 途中で切れている(想定外だが握りつぶさず入るだけ入れる)
            break;
        }
        out.push_str(&after[..size]);
        rest = after[size..].strip_prefix("\r\n").unwrap_or(&after[size..]);
    }
    out
}

fn get(port: u16, path: &str) -> Res {
    request(port, "GET", path, &[], b"")
}

// bun/node が無い環境ではスキップする(cli.rs と同じ方針)
macro_rules! require_js {
    () => {
        if js_runtime().is_none() {
            eprintln!("skip: bun/node が見つからないため mesh/http のテストをスキップ");
            return;
        }
    };
}

#[test]
fn パスで分岐しクエリ文字列を読み200と404を返す() {
    require_js!();
    let src = r#"import "mesh/http"

fn handler(req: http.Request) http.Response {
	if req.path == "/hello" {
		return http.Response{status: 200, body: "hello:${req.query}", headers: map<string, string>{}}
	}
	return http.Response{status: 404, body: "not found: ${req.path}", headers: map<string, string>{}}
}

fn main() {
	http.listen("127.0.0.1:18280", handler)
}
"#;
    let _s = start_server("route", src, 18280);
    let ok = get(18280, "/hello?name=world");
    assert_eq!(ok.status, 200);
    assert_eq!(ok.body, "hello:name=world");

    let missing = get(18280, "/nope");
    assert_eq!(missing.status, 404);
    assert_eq!(missing.body, "not found: /nope");
}

#[test]
fn リクエストヘッダを読みレスポンスヘッダを設定できる() {
    require_js!();
    let src = r#"import "mesh/http"

fn handler(req: http.Request) http.Response {
	got := req.headers["x-request-id"]
	if got is none {
		return http.Response{status: 400, body: "missing header", headers: map<string, string>{}}
	}
	mut headers := map<string, string>{}
	headers["x-echoed"] = got
	return http.Response{status: 200, body: "ok", headers: headers}
}

fn main() {
	http.listen("127.0.0.1:18281", handler)
}
"#;
    let _s = start_server("headers", src, 18281);
    let res = request(18281, "GET", "/", &[("X-Request-Id", "abc123")], b"");
    assert_eq!(res.status, 200);
    assert_eq!(res.header("x-echoed"), Some("abc123"), "{:?}", res.headers);
}

#[test]
fn postのbodyを読める_json_structデコードと組み合わせ() {
    require_js!();
    let src = r#"import "mesh/http"
import "mesh/json"

json struct CreateUser {
	name: string
	age: int
}

fn handler(req: http.Request) http.Response {
	v := json.parse(req.body)
	if v is error {
		return http.Response{status: 400, body: "bad json", headers: map<string, string>{}}
	}
	u := decodeCreateUser(v)
	if u is error {
		return http.Response{status: 400, body: "${u}", headers: map<string, string>{}}
	}
	return http.Response{status: 201, body: "created:${u.name}:${u.age}", headers: map<string, string>{}}
}

fn main() {
	http.listen("127.0.0.1:18282", handler)
}
"#;
    let _s = start_server("post", src, 18282);
    let created = request(18282, "POST", "/users", &[], br#"{"name":"alice","age":30}"#);
    assert_eq!(created.status, 201);
    assert_eq!(created.body, "created:alice:30");

    // age欠落 → デコード失敗
    let bad = request(18282, "POST", "/users", &[], br#"{"name":"bob"}"#);
    assert_eq!(bad.status, 400);
    assert!(bad.body.contains("missing field 'age'"), "body: {}", bad.body);
}

#[test]
fn 障害分離_1リクエストのpanicは500になりサーバーは応答し続ける() {
    require_js!();
    let src = r#"import "mesh/http"

fn handler(req: http.Request) http.Response {
	if req.path == "/boom" {
		xs := [1, 2]
		return http.Response{status: 200, body: "${xs[10]}", headers: map<string, string>{}}
	}
	return http.Response{status: 200, body: "alive", headers: map<string, string>{}}
}

fn main() {
	http.listen("127.0.0.1:18283", handler)
}
"#;
    let mut s = start_server("panic", src, 18283);
    let boom = get(18283, "/boom");
    assert_eq!(boom.status, 500);
    // 内部のpanic詳細はクライアントに漏らさない
    assert_eq!(boom.body, "internal server error");

    let still_alive = get(18283, "/anything");
    assert_eq!(still_alive.status, 200);
    assert_eq!(still_alive.body, "alive");

    // panicの詳細はサーバー側のstderrにだけ出る
    let err = s.stderr();
    assert!(err.contains("index 10 out of range"), "stderr: {err}");
}

#[test]
fn 巨大なリクエストボディは413で隔離されサーバーは応答し続ける() {
    require_js!();
    let src = r#"import "mesh/http"

fn handler(req: http.Request) http.Response {
	return http.Response{status: 200, body: "len:${len(req.body)}", headers: map<string, string>{}}
}

fn main() {
	http.listen("127.0.0.1:18285", handler)
}
"#;
    let _s = start_server("toolarge", src, 18285);
    let too_large = request(18285, "POST", "/upload", &[], &vec![b'a'; 11 * 1024 * 1024]); // 11MiB > 10MiBの上限
    assert_eq!(too_large.status, 413);
    assert_eq!(too_large.body, "request body too large");

    // 上限超過の直後でも通常サイズには応答できる(プロセスが生きている証拠)
    let ok = request(18285, "POST", "/upload", &[], b"hi");
    assert_eq!(ok.status, 200);
    assert_eq!(ok.body, "len:2");
}

#[test]
fn 同じポートに2度listenすると2度目はerrorになりプロセスは落ちない() {
    require_js!();
    let src = r#"import "mesh/http"

fn handler(req: http.Request) http.Response {
	return http.Response{status: 200, body: "first", headers: map<string, string>{}}
}

fn main() {
	r1 := http.listen("127.0.0.1:18284", handler)
	if r1 is error {
		print("unexpected: ${r1}")
		return
	}
	r2 := http.listen("127.0.0.1:18284", handler)
	if r2 is error {
		print("second listen failed as expected")
		return
	}
	print("should not happen")
}
"#;
    let _s = start_server("double-listen", src, 18284);
    // 1本目のサーバーは生きたまま応答し続ける(2本目の失敗でプロセスが落ちていない証拠)
    let res = get(18284, "/");
    assert_eq!(res.status, 200);
    assert_eq!(res.body, "first");
}
