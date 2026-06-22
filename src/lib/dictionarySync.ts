import type { CharacterDict, Character, GlossaryEntry, ProperNoun } from "../types";
import { saveCharacterDictionary, saveGlossaryDictionary } from "./tauri";
import { useDictionaryStore } from "../stores/useDictionaryStore";
import { useAppLogStore } from "../stores/useAppLogStore";

function normalizeSource(s: string): string {
  return s.toLowerCase().replace(/[^a-z0-9]/g, "");
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
  const chars = dictToCharacters(dict);
  const path = buildDictFilePath(baseDir, "characters.json");
  const log = useAppLogStore.getState().addLog;
  try {
    await saveCharacterDictionary(path, chars);
    useDictionaryStore.getState().setCharacters(chars, path);
    log("success", "辞書", `characters.json 保存: ${chars.length}件 → ${path}`);
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
): Promise<void> {
  const current = useDictionaryStore.getState().glossary;
  // Build lookup: normalized source → index in current array
  const sourceIndex = new Map<string, number>();
  current.forEach((g, i) => sourceIndex.set(normalizeSource(g.source), i));

  const merged = [...current];
  let added = 0;
  let aliasMerged = 0;

  for (const e of newEntries) {
    const key = normalizeSource(e.source);
    const existingIdx = sourceIndex.get(key);
    if (existingIdx != null) {
      // Same source exists — if target also matches, merge aliases
      if (merged[existingIdx].target === e.target) {
        const existing = merged[existingIdx];
        const existingAliases = existing.aliases ?? [];
        const newAliases = e.aliases ?? [];
        for (const a of newAliases) {
          if (!existingAliases.includes(a)) {
            existingAliases.push(a);
          }
        }
        if (newAliases.length > 0) {
          merged[existingIdx] = { ...existing, aliases: existingAliases };
          aliasMerged++;
        }
        continue;
      }
      // Different target ≠ same term, fall through to add
    }
    merged.push(e);
    sourceIndex.set(key, merged.length - 1);
    added++;
  }

  if (added === 0 && aliasMerged === 0) {
    useAppLogStore.getState().addLog(
      "info",
      "辞書",
      "glossary.json: 追加項目なし (全件既存と重複)",
    );
    return;
  }

  const path = buildDictFilePath(baseDir, "glossary.json");
  const log = useAppLogStore.getState().addLog;
  try {
    await saveGlossaryDictionary(path, merged);
    useDictionaryStore.getState().setGlossary(merged, path);
    const parts: string[] = [];
    if (added > 0) parts.push(`${added}件追加`);
    if (aliasMerged > 0) parts.push(`${aliasMerged}件エイリアス統合`);
    log("success", "辞書", `glossary.json 保存: ${parts.join(", ")} → ${path}`);
  } catch (e) {
    log("error", "辞書", `glossary.json 保存失敗: ${e}`);
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
      notes: n.ja_kanji_source
        ? `source: ${n.ja_kanji_source}${
            n.ja_kanji_confidence != null
              ? `, confidence: ${n.ja_kanji_confidence}`
              : ""
          }`
        : undefined,
    }));
}
