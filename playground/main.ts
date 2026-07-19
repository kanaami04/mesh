// Mesh Playground の配線。
// コンパイラ (src/compiler.ts) をそのまま import してブラウザで動かす。
// 実行は Web Worker 内で行い、無限ループでも画面が固まらないようにする。

import { compile, formatDiagnostics } from "../src/compiler";

// ---- サンプル(examples/ と同内容) ----

const EXAMPLES: Record<string, { label: string; source: string }> = {
  channels: {
    label: "channels — 並行処理",
    source: `// 並行処理: spawn で起動し、受取口(task)や channel で結果を受け取る

fn double(n: int) int {
	sleep(200)
	return n * 2
}

fn worker(id: int, ch: chan<string>) {
	sleep(200 * id)
	ch <- "worker \${id} done"
}

fn main() {
	// spawn は「結果の受取口」を返す
	task := spawn double(21)
	print("計算中...")
	print(<-task)

	// channel で複数タスクの結果を集める
	ch := chan<string>()
	for i := 1; i <= 3; i++ {
		spawn worker(i, ch)
	}
	for i := 0; i < 3; i++ {
		print(<-ch)
	}
}
`,
  },
  maps: {
    label: "maps — mapとfor range",
    source: `// map と for range:
// m[k] は V | none を返すので「無いキー」を無視できない

fn main() {
	ages := map<string, int>{"alice": 30, "bob": 25}
	ages["carol"] = 28

	age := ages["alice"] or 0
	print("alice is \${age}")

	missing := ages["dave"]
	if missing is none {
		print("dave is unknown")
	}

	delete(ages, "bob")
	for k, v := range ages {
		print("\${k}: \${v}")
	}

	for i := range 3 {
		print("tick \${i}")
	}
}
`,
  },
  tree: {
    label: "tree — 判別可能union(木構造)",
    source: `// 判別可能union — タグ付きstruct形式のunion。自己参照(木構造)もOK。
// 値は union 自身の名前で作り、match は部分構造パターンで絞り込む

type Tree = { kind: "leaf", value: int } | { kind: "node", left: Tree, right: Tree }

fn leaf(v: int) Tree {
	return Tree{ kind: "leaf", value: v }
}

fn node(l: Tree, r: Tree) Tree {
	return Tree{ kind: "node", left: l, right: r }
}

fn sumTree(t: Tree) int {
	return match t {
		{ kind: "leaf" } => t.value
		{ kind: "node" } => sumTree(t.left) + sumTree(t.right)
	}
}

fn main() {
	tree := node(node(leaf(1), leaf(2)), leaf(3))
	print(sumTree(tree))
}
`,
  },
  users: {
    label: "users — struct+union+match",
    source: `// struct + union + match — Mesh の型システムの全部乗せ。
// u.nmae のようなタイポも、match のアーム漏れもコンパイルエラーになる

struct User {
	name: string
	age: int
}

fn find(id: int) User | none | error {
	if id < 0 {
		return error("invalid id: \${id}")
	}
	if id == 1 {
		return User{name: "alice", age: 30}
	}
	return none
}

fn label(id: int) string {
	u := find(id)
	return match u {
		User => "hello \${u.name} (\${u.age})"
		none => "404 not found"
		error => "500: \${u}"
	}
}

fn main() {
	print(label(1))
	print(label(2))
	print(label(-1))
}
`,
  },
  status: {
    label: "status — リテラル型とmatch",
    source: `// type宣言 + 文字列リテラル型 + match:
// タイポはコンパイルエラー、アーム漏れもコンパイルエラー

type Status = "active" | "banned" | "pending"

fn label(s: Status) string {
	return match s {
		"active" => "ようこそ"
		"banned", "pending" => "アクセス不可"
	}
}

fn main() {
	print(label("active"))
	print(label("pending"))
	// 試してみて:
	// label("actev")     → cannot use "actev" as "active" | ...
	// アームを1つ消す    → match is not exhaustive — missing: ...
}
`,
  },
  errors: {
    label: "errors — union型エラー処理",
    source: `// union型のエラーハンドリング:
// 失敗し得る関数は T | error を返し、is で絞り込む

fn divide(a: int, b: int) int | error {
	if b == 0 {
		return error("division by zero")
	}
	return a / b
}

fn main() {
	result := divide(10, 3)
	if result is error {
		print("error:", result)
		return
	}
	print("10 / 3 =", result)

	fallback := divide(1, 0) or _ => 0
	print("fallback: \${fallback}")

	bad := divide(1, 0)
	if bad is error {
		print("caught: \${bad}")
	}

	// match式: union を網羅的に分解する
	r := divide(9, 3)
	print(match r {
		error => "failed: \${r}"
		int => "match says: \${r}"
	})
}
`,
  },
  fizzbuzz: {
    label: "fizzbuzz — 制御構文",
    source: `fn main() {
	for i := 1; i <= 15; i++ {
		if i % 15 == 0 {
			print("FizzBuzz")
		} else if i % 3 == 0 {
			print("Fizz")
		} else if i % 5 == 0 {
			print("Buzz")
		} else {
			print(i)
		}
	}
}
`,
  },
  hello: {
    label: "hello — はじめの一歩",
    source: `fn main() {
	print("Hello, Mesh!")
}
`,
  },
};

