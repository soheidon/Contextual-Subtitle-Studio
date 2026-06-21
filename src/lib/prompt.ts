import type { CharacterDict, SynopsisSummary } from "../types";

export interface PromptInput {
  synopsisSummary: SynopsisSummary | null;
  dict: CharacterDict | null;
  /** Manually edited kanji for proper nouns, keyed by index */
  editablePnKanji: Record<number, string>;
}

export function buildPromptText(input: PromptInput): string {
  const { synopsisSummary, dict, editablePnKanji } = input;
  const lines: string[] = [];

  // Preamble — translation instructions
  lines.push("あなたはドラマ字幕を日本語に翻訳する翻訳者です。");
  lines.push("");
  lines.push("以下の字幕を、自然な日本語字幕に翻訳してください。");
  lines.push("");
  lines.push("翻訳では、まず用語表を最優先してください。");
  lines.push(
    "用語表にある語が字幕中に出た場合、英語表記・中国語表記・表記揺れのいずれで出ても、" +
    "必ず用語表の「日本語」欄の表記に統一してください。",
  );
  lines.push("");
  lines.push("出力ルール：");
  lines.push("");
  lines.push("* 原文にない説明を追加しない。");
  lines.push("* 字幕として短く自然な日本語にする。");
  lines.push("* 直訳しすぎず、ただし意味を変えない。");
  lines.push("* 人名・地名・勢力名・役名は用語表の日本語表記を使う。");
  lines.push("* 同じ人物・地名・勢力名の表記を途中で変えない。");
  lines.push("* 身分語・役職語は現代語に寄せすぎず、時代劇調の自然な日本語にする。");
  lines.push(
    "* 英語字幕から翻訳する場合も、中国語字幕から翻訳する場合も、" +
    "用語表の「日本語」欄を出力表記として使う。",
  );
  lines.push("* 出力は翻訳文のみとし、解説や注釈は付けない。");
  lines.push("");
  lines.push(
    "以下の対応表では、「英語」は英語字幕に出る可能性のある表記、" +
    "「中文」は中国語字幕に出る可能性のある表記、" +
    "「日本語」は日本語字幕で出力すべき表記です。" +
    "対応表にある語は、日本語欄の表記に統一してください。",
  );

  // Section 1: 翻訳用背景メモ
  if (synopsisSummary?.translation_context_short_ja) {
    lines.push("");
    lines.push("【翻訳用背景メモ】");
    lines.push("");
    lines.push(synopsisSummary.translation_context_short_ja);
  }

  // Section 2: 固有名詞対応表
  if (synopsisSummary && synopsisSummary.proper_nouns.length > 0) {
    const pnRows = synopsisSummary.proper_nouns
      .map((noun, i) => {
        const kanji = editablePnKanji[i] ?? noun.japanese_kanji ?? "";
        return {
          en: noun.english.trim(),
          cn: noun.chinese.trim(),
          jp: kanji.trim(),
          source: noun.ja_kanji_source ?? "",
        };
      })
      .filter((r) => {
        if (!r.en || !r.cn || !r.jp) return false;
        if (r.source === "pending_llm" || r.source === "rule" || !r.source) return false;
        return true;
      });
    if (pnRows.length > 0) {
      lines.push("");
      lines.push("【固有名詞対応表】");
      lines.push("英語 : 中文 : 日本語");
      lines.push("");
      for (const r of pnRows) {
        lines.push(`${r.en} : ${r.cn} : ${r.jp}`);
      }
    }
  }

  // Section 3: 登場人物名
  if (dict) {
    const charRows = Object.entries(dict)
      .filter(([_, entry]) => entry.ja_kanji_source !== "pending_llm")
      .map(([_, entry]) => ({
        en: (entry.role.english ?? "").trim(),
        cn: (entry.role.chinese ?? "").trim(),
        jp: (entry.role.japanese_kanji || "").trim(),
      }))
      .filter((r) => r.en && r.cn && r.jp);
    if (charRows.length > 0) {
      lines.push("");
      lines.push("【登場人物名】");
      lines.push("英語 : 中文 : 日本語");
      lines.push("");
      for (const r of charRows) {
        lines.push(`${r.en} : ${r.cn} : ${r.jp}`);
      }
    }
  }

  return lines.join("\n");
}
