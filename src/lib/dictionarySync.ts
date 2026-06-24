import type { CharacterDict, Character, GlossaryEntry, ProperNoun, UnresolvedTerm } from "../types";
import { saveCharacterDictionary, saveGlossaryDictionary } from "./tauri";
import { useDictionaryStore } from "../stores/useDictionaryStore";
import { useAppLogStore } from "../stores/useAppLogStore";
import { extractCleanUrls } from "./urlCleaner";

function normalizeSource(s: string): string {
  return s.toLowerCase().replace(/[^a-z0-9]/g, "");
}

/** Deduplicate string arrays by case-insensitive comparison.
 *  Structural variants (space vs no-space vs underscore) are kept as
 *  separate aliases because each serves as a distinct replacement key.
 *  Only case-only duplicates (e.g. "Qiao Qiao" vs "qiao qiao") are removed. */
function deduplicateAliases(existing: string[], incoming: string[]): string[] {
  const seen = new Set(existing.map((s) => s.toLowerCase().trim()));
  const result = [...existing];
  for (const a of incoming) {
    if (!a || !a.trim()) continue;
    const norm = a.toLowerCase().trim();
    if (seen.has(norm)) continue;
    seen.add(norm);
    result.push(a);
  }
  return result;
}

// ---------------------------------------------------------------------------
// Romanized-to-katakana (mirrors Rust romanized_to_katakana_candidates)
// ---------------------------------------------------------------------------

const ROMAJI_MAP: [string, string][] = [
  ["qian","チェン"],["cheng","チェン"],["zhang","ジャン"],
  ["chang","チャン"],["chun","チュン"],["xian","シエン"],
  ["jian","ジエン"],["yuan","ユエン"],["hui","フイ"],
  ["song","ソン"],["tong","トン"],["dong","ドン"],
  ["zhen","ジェン"],["zheng","ジェン"],["shan","シャン"],
  ["shen","シェン"],["jing","ジン"],["yong","ヨン"],
  ["tuo","トウ"],["jin","ジン"],["er","アル"],
  ["lun","ルン"],["run","ルン"],["jun","ジュン"],
  ["kya","キャ"],["kyu","キュ"],["kyo","キョ"],
  ["sha","シャ"],["shu","シュ"],["sho","ショ"],
  ["cha","チャ"],["chu","チュ"],["cho","チョ"],
  ["nya","ニャ"],["nyu","ニュ"],["nyo","ニョ"],
  ["hya","ヒャ"],["hyu","ヒュ"],["hyo","ヒョ"],
  ["mya","ミャ"],["myu","ミュ"],["myo","ミョ"],
  ["rya","リャ"],["ryu","リュ"],["ryo","リョ"],
  ["gya","ギャ"],["gyu","ギュ"],["gyo","ギョ"],
  ["ja","ジャ"],["ju","ジュ"],["jo","ジョ"],
  ["bya","ビャ"],["byu","ビュ"],["byo","ビョ"],
  ["pya","ピャ"],["pyu","ピュ"],["pyo","ピョ"],
  ["ga","ガ"],["gi","ギ"],["gu","グ"],["ge","ゲ"],["go","ゴ"],
  ["za","ザ"],["ji","ジ"],["zu","ズ"],["ze","ゼ"],["zo","ゾ"],
  ["da","ダ"],["de","デ"],["do","ド"],
  ["ba","バ"],["bi","ビ"],["bu","ブ"],["be","ベ"],["bo","ボ"],
  ["pa","パ"],["pi","ピ"],["pu","プ"],["pe","ペ"],["po","ポ"],
  ["ka","カ"],["ki","キ"],["ku","ク"],["ke","ケ"],["ko","コ"],
  ["sa","サ"],["shi","シ"],["su","ス"],["se","セ"],["so","ソ"],
  ["ta","タ"],["chi","チ"],["tu","トゥ"],["tsu","ツ"],["te","テ"],["to","ト"],
  ["na","ナ"],["ni","ニ"],["nu","ヌ"],["ne","ネ"],["no","ノ"],
  ["ha","ハ"],["hi","ヒ"],["fu","フ"],["he","ヘ"],["ho","ホ"],
  ["ma","マ"],["mi","ミ"],["mu","ム"],["me","メ"],["mo","モ"],
  ["ya","ヤ"],["yu","ユ"],["yo","ヨ"],
  ["ra","ラ"],["ri","リ"],["ru","ル"],["re","レ"],["ro","ロ"],
  ["wa","ワ"],["wo","ヲ"],
  ["a","ア"],["i","イ"],["u","ウ"],["e","エ"],["o","オ"],
  ["n","ン"],
];