const NEW_FILE_TEMPLATE = `fn main() {
\t
}
`;

// ---- 実行用 Worker ----

const WORKER_SRC = `
self.onmessage = async (e) => {
  const send = (type, text) => self.postMessage({ type, text });
  console.log = (...a) => send("log", a.join(" "));
  console.error = (...a) => send("errlog", a.join(" "));
  try {
    const AsyncFunction = Object.getPrototypeOf(async function () {}).constructor;
    await new AsyncFunction(e.data)();
    send("done", "");
  } catch (err) {
    send("error", err instanceof Error ? err.message : String(err));
  }
};
`;
const workerUrl = URL.createObjectURL(new Blob([WORKER_SRC], { type: "application/javascript" }));

// ---- DOM ----

const editor = document.getElementById("editor") as HTMLTextAreaElement;
const jsEl = document.getElementById("js") as HTMLPreElement;
const outputEl = document.getElementById("output") as HTMLPreElement;
const statusEl = document.getElementById("status") as HTMLSpanElement;
const runBtn = document.getElementById("run") as HTMLButtonElement;
const newFileBtn = document.getElementById("new-file") as HTMLButtonElement;
const exampleSel = document.getElementById("examples") as HTMLSelectElement;

let currentCode: string | null = null;
let worker: Worker | null = null;
let timeoutId: ReturnType<typeof setTimeout> | null = null;

// ---- コンパイル(入力のたびに実行) ----

function update() {
  const result = compile(editor.value, "playground.mesh");
  if (result.code !== null) {
    jsEl.textContent = result.code;
    jsEl.classList.remove("error");
    currentCode = result.code;
    runBtn.disabled = false;
  } else {
    jsEl.textContent = formatDiagnostics("playground.mesh", result.diagnostics);
    jsEl.classList.add("error");
    currentCode = null;
    runBtn.disabled = true;
  }
}

let debounce: ReturnType<typeof setTimeout>;
editor.addEventListener("input", () => {
  clearTimeout(debounce);
  debounce = setTimeout(update, 150);
});

// Tab キーでインデント(フォーカスを奪わせない)
editor.addEventListener("keydown", (e) => {
  if (e.key === "Tab") {
    e.preventDefault();
    const { selectionStart: s, selectionEnd: end } = editor;
    editor.setRangeText("\t", s, end, "end");
    editor.dispatchEvent(new Event("input"));
  }
});

// ---- 実行 ----

function setStatus(text: string, cls: string) {
  statusEl.textContent = text;
  statusEl.className = cls;
}

function appendLine(text: string, isError = false) {
  const span = document.createElement("span");
  if (isError) span.className = "errline";
  span.textContent = text + "\n";
  outputEl.appendChild(span);
  outputEl.scrollTop = outputEl.scrollHeight;
}

function stopWorker() {
  if (timeoutId !== null) clearTimeout(timeoutId);
  timeoutId = null;
  worker?.terminate();
  worker = null;
}

function run() {
  if (currentCode === null) return;
  stopWorker();
  outputEl.textContent = "";
  setStatus("実行中…", "running");

  worker = new Worker(workerUrl);
  worker.onmessage = (e) => {
    const { type, text } = e.data as { type: string; text: string };
    switch (type) {
      case "log":
        appendLine(text);
        break;
      case "errlog":
        appendLine(text, true);
        break;
      case "done":
        setStatus("✓ 完了", "ok");
        stopWorker();
        break;
      case "error":
        appendLine("panic: " + text, true);
        setStatus("✕ エラー", "err");
        stopWorker();
        break;
    }
  };

  // main() の完了を検知できるよう、末尾の起動行を await に置き換える
  const code = currentCode.replace("main().catch(__panic);", "await main();");
  worker.postMessage(code);

  timeoutId = setTimeout(() => {
    appendLine("(10秒でタイムアウトしました — 無限ループかも?)", true);
    setStatus("⏱ タイムアウト", "err");
    stopWorker();
  }, 10_000);
}

runBtn.addEventListener("click", run);
window.addEventListener("keydown", (e) => {
  if ((e.metaKey || e.ctrlKey) && e.key === "Enter") {
    e.preventDefault();
    run();
  }
});

// ---- サンプル切り替え ----

for (const [key, ex] of Object.entries(EXAMPLES)) {
  const opt = document.createElement("option");
  opt.value = key;
  opt.textContent = ex.label;
  exampleSel.appendChild(opt);
}
exampleSel.addEventListener("change", () => {
  editor.value = EXAMPLES[exampleSel.value].source;
  outputEl.textContent = "";
  setStatus("", "");
  update();
});

// ---- 新規作成 ----

newFileBtn.addEventListener("click", () => {
  editor.value = NEW_FILE_TEMPLATE;
  exampleSel.selectedIndex = -1; // どのサンプルとも異なることを示す(空欄表示)
  outputEl.textContent = "";
  setStatus("", "");
  update();

  editor.focus();
  const cursor = NEW_FILE_TEMPLATE.indexOf("\t") + 1;
  editor.setSelectionRange(cursor, cursor);
});

// ---- 初期化 ----

editor.value = EXAMPLES.channels.source;
update();
