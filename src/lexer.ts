import { CompileError, KEYWORDS, type Pos, type StringPart, type Token, type TokenType } from "./token";

// Go と同じ「セミコロン自動挿入」ルール:
// 行末のトークンがこの集合に含まれるとき、改行を ";" として扱う。
// これにより Mesh のコードは行末セミコロン不要になる。
const ASI_AFTER = new Set<TokenType>([
  "ident",
  "int",
  "float",
  "string",
  "none",
  "true",
  "false",
  "return",
  "break",
  "continue",
  ")",
  "]",
  "}",
  "++",
  "--",
  "!", // 後置の伝播演算子 x! が行末に来るため
]);

// 長い記号から先に照合する(":=" を ":" "=" に分解しないため)
const OPERATORS: TokenType[] = [
  ":=", "==", "!=", "<=", ">=", "&&", "||", "<-", "++", "--", "=>",
  "=", "<", ">", "+", "-", "*", "/", "%", "!", "|",
  ",", ":", ";", "(", ")", "{", "}", "[", "]", ".",
];

const ESCAPES: Record<string, string> = {
  n: "\n",
  t: "\t",
  r: "\r",
  '"': '"',
  "\\": "\\",
  $: "$", // リテラルの $ が欲しいとき: "\$"(補間させないための唯一のエスケープ)
};

// startPos: 文字列補間の式断片を再字句解析するとき、元ソース上の位置から数え始めるために使う
export function lex(source: string, startPos?: Pos): Token[] {
  const tokens: Token[] = [];
  let i = 0;
  let line = startPos?.line ?? 1;
  let col = startPos?.col ?? 1;

  const pos = (): Pos => ({ line, col });
  const last = (): Token | undefined => tokens[tokens.length - 1];

  const advance = (n = 1) => {
    for (let k = 0; k < n; k++) {
      if (source[i] === "\n") {
        line++;
        col = 1;
      } else {
        col++;
      }
      i++;
    }
  };

  while (i < source.length) {
    const ch = source[i];

    // 改行: セミコロン自動挿入の判定
    if (ch === "\n") {
      const prev = last();
      if (prev && ASI_AFTER.has(prev.type)) {
        tokens.push({ type: ";", value: ";", pos: pos() });
      }
      advance();
      continue;
    }

    // 空白
    if (ch === " " || ch === "\t" || ch === "\r") {
      advance();
      continue;
    }

    // 行コメント(改行は消費しない = セミコロン挿入は生きる)
    if (ch === "/" && source[i + 1] === "/") {
      while (i < source.length && source[i] !== "\n") advance();
      continue;
    }

    // 文字列リテラル(補間 ${式} 対応)
    if (ch === '"') {
      const start = pos();
      advance();
      const parts: StringPart[] = [];
      let text = "";
      while (i < source.length && source[i] !== '"') {
        if (source[i] === "\n") throw new CompileError("string literal not terminated", start);
        if (source[i] === "\\") {
          const esc = ESCAPES[source[i + 1]];
          if (esc === undefined) throw new CompileError(`unknown escape \\${source[i + 1]}`, pos());
          text += esc;
          advance(2);
          continue;
        }
        // ${式} — 対応する } までを式のソース断片として切り出す
        if (source[i] === "$" && source[i + 1] === "{") {
          advance(2);
          const exprPos = pos();
          let exprSrc = "";
          let depth = 1;
          while (i < source.length && source[i] !== "\n") {
            const c = source[i];
            // 補間式の中の入れ子文字列("x${m["k"]}" など)。中の { } は深さに数えない
            if (c === '"') {
              exprSrc += c;
              advance();
              while (i < source.length && source[i] !== '"' && source[i] !== "\n") {
                if (source[i] === "\\") {
                  exprSrc += source[i] + (source[i + 1] ?? "");
                  advance(2);
                } else {
                  exprSrc += source[i];
                  advance();
                }
              }
              if (i >= source.length || source[i] === "\n") {
                throw new CompileError("string literal not terminated", exprPos);
              }
              exprSrc += '"';
              advance();
              continue;
            }
            if (c === "{") depth++;
            if (c === "}") {
              depth--;
              if (depth === 0) break;
            }
            exprSrc += c;
            advance();
          }
          if (depth > 0) {
            throw new CompileError("interpolation not terminated — missing '}'", exprPos);
          }
          advance(); // 閉じの }
          if (exprSrc.trim() === "") {
            throw new CompileError("empty interpolation '${}'", exprPos);
          }
          if (text !== "") {
            parts.push({ kind: "text", text });
            text = "";
          }
          parts.push({ kind: "expr", source: exprSrc, pos: exprPos });
          continue;
        }
        text += source[i];
        advance();
      }
      if (i >= source.length) throw new CompileError("string literal not terminated", start);
      advance(); // 閉じの "
      if (parts.length > 0) {
        if (text !== "") parts.push({ kind: "text", text });
        tokens.push({ type: "string", value: "", pos: start, parts });
      } else {
        tokens.push({ type: "string", value: text, pos: start });
      }
      continue;
    }

    // 数値リテラル(int / float)
    if (ch >= "0" && ch <= "9") {
      const start = pos();
      let value = "";
      while (i < source.length && source[i] >= "0" && source[i] <= "9") {
        value += source[i];
        advance();
      }
      let isFloat = false;
      if (source[i] === "." && source[i + 1] >= "0" && source[i + 1] <= "9") {
        isFloat = true;
        value += ".";
        advance();
        while (i < source.length && source[i] >= "0" && source[i] <= "9") {
          value += source[i];
          advance();
        }
      }
      tokens.push({ type: isFloat ? "float" : "int", value, pos: start });
      continue;
    }

    // 識別子・キーワード
    if (/[A-Za-z_]/.test(ch)) {
      const start = pos();
      let value = "";
      while (i < source.length && /[A-Za-z0-9_]/.test(source[i])) {
        value += source[i];
        advance();
      }
      const type = KEYWORDS.has(value as TokenType) ? (value as TokenType) : "ident";
      tokens.push({ type, value, pos: start });
      continue;
    }

    // 記号・演算子
    const op = OPERATORS.find((o) => source.startsWith(o, i));
    if (op) {
      // "a < -1" のような場合: "<" の直後が "-" でも間に空白があれば "<-" にならない
      // (startsWith 照合は隣接している場合のみマッチするのでこれで正しい)
      tokens.push({ type: op, value: op, pos: pos() });
      advance(op.length);
      continue;
    }

    throw new CompileError(`unexpected character '${ch}'`, pos());
  }

  // 最終行の文もセミコロンで閉じる
  const prev = last();
  if (prev && ASI_AFTER.has(prev.type)) {
    tokens.push({ type: ";", value: ";", pos: pos() });
  }
  tokens.push({ type: "eof", value: "", pos: pos() });
  return tokens;
}