function romanizeWord(word: string): string {
  const lower = word.toLowerCase();
  let result = "";
  let pos = 0;
  while (pos < lower.length) {
    let matched = false;
    for (let len = Math.min(6, lower.length - pos); len >= 1; len--) {
      const slice = lower.slice(pos, pos + len);
      const entry = ROMAJI_MAP.find(([pat]) => pat === slice);
      if (entry) {
        result += entry[1];
        pos += len;
        matched = true;
        break;
      }
    }
    if (!matched) pos++;
  }
  return result;
}

function romanizedToKatakanaCandidates(source: string): string[] {
  const candidates: string[] = [];
  const stripped = source.toLowerCase().replace(/[\s\-']/g, "");
  const concat = romanizeWord(stripped);
  if (concat) candidates.push(concat);

  const words = source.split(/\s+/).filter(Boolean);
  if (words.length >= 2) {
    const dotForm = words.map((w) => romanizeWord(w)).filter(Boolean);
    if (dotForm.length === words.length) {
      candidates.push(dotForm.join("・"));
    }
  }

  // Pass C: alt mapping without "tuo"→"トウ", so "tu"→"トゥ" + "o"→"オ" produces トゥオ variants
  if (source.toLowerCase().includes("tuo")) {
    const altMap: [string, string][] = ROMAJI_MAP.filter(([pat]) => pat !== "tuo");
    function romanizeWithMap(word: string, map: [string, string][]): string {
      const lower = word.toLowerCase();
      let result = "";
      let pos = 0;
      while (pos < lower.length) {
        let matched = false;
        for (let len = Math.min(6, lower.length - pos); len >= 1; len--) {
          const slice = lower.slice(pos, pos + len);
          const entry = map.find(([pat]) => pat === slice);
          if (entry) {
            result += entry[1];
            pos += len;
            matched = true;
            break;
          }
        }
        if (!matched) pos++;
      }
      return result;
    }
    const concatAlt = romanizeWithMap(stripped, altMap);
    if (concatAlt && concatAlt !== candidates[0]) {
      candidates.push(concatAlt);
    }
    if (words.length >= 2) {
      const dotAlt = words.map((w) => romanizeWithMap(w, altMap)).filter(Boolean);
      if (dotAlt.length === words.length) {
        const dotAltStr = dotAlt.join("・");
        if (candidates.length < 2 || dotAltStr !== candidates[1]) {
          candidates.push(dotAltStr);
        }
      }
    }
  }

  return [...new Set(candidates)];
}

/** Generate all search aliases from an English source: source itself, space-stripped,
 *  underscore variant, and katakana forms. Merges into existing aliases if provided. */
function generateSourceAliases(source: string, existingAliases?: string[]): string[] {
  const aliases = existingAliases ? [...existingAliases] : [];
  const seen = new Set(aliases.map((s) => s.toLowerCase()));
  const add = (s: string) => {
    if (!s) return;
    const lower = s.toLowerCase();
    if (seen.has(lower)) return;
    seen.add(lower);
    aliases.push(s);
  };

  // English variants
  add(source);
  const noSpace = source.replace(/\s+/g, "");
  if (noSpace !== source) add(noSpace);
  const underscored = source.replace(/\s+/g, "_");
  if (underscored !== source && underscored !== noSpace) add(underscored);

  // Katakana forms
  for (const kana of romanizedToKatakanaCandidates(source)) {
    add(kana);
  }
  return aliases;
}

/** Enrich a GlossaryEntry's aliases by re-generating from its source field
 *  (English variants, katakana forms). Merges with existing aliases.
 *  All other fields are preserved unchanged. */
function enrichGlossaryEntryAliases(g: GlossaryEntry): GlossaryEntry {
  const enriched = generateSourceAliases(g.source, g.aliases ?? []);
  return { ...g, aliases: enriched };
}

/** Enrich aliases for all characters. Returns enriched array and count of changed entries. */
function enrichAllCharacters(
  chars: Character[],
): { enriched: Character[]; enrichedCount: number } {
  let enrichedCount = 0;
  const enriched = chars.map((c) => {
    const before = c.aliases?.length ?? 0;
    const ec = enrichCharacterAliases(c);
    if (ec.aliases.length > before) enrichedCount++;
    return ec;
  });
  return { enriched, enrichedCount };
}

/** Enrich aliases for all glossary entries. Returns enriched array and count of changed entries. */
function enrichAllGlossaryEntries(
  entries: GlossaryEntry[],
): { enriched: GlossaryEntry[]; enrichedCount: number } {
  let enrichedCount = 0;
  const enriched = entries.map((g) => {
    const before = g.aliases?.length ?? 0;
    const eg = enrichGlossaryEntryAliases(g);
    if (eg.aliases && eg.aliases.length > before) enrichedCount++;
    return eg;
  });
  return { enriched, enrichedCount };
}

function buildDictFilePath(baseDir: string, fileName: string): string {
  const sep = baseDir.includes("\\") ? "\\" : "/";
  return `${baseDir}${sep}dictionaries${sep}${fileName}`;
}

/** Generate a stable character id from role data. */
function roleId(role: { english: string | null; chinese: string | null; japanese_kanji: string }): string {
  const base = role.english || role.chinese || role.japanese_kanji;
  return normalizeSource(base);
}

/** Build aliases from role name data: English variants, Chinese, Japanese kanji. */
function buildRoleAliases(role: {
  english: string | null;
  chinese: string | null;
  japanese_kanji: string;
}): string[] {
  const aliases: string[] = [];
  if (role.english) {
    const en = role.english.trim();
    aliases.push(en);
    // space-removed variant (e.g. "Xiao Feng" → "XiaoFeng")
    const noSpace = en.replace(/\s+/g, "");
    if (noSpace !== en) aliases.push(noSpace);
    // underscore variant (e.g. "Xiao Feng" → "Xiao_Feng")
    const underscored = en.replace(/\s+/g, "_");
    if (underscored !== en && underscored !== noSpace) aliases.push(underscored);

    // Repeated given-name variant for 2-token Chinese names
    // (e.g. "Chu Qiao" → "Qiao Qiao", "QiaoQiao", "Qiao_Qiao")
    const tokens = en.split(/\s+/);
    if (tokens.length === 2 && tokens[1]) {
      const given = tokens[1];
      const repeated = `${given} ${given}`;
      if (repeated !== en) aliases.push(repeated);
      const repeatedNoSpace = `${given}${given}`;
      if (repeatedNoSpace !== repeated && !aliases.includes(repeatedNoSpace)) aliases.push(repeatedNoSpace);
      const repeatedUnderscore = `${given}_${given}`;
      if (repeatedUnderscore !== repeatedNoSpace && !aliases.includes(repeatedUnderscore)) aliases.push(repeatedUnderscore);
    }
  }
  if (role.chinese) {
    const zh = role.chinese.trim();
    if (zh && !aliases.includes(zh)) aliases.push(zh);
  }
  if (role.japanese_kanji) {
    const jp = role.japanese_kanji.trim();
    if (jp && !aliases.includes(jp)) aliases.push(jp);
  }
  return aliases;
}

/** Enrich a Character's aliases by re-generating from english_name, chinese_name,
 *  and japanese_name. Merges with existing aliases, deduplicates.
 *  All other fields are preserved unchanged. */
function enrichCharacterAliases(c: Character): Character {
  const fresh = buildRoleAliases({
    english: c.english_name || null,
    chinese: c.chinese_name ?? null,
    japanese_kanji: c.japanese_name,
  });
  const merged = deduplicateAliases(c.aliases ?? [], fresh);
  return { ...c, aliases: merged };
}

/** Convert the actor-keyed CharacterDict into the Character[] format for characters.json.
 *  Role (character) data is PRIMARY; actor data is NOT used for identity fields. */
export function dictToCharacters(dict: CharacterDict): Character[] {
  return Object.values(dict).map((entry) => {
    const role = entry.role;
    return {
      id: roleId(role),
      english_name: role.english ?? "",
      chinese_name: role.chinese ?? undefined,
      japanese_name: role.japanese_kanji,
      aliases: buildRoleAliases(role),
      default_register: "neutral",
    };
  });
}

/** Save CharacterDict to {baseDir}/dictionaries/characters.json and update the store. */
export async function persistCharacters(
  baseDir: string,
  dict: CharacterDict,
): Promise<void> {
  const rawChars = dictToCharacters(dict);
  const { enriched: chars, enrichedCount } = enrichAllCharacters(rawChars);
  const path = buildDictFilePath(baseDir, "characters.json");
  const log = useAppLogStore.getState().addLog;
  try {
    await saveCharacterDictionary(path, chars);
    useDictionaryStore.getState().setCharacters(chars, path);
    const parts: string[] = [`${chars.length}件保存`];
    if (enrichedCount > 0) parts.push(`${enrichedCount}件エイリアス自動補完`);
    log("success", "辞書", `characters.json: ${parts.join(", ")} → ${path}`);
  } catch (e) {
    log("error", "辞書", `characters.json 保存失敗: ${e}`);
  }
}

/**
 * Append new glossary entries to {baseDir}/dictionaries/glossary.json.
 * Deduplicates against existing entries: if both normalized source AND target match,
 * merge aliases from the new entry into the existing one.
 */
export async function appendToGlossary(
  baseDir: string,
  newEntries: GlossaryEntry[],
): Promise<{ added: number; aliasMerged: number }> {
  const current = useDictionaryStore.getState().glossary;
  // Build lookup: normalized source → index in current array
  const sourceIndex = new Map<string, number>();
  current.forEach((g, i) => sourceIndex.set(normalizeSource(g.source), i));

  const merged = [...current];
  let added = 0;
  let aliasMerged = 0;

  for (const e of newEntries) {
    const key = normalizeSource(e.source);
    const aliasesEnriched = generateSourceAliases(e.source, e.aliases);
    const enrichedEntry = { ...e, aliases: aliasesEnriched };
    const existingIdx = sourceIndex.get(key);
    if (existingIdx != null) {
      if (merged[existingIdx].target === e.target) {
        const existing = merged[existingIdx];
        const existingAliases = generateSourceAliases(existing.source, existing.aliases ?? []);
        for (const a of aliasesEnriched) {
          if (!existingAliases.includes(a)) {
            existingAliases.push(a);
          }
        }
        if (existingAliases.length > (existing.aliases?.length ?? 0)) {
          merged[existingIdx] = { ...existing, aliases: existingAliases };
          aliasMerged++;
        }
        continue;
      }
    }
    merged.push(enrichedEntry);
    sourceIndex.set(key, merged.length - 1);
    added++;
  }

  // Re-enrich ALL glossary entries to heal stale/missing aliases in existing entries
  const { enriched: aliasEnriched, enrichedCount } = enrichAllGlossaryEntries(merged);

  // Clean broken evidence_urls in ALL entries (migration for existing broken data)
  let evidenceCleaned = 0;
  let evidencePruned = 0;
  const fullyEnriched = aliasEnriched.map((g) => {
    if (!g.evidence_urls || g.evidence_urls.length === 0) return g;
    const cleaned = extractCleanUrls(g.evidence_urls);
    if (cleaned.length === g.evidence_urls.length && cleaned.every((u, i) => u === g.evidence_urls![i])) {
      return g;
    }
    if (cleaned.length > 0) evidenceCleaned++;
    evidencePruned += g.evidence_urls.length - cleaned.length;
    return { ...g, evidence_urls: cleaned.length > 0 ? cleaned : undefined };
  });

  if (added === 0 && aliasMerged === 0 && enrichedCount === 0 && evidenceCleaned === 0) {
    useAppLogStore.getState().addLog(
      "info",
      "辞書",
      "glossary.json: 追加項目なし (全件既存と重複)",
    );
    return { added: 0, aliasMerged: 0 };
  }

  const path = buildDictFilePath(baseDir, "glossary.json");
  const log = useAppLogStore.getState().addLog;
  try {
    await saveGlossaryDictionary(path, fullyEnriched);
    useDictionaryStore.getState().setGlossary(fullyEnriched, path);
    const parts: string[] = [];
    if (added > 0) parts.push(`${added}件追加`);
    if (aliasMerged > 0) parts.push(`${aliasMerged}件エイリアス統合`);
    if (enrichedCount > 0) parts.push(`${enrichedCount}件エイリアス自動補完`);
    if (evidenceCleaned > 0) parts.push(`evidence_urls: ${evidenceCleaned}件修復`);
    if (evidencePruned > 0) parts.push(`${evidencePruned}件不正URLを除去`);
    log("success", "辞書", `glossary.json 保存: ${parts.join(", ")} → ${path}`);
    return { added, aliasMerged };
  } catch (e) {
    log("error", "辞書", `glossary.json 保存失敗: ${e}`);
    return { added: 0, aliasMerged: 0 };
  }
}

/** Convert ProperNoun[] from synopsis summary into GlossaryEntry[].
 *  Skips entries with empty english, empty japanese_kanji, or pending_llm source. */
export function properNounsToGlossaryEntries(nouns: ProperNoun[]): GlossaryEntry[] {
  return nouns
    .filter((n) => {
      if (!n.english || !n.english.trim()) return false;
      if (!n.japanese_kanji || !n.japanese_kanji.trim()) return false;
      if (n.ja_kanji_source === "pending_llm") return false;
      return true;
    })
    .map((n) => ({
      source: n.english,
      target: n.japanese_kanji,
      type: "proper_noun",
      aliases: generateSourceAliases(n.english),
      notes: n.ja_kanji_source
        ? `source: ${n.ja_kanji_source}${
            n.ja_kanji_confidence != null
              ? `, confidence: ${n.ja_kanji_confidence}`
              : ""
          }`
        : undefined,
    }));
}

/**
 * Batch-save adopted terms to glossary.json and (for alias candidates) to characters.json.
 * Returns counts for AppLogPanel logging.
 */
export async function batchSaveAdoptedTerms(
  baseDir: string,
  adoptedTerms: GlossaryEntry[],
  aliasCandidateTerms: UnresolvedTerm[],
  characters: Character[],
): Promise<{ glossaryAdded: number; charactersAliasAdded: number }> {
  // Phase 1: Append to glossary
  const { added: glossaryAdded } = await appendToGlossary(baseDir, adoptedTerms);

  // Phase 2: Append resolved kanji aliases + enrich all character aliases
  let charactersAliasAdded = 0;

  const updatedChars = characters.map((c) => ({ ...c, aliases: [...c.aliases] }));

  // 2a: Append resolved kanji from alias candidate terms
  if (aliasCandidateTerms.length > 0 && characters.length > 0) {
    const charMap = new Map<string, Character>();
    for (const c of characters) {
      const key = normalizeSource(c.english_name);
      charMap.set(key, c);
      for (const alias of c.aliases) {
        const aliasKey = normalizeSource(alias);
        if (!charMap.has(aliasKey)) charMap.set(aliasKey, c);
      }
    }

    for (const t of aliasCandidateTerms) {
      const termKey = normalizeSource(t.source_text);
      const matched = charMap.get(termKey);
      if (!matched) continue;

      const resolvedKanji = t.webResult?.candidate_zh ?? t.webResult?.candidate_ja ?? t.surface_ja;
      if (!resolvedKanji) continue;

      const idx = updatedChars.findIndex((c) => normalizeSource(c.english_name) === normalizeSource(matched.english_name));
      if (idx === -1) continue;

      const char = updatedChars[idx];
      if (!char.aliases.includes(resolvedKanji)) {
        char.aliases.push(resolvedKanji);
        charactersAliasAdded++;
      }
    }
  }

  // 2b: Enrich ALL characters with generated aliases (heals stale/missing aliases)
  const { enriched: enrichedChars, enrichedCount } = enrichAllCharacters(updatedChars);

  if (charactersAliasAdded > 0 || enrichedCount > 0) {
    const charPath = buildDictFilePath(baseDir, "characters.json");
    try {
      await saveCharacterDictionary(charPath, enrichedChars);
      useDictionaryStore.getState().setCharacters(enrichedChars, charPath);
      const parts: string[] = [];
      if (charactersAliasAdded > 0) parts.push(`${charactersAliasAdded}件の別名追加`);
      if (enrichedCount > 0) parts.push(`${enrichedCount}件のエイリアス自動補完`);
      useAppLogStore.getState().addLog(
        "success",
        "辞書",
        `characters.json 更新: ${parts.join(", ")} → ${charPath}`,
      );
    } catch (e) {
      useAppLogStore.getState().addLog("error", "辞書", `characters.json 更新失敗: ${e}`);
    }
  }

  return { glossaryAdded, charactersAliasAdded };
}
