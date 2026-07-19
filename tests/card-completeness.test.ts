// カード完全性の逆方向検査(F-13後半): 実装がカードからはみ出す(=カードに書いていない名前を
// 受け付けてしまう)ことを防ぐCI検査。素朴に「実装からその都度全部拾う」一般的な方法は無いので、
// ここでは「BUILTINS/RESERVEDという既存の正典リストに新しい名前が足されたら、カードの該当節に
// 書き忘れていないか」という退行防止に絞る。過去の実例(eval/argumentsをRESERVEDに追加した際、
// 別セッションでカード追記を忘れかけた)はまさにこの形の見落としだった

import { describe, expect, test } from "bun:test";
import { LANGUAGE_CARD } from "../src/card";
import { BUILTINS, RESERVED } from "../src/checker";

function cardSection(heading: string): string {
  const chunk = LANGUAGE_CARD.split(/(?=^## )/m).find((c) => c.startsWith(`## ${heading}\n`));
  if (!chunk) throw new Error(`card section not found: "${heading}" (見出し文字列がcard.tsとズレていないか確認)`);
  return chunk;
}

describe("カード完全性の逆方向検査(F-13後半)", () => {
  test("BUILTINSの全関数名がカードのBuiltinsセクションに載っている", () => {
    const section = cardSection("Builtins (complete list)");
    const missing = [...BUILTINS].filter((name) => !new RegExp(`\\b${name}\\b`).test(section));
    expect(missing).toEqual([]);
  });

  test("RESERVEDの全予約語がカードのBindingsセクションに載っている", () => {
    const section = cardSection("Bindings (immutable by default)");
    const missing = [...RESERVED].filter((name) => !new RegExp(`\\b${name}\\b`).test(section));
    expect(missing).toEqual([]);
  });
});
