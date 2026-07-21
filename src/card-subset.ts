// サブセットカード(F-13後半): `mesh card --for <file.mesh>...` で、渡されたMeshソースが
// 実際に使っている機能のセクションだけに絞った縮小版カードを組み立てる。狙いはトークン節約 —
// 並行処理も判別可能unionもジェネリクスも使わない小さなプロジェクトに、全部載りのカードを
// 毎回渡す必要はない。
//
// 実装方針: LANGUAGE_CARD(src/card.ts)は書き換えない。`## 見出し` の境界で実行時に分割し、
// 見出し名だけを頼りに「常に含む」か「この機能が検出されたら含む」かを振り分ける。
// カード本文を複製・再定義しないので、本文を編集してもここが古くなる心配がない。
// 未知の見出し(将来カードに追加されて、ここが追随していない場合)は安全側で常に含める —
// 「知らない見出しだから黙って消える」事故を避けるため

import { LANGUAGE_CARD } from "./card";

export type Feature =
  | "generics"
  | "discriminatedUnions"
  | "structuredErrors"
  | "structs"
  | "arrays"
  | "concurrency"
  | "modules"
  | "defer"
  | "httpServer";

// 見出し文字列は src/card.ts の `## ...` 行と完全一致させること(ズレると常時「含める」側に落ちる —
// FEATURE_HEADINGSに無い見出しはALWAYSでなくても含める仕様なので実害は無いが、
// 意図した絞り込みが効かなくなる)
const FEATURE_HEADINGS: Record<Feature, string> = {
  generics: "Generic functions",
  discriminatedUnions: "Discriminated unions (tagged struct shapes)",
  structuredErrors: "Structured errors (discriminated unions that '?'/'or' can propagate)",
  structs: "Structs, maps & methods",
  arrays: "Arrays",
  concurrency: "Concurrency (structured — every task has an owner, leaks are impossible)",
  modules: "Modules (import / export)",
  defer: "defer (run a call when the enclosing function returns)",
  httpServer: "Standard library: mesh/http (C-6: server-only, v1)",
};

// ソース文字列に対する簡易パターン検出(字句解析まではしない — 誤検出より見逃しの方が
// 安全〈=そのセクションが余分に残るだけ〉なので、多少broadな正規表現で十分)
const FEATURE_PATTERNS: Record<Feature, RegExp> = {
  generics: /\bfn\s+\w+\s*</,
  discriminatedUnions: /\btype\s+\w+\s*=\s*\{/,
  structuredErrors: /\berror\s+(type|struct)\b/,
  structs: /\bstruct\s+\w+/,
  arrays: /\[\s*\]|\w+\[\]/,
  concurrency: /\b(spawn|detach|chan|select|wait)\b/,
  modules: /\b(import|export)\b/,
  defer: /\bdefer\b/,
  httpServer: /"mesh\/http"/,
};

export function detectFeatures(source: string): Set<Feature> {
  const found = new Set<Feature>();
  for (const feature of Object.keys(FEATURE_PATTERNS) as Feature[]) {
    if (FEATURE_PATTERNS[feature].test(source)) found.add(feature);
  }
  return found;
}

interface CardSection {
  heading: string | null; // null = 最初の`##`より前(タイトル+導入文)
  body: string; // 見出し行を含む全文(次の`##`直前まで)
}

function splitSections(card: string): CardSection[] {
  const chunks = card.split(/(?=^## )/m);
  return chunks.map((body) => {
    const m = body.match(/^## (.+)$/m);
    return { heading: m ? m[1].trim() : null, body };
  });
}

const SUBSET_DISCLAIMER =
  `This card is a PROJECT-SCOPED SUBSET — it includes only the sections relevant to features ` +
  `detected in the source given to 'mesh card --for'. It is NOT a complete list of Mesh's ` +
  `features; if you need something not covered here, run 'mesh card' (no --for) for the full ` +
  `reference before concluding a feature doesn't exist.`;

// 渡されたMeshソース(複数可)を見て、使われている機能のセクションだけに絞ったカードを返す。
// 「常に含む」セクション(見出しがFEATURE_HEADINGSに無いもの — 導入文・Program structure・
// Bindings・Types・Absence & failure・Control flow・Operators・Strings・Builtins・
// Does NOT exist・診断コード関連・Verify)は毎回そのまま入る
export function buildSubsetCard(sources: string[]): string {
  const features = detectFeatures(sources.join("\n"));
  const featureHeadingToKey = new Map(
    (Object.keys(FEATURE_HEADINGS) as Feature[]).map((f) => [FEATURE_HEADINGS[f], f] as const),
  );
  const allSections = splitSections(LANGUAGE_CARD);
  const sections = allSections.filter((s) => {
    if (s.heading === null) return true;
    const feature = featureHeadingToKey.get(s.heading);
    return feature === undefined || features.has(feature); // 未知の見出しは常に含める
  });
  // 検出した機能が全部揃っていて実質何も落ちていないなら、フルカードそのまま(「COMPLETE
  // reference」の主張も正しいまま)を返す — 注記に置き換えるのは本当に何か削ったときだけ
  if (sections.length === allSections.length) return LANGUAGE_CARD;
  return sections
    .map((s) =>
      s.heading === null
        ? s.body.replace(
            "This card is the COMPLETE reference —\nMesh has no features beyond what is listed here.",
            SUBSET_DISCLAIMER,
          )
        : s.body,
    )
    .join("");
}
