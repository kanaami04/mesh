// C-6続き: mesh/http のe2eテスト。
// http.listenは実プロセスを立ち上げっぱなしにする(生きているソケットがイベントループを
// 保持する)ので、他のe2eテストのような spawnSync(「終了して0を返す」を待つ)は使えない。
// 代わりに spawn で子プロセスを起動したまま残し、実ネットワーク越しに fetch でリクエストし、
// アサーション後に必ずkillする
import { afterEach, describe, expect, test } from "bun:test";
import { type ChildProcess, spawn } from "node:child_process";
import { mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { compile } from "../src/compiler";

const children: ChildProcess[] = [];
afterEach(() => {
  for (const c of children.splice(0)) c.kill();
});

// portごとにMeshソースをコンパイル→起動し、リッスンし始めるまで待ってから使わせる。
// 呼び出し元はfetchでリクエストするだけでよく、後片付け(kill)はafterEachに任せる
async function startServer(source: string, port: number): Promise<{ stderr: () => string }> {
  const result = compile(source);
  if (result.code === null) {
    throw new Error("compile failed:\n" + result.diagnostics.map((d) => d.message).join("\n"));
  }
  const dir = mkdtempSync(join(tmpdir(), "mesh-http-test-"));
  const path = join(dir, "out.mjs");
  writeFileSync(path, result.code);
  const proc = spawn(process.execPath, [path], { stdio: ["ignore", "pipe", "pipe"] });
  children.push(proc);
  let stderr = "";
  proc.stderr.on("data", (d) => (stderr += d.toString()));
  let stdout = "";
  proc.stdout.on("data", (d) => (stdout += d.toString()));

  const deadline = Date.now() + 5000;
  for (;;) {
    if (proc.exitCode !== null) {
      throw new Error(`server process exited early (code ${proc.exitCode}):\nstdout: ${stdout}\nstderr: ${stderr}`);
    }
    try {
      await fetch(`http://127.0.0.1:${port}/__mesh_http_test_readiness_probe`);
      break;
    } catch {
      if (Date.now() > deadline) throw new Error("server never became ready:\n" + stderr);
      await new Promise((r) => setTimeout(r, 20));
    }
  }
  return { stderr: () => stderr };
}

describe("C-6続き: mesh/http(検証つきサーバーAPI)", () => {
  test("パスで分岐し、クエリ文字列を読み、200/404を返す", async () => {
    await startServer(
      `import "mesh/http"

fn handler(req: http.Request) http.Response {
	if req.path == "/hello" {
		return http.Response{status: 200, body: "hello:\${req.query}", headers: map<string, string>{}}
	}
	return http.Response{status: 404, body: "not found: \${req.path}", headers: map<string, string>{}}
}

fn main() {
	http.listen("127.0.0.1:18180", handler)
}`,
      18180,
    );

    const ok = await fetch("http://127.0.0.1:18180/hello?name=world");
    expect(ok.status).toBe(200);
    expect(await ok.text()).toBe("hello:name=world");

    const missing = await fetch("http://127.0.0.1:18180/nope");
    expect(missing.status).toBe(404);
    expect(await missing.text()).toBe("not found: /nope");
  });

  test("リクエストヘッダを読み、レスポンスヘッダを設定できる", async () => {
    await startServer(
      `import "mesh/http"

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
	http.listen("127.0.0.1:18181", handler)
}`,
      18181,
    );

    const res = await fetch("http://127.0.0.1:18181/", { headers: { "X-Request-Id": "abc123" } });
    expect(res.status).toBe(200);
    expect(res.headers.get("x-echoed")).toBe("abc123");
  });

  test("POSTのbodyを読める(mesh/jsonのjson structデコードと組み合わせ)", async () => {
    await startServer(
      `import "mesh/http"
import "mesh/json"

json struct CreateUser { name: string  age: int }

fn handler(req: http.Request) http.Response {
	v := json.parse(req.body)
	if v is error {
		return http.Response{status: 400, body: "bad json", headers: map<string, string>{}}
	}
	u := decodeCreateUser(v)
	if u is error {
		return http.Response{status: 400, body: "\${u}", headers: map<string, string>{}}
	}
	return http.Response{status: 201, body: "created:\${u.name}:\${u.age}", headers: map<string, string>{}}
}

fn main() {
	http.listen("127.0.0.1:18182", handler)
}`,
      18182,
    );

    const created = await fetch("http://127.0.0.1:18182/users", {
      method: "POST",
      body: JSON.stringify({ name: "alice", age: 30 }),
    });
    expect(created.status).toBe(201);
    expect(await created.text()).toBe("created:alice:30");

    const bad = await fetch("http://127.0.0.1:18182/users", {
      method: "POST",
      body: JSON.stringify({ name: "bob" }), // age欠落
    });
    expect(bad.status).toBe(400);
    expect(await bad.text()).toContain("missing field 'age'");
  });

  test("障害分離: 1リクエストのpanicは500になり、サーバーは他のリクエストに応答し続ける", async () => {
    const server = await startServer(
      `import "mesh/http"

fn handler(req: http.Request) http.Response {
	if req.path == "/boom" {
		xs := [1, 2]
		return http.Response{status: 200, body: "\${xs[10]}", headers: map<string, string>{}}
	}
	return http.Response{status: 200, body: "alive", headers: map<string, string>{}}
}

fn main() {
	http.listen("127.0.0.1:18183", handler)
}`,
      18183,
    );

    const boom = await fetch("http://127.0.0.1:18183/boom");
    expect(boom.status).toBe(500);
    expect(await boom.text()).toBe("internal server error"); // 内部のpanic詳細はクライアントに漏らさない

    const stillAlive = await fetch("http://127.0.0.1:18183/anything");
    expect(stillAlive.status).toBe(200);
    expect(await stillAlive.text()).toBe("alive");

    expect(server.stderr()).toContain("index 10 out of range");
  });

  test("code review(PR #39): 巨大なリクエストボディは413で隔離され、サーバーは他のリクエストに応答し続ける", async () => {
    await startServer(
      `import "mesh/http"

fn handler(req: http.Request) http.Response {
	return http.Response{status: 200, body: "len:\${len(req.body)}", headers: map<string, string>{}}
}

fn main() {
	http.listen("127.0.0.1:18185", handler)
}`,
      18185,
    );

    const tooLarge = await fetch("http://127.0.0.1:18185/upload", {
      method: "POST",
      body: new Uint8Array(11 * 1024 * 1024), // 11MiB > 10MiBの上限
    });
    expect(tooLarge.status).toBe(413);
    expect(await tooLarge.text()).toBe("request body too large");

    // 上限超過の直後でも、通常サイズのリクエストには普通に応答できる(プロセスは生きている)
    const ok = await fetch("http://127.0.0.1:18185/upload", { method: "POST", body: "hi" });
    expect(ok.status).toBe(200);
    expect(await ok.text()).toBe("len:2");
  });

  test("同じポートに2度listenすると2度目はerrorになる(プロセスは落ちない)", async () => {
    await startServer(
      `import "mesh/http"

fn handler(req: http.Request) http.Response {
	return http.Response{status: 200, body: "first", headers: map<string, string>{}}
}

fn main() {
	r1 := http.listen("127.0.0.1:18184", handler)
	if r1 is error {
		print("unexpected: \${r1}")
		return
	}
	r2 := http.listen("127.0.0.1:18184", handler)
	if r2 is error {
		print("second listen failed as expected")
		return
	}
	print("should not happen")
}`,
      18184,
    );

    // 1本目のサーバーは生きたまま応答し続ける(2本目の失敗でプロセス全体が落ちていない証拠)
    const res = await fetch("http://127.0.0.1:18184/");
    expect(res.status).toBe(200);
    expect(await res.text()).toBe("first");
  });
});
