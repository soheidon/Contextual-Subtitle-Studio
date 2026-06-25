import type { BatchTermRequest, EvidenceItem, WebTermResolution } from "../types";
import { extractCleanUrls, cleanEvidenceItems } from "./urlCleaner";

// ---------------------------------------------------------------------------
// Search alias sanitization (mirrors Rust sanitize_search_aliases in srt.rs)
// ---------------------------------------------------------------------------

const GENERIC_ALIAS_WORDS = new Set([
  "moon", "sun", "star", "wind", "fire", "water", "earth", "sky",
  "ice", "snow", "iron", "gold", "silver", "jade", "sea", "storm",
  "great", "black", "white", "red", "blue", "green", "dark",
  "east", "west", "north", "south", "young", "old",
]);

/** Filter out overly generic single-word aliases (e.g. "Moon" from "Moon Guards").
 *  After filtering, if only the source_text remains, returns empty. */
function sanitizeSearchAliases(aliases: string[] | undefined): string[] {
  if (!aliases || aliases.length === 0) return [];
  const filtered = aliases.filter((a) => {
    const isSingleWord = !a.includes(" ");
    if (isSingleWord) {
      return !GENERIC_ALIAS_WORDS.has(a.toLowerCase());
    }
    return true;
  });
  // If only source_text (first entry) remains, return empty — no extra search hints
  if (filtered.length <= 1) return [];
  return filtered;
}

// ---------------------------------------------------------------------------
// Episode extraction (mirrors Rust extract_episode_from_filename in srt.rs)
// ---------------------------------------------------------------------------

interface EpisodeInfo {
  season: number | null;
  episode: number | null;
  episodeLabel: string | null; // "第5話"
}

function extractEpisodeFromFilename(filename: string): EpisodeInfo {
  // S01E05 / S1E5 (case insensitive)
  const s00e00 = /s(\d{1,2})e(\d{1,3})/i.exec(filename);
  if (s00e00) {
    const episode = parseInt(s00e00[2], 10);
    return { season: parseInt(s00e00[1], 10) || null, episode, episodeLabel: episode ? `第${episode}話` : null };
  }
  // episode N / ep N / epN (case insensitive)
  const epN = /ep(?:isode)?\s*(\d{1,3})/i.exec(filename);
  if (epN) {
    const episode = parseInt(epN[1], 10);
    return { season: null, episode, episodeLabel: episode ? `第${episode}話` : null };
  }
  // 第N話 / 第N集
  const jpEp = /第(\d{1,3})[話集]/.exec(filename);
  if (jpEp) {
    const episode = parseInt(jpEp[1], 10);
    return { season: null, episode, episodeLabel: episode ? `第${episode}話` : null };
  }
  return { season: null, episode: null, episodeLabel: null };
}

// ---------------------------------------------------------------------------
// Prompt builder
// ---------------------------------------------------------------------------

export interface BuildBatchPromptParams {
  terms: BatchTermRequest[];
  drama_title_zh: string;
  drama_title_en: string;
  drama_title_ja: string;
  folder_label: string;
  srt_filename: string;
  short_context: string;
}

/** Builds the ChatGPT paste prompt with explicit JSON schema and ja conversion rules. */
export function buildBatchPrompt(params: BuildBatchPromptParams): string {
  const { terms, drama_title_zh, drama_title_en, drama_title_ja, folder_label, srt_filename, short_context } = params;

  const zhTitle = drama_title_zh || null;
  const enTitle = drama_title_en || null;
  const jaOfficial = /\p{Script=Hiragana}|\p{Script=Katakana}|\p{Script=Han}/u.test(drama_title_ja)
    ? drama_title_ja : null;
  const folder = folder_label || null;

  // searchPrimaryTitle = zh → en → folderLabel → officialTitleJa
  const searchPrimary = zhTitle ?? enTitle ?? folder ?? jaOfficial ?? "";

  const epInfo = extractEpisodeFromFilename(srt_filename);
  const episodeNum = epInfo.episode;

  // Build episode label: "第5話 / 第5集 / episode 5"
  let episodeLabel = "";
  if (episodeNum) {
    episodeLabel = `第${episodeNum}話 / 第${episodeNum}集 / episode ${episodeNum}`;
  }

  // Build title block — each field on its own line (never space-joined)
  const titleLines: string[] = [];
  if (zhTitle) {
    titleLines.push(`作品中文名: ${zhTitle}`);
  }
  if (enTitle) {
    if (zhTitle) {
      titleLines.push(`英語題: ${enTitle}（補助情報。一般語で混同しやすいため、検索では必ず中文名と併用）`);
    } else {
      titleLines.push(`作品英語名: ${enTitle}`);
    }
  }
  if (jaOfficial) {
    titleLines.push(`日本語題: ${jaOfficial}`);
  }
  if (folder) {
    titleLines.push(`作業フォルダ名: ${folder}`);
  }
  if (!titleLines.length && folder) {
    titleLines.push(`作業フォルダ名: ${folder}`);
  }
  if (episodeLabel) {
    titleLines.push(`対象: ${episodeLabel}`);
  }

  // Numbered source list (ALL terms — no chunking for ChatGPT)
  const sourceList = terms
    .map((t, i) => {
      let line = `${i + 1}. ${t.source_text}`;
      const safe = sanitizeSearchAliases(t.aliases);
      if (safe.length > 0) {
        line += `\n    search_aliases: ${safe.join(", ")}`;
      }
      return line;
    })
    .join("\n");

  // ---- Assemble prompt ----

  let prompt = "";

  // Opening line
  prompt += "You are a Chinese drama proper noun identification assistant. Use web search to find the correct Chinese character forms corresponding to English subtitle text from a Chinese drama.\n";

  // Title block — one field per line
  if (titleLines.length) {
    prompt += "\n" + titleLines.join("\n") + "\n";
  }

  // Question + source list
  prompt += "\n";
  if (zhTitle) {
    prompt += `作品中文名「${zhTitle}」の英語字幕において、下記の英語字幕表記に対応する漢語表記を確認してください。\n`;
  } else if (enTitle) {
    prompt += `作品「${enTitle}」の英語字幕において、下記の英語字幕表記に対応する漢語表記を確認してください。\n`;
  } else if (folder) {
    prompt += `作業フォルダ「${folder}」の英語字幕において、下記の英語字幕表記に対応する漢語表記を確認してください。\n`;
  } else {
    prompt += `下記の英語字幕表記に対応する漢語表記を確認してください。\n`;
  }
  prompt += `\nsource_text:\n\n${sourceList}\n`;

  // Search policy
  prompt += "\n検索方針:\n";
  prompt += "\n";
  if (zhTitle) {
    prompt += `* 検索では必ず中文タイトル「${zhTitle}」を主キーとして使う。\n`;
    if (enTitle) {
      prompt += `* 英語題「${enTitle}」は一般語で混同しやすいため、中文タイトルと併用する場合のみ使う。\n`;
    }
  } else if (enTitle) {
    prompt += `* 検索では必ず英語題「${enTitle}」を主キーとして使う。\n`;
  }
  prompt += "* 英語題だけをキーにして検索して得た根拠は採用しない。\n";

  if (episodeNum && zhTitle) {
    prompt += "* 検索時には「冰湖重生 第5集」「冰湖重生 分集剧情」「冰湖重生 剧情介绍」「冰湖重生 episode 5」なども考慮する。\n".replace(/冰湖重生/g, zhTitle);
    prompt += "* 必要に応じて「冰湖重生 第5集 Song Cheng」のように、作品中文名・話数・source_text を組み合わせて検索する。\n".replace(/冰湖重生/g, zhTitle);
  } else if (searchPrimary) {
    prompt += `* 必要に応じて「${searchPrimary} Song Cheng」のように、作品名・source_text を組み合わせて検索する。\n`;
  }

  // Strict rules
  prompt += "\n厳守ルール:\n";
  prompt += "\n";
  prompt += "* 推論だけで漢字候補を作らない。\n";
  prompt += "* 英語字幕表記 source_text はソース由来なので、誤字・聞き間違いとは仮定しない。\n";
  prompt += "* Web検索結果、配信元ページ、あらすじ、人物関係解説、分集剧情などに直接出ている漢字表記だけを candidate_zh に採用する。\n";
  prompt += "* candidate_zh が簡体字で確認できた場合、candidate_ja には日本語字幕用の漢字表記を入れてよい。\n";
  prompt += "  例: 金辉部族 → 金暉部族\n";
  prompt += "  ただし、根拠のない別名を作らない。\n";
  prompt += "* 検索結果に明確な漢字表記が見つからない場合は、candidate_zh / candidate_ja を空にし、status を \"not_found\" にする。\n";
  prompt += "* 候補が検索結果に直接確認できず、文脈推定にすぎない場合は、status を \"uncertain\"、confidence を \"low\" にする。\n";
  prompt += "* 作品名そのものは出力しない。\n";
  prompt += "* drama_title, notes, summary, explanation などのトップレベル項目は禁止する。\n";
  prompt += "* 返答JSONのトップレベルは必ず {\"terms\": [...]} のみにする。\n";
  prompt += "* terms 配列には、入力した source_text と同じ件数・同じ順序で返す。\n";
  prompt += "* 各 source_text について、まず候補後処理 action を \"keep\" | \"remove\" | \"replace\" | \"review\" のいずれかで判定する。\n";
  prompt += "* action=\"remove\": source_text 全体が英語の短縮形・代名詞・助動詞・接続詞・文頭語だけ、または一般役職名単独の場合。is_proper_noun=false、candidate_zh/candidate_ja=null、status=\"not_found\" にする。\n";
  prompt += "  除外例: \"I'll\", \"I'm\", \"You're\", \"We're\", \"They're\", \"They'll\", \"It's\", \"That's\", \"There's\", \"Don't\", \"Can't\", \"When\", \"If\", \"Then\", \"And\", \"But\", \"So\", \"Because\", \"Although\", \"Since\", \"While\", \"Before\", \"After\", \"Shopkeeper\"。\n";
  prompt += "* action=\"replace\": source_text が文断片 + 固有名詞候補の場合。suggested_source_text に固有名詞候補だけを入れる。\n";
  prompt += "  例: \"I'm A'Chu\" → suggested_source_text=\"A'Chu\"、\"When Snow Region\" → suggested_source_text=\"Snow Region\"、\"But Yanbei City\" → suggested_source_text=\"Yanbei City\"。\n";
  prompt += "* And/But/So/When/If/Then/Because/Although/Since/While/Before/After + proper noun phrase は、先頭機能語を除いた候補へ action=\"replace\" にする。\n";
  prompt += "* action=\"keep\": Web検索で実在確認できる、または明らかに固有名詞候補として妥当な場合。\n";
  prompt += "* action=\"review\": 判断不能、Web検索で見つからないが字幕内固有名詞の可能性がある場合。\n";
  prompt += "* normalized_source_text を使う場合でも、返答JSONの source_text は必ず元の入力表記のままにする。\n";
  prompt += "* 固有名詞候補として残す場合は is_proper_noun=true にする。\n";
  prompt += "* status は \"found\" | \"uncertain\" | \"not_found\" のみを使う。\n";
  prompt += "* confidence は \"high\" | \"medium\" | \"low\" のみを使う。\n";
  prompt += "* term_type は \"person\" | \"place\" | \"organization\" | \"tribe\" | \"army\" | \"object\" | \"other\" のいずれか1つを使う。\n";
  prompt += "* evidence には根拠ページの title, url, quote を入れる。\n";
  prompt += "* JSONのみで返す。Markdownコードブロックや説明文は不要。\n";
  prompt += "* Markdownリンク [text](url) は絶対に使わない。URLはJSON文字列として直接書く。\n";
  prompt += "\n";
  prompt += "* 各候補について、ChatGPT自身が「どの程度正しいと判断したか」を評価する。\n";
  prompt += "* evidence_strength は \"direct\" | \"indirect\" | \"none\" のいずれか1つを使う。\n";
  prompt += "  - \"direct\": source_text に対応する漢字表記が、作品ページ・分集剧情・人物紹介などで直接確認できる\n";
  prompt += "  - \"indirect\": 作品関連ページに近い表記はあるが、source_text との対応が完全には直接確認できない\n";
  prompt += "  - \"none\": 根拠が見つからない\n";
  prompt += "* match_judgment は \"exact\" | \"probable\" | \"weak\" | \"not_found\" のいずれか1つを使う。\n";
  prompt += "  - \"exact\": source_text と candidate_zh の対応が直接確認できる\n";
  prompt += "  - \"probable\": 作品文脈上かなり妥当だが、完全な直接対応ではない\n";
  prompt += "  - \"weak\": 根拠が弱く、人間の確認が必要\n";
  prompt += "  - \"not_found\": 対応候補が見つからない\n";
  prompt += "* needs_human_review は、人間が確認すべき場合 true にする。\n";
  prompt += "  - status が \"uncertain\" または \"not_found\" の場合は true\n";
  prompt += "  - evidence_strength が \"indirect\" または \"none\" の場合は true\n";
  prompt += "  - confidence が \"low\" の場合は true\n";
  prompt += "* confidence_reason には、なぜその確度と判断したかを短く書く。\n";
  prompt += "* search_aliases は検索補助語です。返答JSONの source_text には必ず元の入力表記を使ってください。\n";
  prompt += "  例: Zhenhuang で検索して候補を見つけた場合でも、source_text は \"Zhenhuang City\" のままにする。\n";
  prompt += "* search_aliases が一般語すぎる場合（例: \"Moon\", \"Great\", \"Black\"）、\n";
  prompt += "  中文タイトルと同時に検索しても作品関連の根拠が確認できない限り採用しないでください。\n";

  // Context memo
  if (short_context) {
    prompt += `\n参考文脈:\n`;
    prompt += `* 参考文脈は作品世界を理解するための補助情報です。未確認語の候補を作る根拠にはしないでください。\n`;
    prompt += `* candidate_zh は必ずWeb上で直接確認できる表記だけにしてください。\n`;
    prompt += `\n${short_context}\n`;
  }

  // JSON schema — indented multi-line with a single concrete example value
  prompt += `\n返答形式:\n`;
  prompt += `{
  "terms": [
    {
      "source_text": "Song Cheng",
      "action": "keep",
      "suggested_source_text": "",
      "normalized_source_text": "Song Cheng",
      "is_proper_noun": true,
      "candidate_zh": "",
      "candidate_ja": "",
      "term_type": "person",
      "status": "found",
      "confidence": "high",
      "evidence_strength": "direct",
      "match_judgment": "exact",
      "needs_human_review": false,
      "confidence_reason": "作品関連ページで source_text に対応する漢字表記を直接確認できたため。",
      "reason": "",
      "evidence": [
        {
          "title": "",
          "url": "",
          "quote": ""
        }
      ]
    }
  ]
}\n`;
  prompt += "\n重要:\n";
  prompt += "* 上記の返答形式は1件分の例です。実際の返答では、source_text の全件を terms 配列に含めてください。\n";
  prompt += "* source_text の文字列は入力と完全一致させ、順序も入力と同じにしてください。\n";
  prompt += "* 固有名詞ではない候補も terms 配列から省略せず、action=\"remove\", is_proper_noun=false として返してください。\n";

  return prompt;
}

// ---------------------------------------------------------------------------
// Response parser
// ---------------------------------------------------------------------------

export interface ParseChatGptResult {
  results: WebTermResolution[];
  warnings: string[];
}

/**
 * Parse a ChatGPT response into WebTermResolution[].
 *
 * Strategy:
 * 1. Strip code fences, find JSON object boundaries
 * 2. Try JSON.parse (happy path — clean output)
 * 3. On failure: extract fields individually from the raw garbled text via regex.
 *    ChatGPT wraps evidence URLs in markdown links [text](url) where the text
 *    side can swallow adjacent JSON structure. Per-field regex avoids that
 *    problem entirely.
 */
export function parseChatGptResponse(text: string, inputTerms: BatchTermRequest[]): ParseChatGptResult {
  const warnings: string[] = [];

  if (!text.trim()) {
    throw new Error("貼り付けられたテキストが空です。ChatGPTの回答を貼り付けてください。");
  }

  // Build surface_ja lookup from input terms
  const surfaceJaMap = new Map<string, string>();
  for (const t of inputTerms) {
    surfaceJaMap.set(t.source_text, t.surface_ja);
  }

  let raw = text.trim();

  // Detect markdown links for diagnostic warning (don't strip — we'll extract
  // around them instead of destroying the text they span)
  const linkCount = (raw.match(/\[([^\]]*)\]\(([^)]+)\)/g) || []).length;
  if (linkCount > 0) {
    warnings.push(`${linkCount}件のMarkdownリンクを検出しました（フィールド単位の抽出で対応）`);
  }

  // Strip markdown code fences
  const fenceMatch = raw.match(/```(?:json)?\s*\n?([\s\S]*?)```/);
  if (fenceMatch) {
    raw = fenceMatch[1].trim();
  } else if (raw.startsWith("```")) {
    const bodyStart = raw.indexOf("\n");
    if (bodyStart !== -1) {
      raw = raw.substring(bodyStart + 1).trim();
      const endFence = raw.lastIndexOf("```");
      if (endFence !== -1) {
        raw = raw.substring(0, endFence).trim();
      }
    }
  }

  // Find JSON object boundaries
  const firstBrace = raw.indexOf("{");
  if (firstBrace === -1) {
    throw new Error("JSONオブジェクトが見つかりません。貼り付けたテキストにJSONが含まれているか確認してください。");
  }

  // Find matching closing brace (string-aware)
  let depth = 0;
  let lastBrace = -1;
  for (let i = firstBrace; i < raw.length; i++) {
    if (raw[i] === "{" && !isInsideString(raw, i)) {
      depth++;
    } else if (raw[i] === "}" && !isInsideString(raw, i)) {
      depth--;
      if (depth === 0) { lastBrace = i; break; }
    }
  }

  if (lastBrace === -1) {
    throw new Error("JSONの閉じ括弧が見つかりません。回答が途切れていないか確認してください。");
  }

  const jsonText = raw.substring(firstBrace, lastBrace + 1);

  // Attempt 1: strict parse (works for clean output)
  try {
    const parsed = JSON.parse(jsonText);
    const results = mapTermsFromParsed(parsed, surfaceJaMap);
    return { results, warnings };
  } catch (e: any) {
    warnings.push(`JSON.parse失敗: ${e.message ?? String(e)}`);
  }

  // Attempt 2: per-field extraction from garbled text
  warnings.push("JSON.parseに失敗したため、フィールド単位の正規表現抽出にフォールバックします");
  const results = extractTermsFromGarbledText(jsonText, inputTerms, surfaceJaMap, warnings);
  return { results, warnings };
}

/** Map from a successfully parsed JSON object (clean path). */
function mapTermsFromParsed(parsed: any, surfaceJaMap: Map<string, string>): WebTermResolution[] {
  let rawTerms: any[];
  if (Array.isArray(parsed.terms)) {
    rawTerms = parsed.terms;
  } else if (Array.isArray(parsed.results)) {
    rawTerms = parsed.results;
  } else if (Array.isArray(parsed)) {
    rawTerms = parsed;
  } else {
    throw new Error(
      `JSONに terms 配列が見つかりません。トップレベルキー: ${Object.keys(parsed).join(", ")}`
    );
  }

  return rawTerms.map((raw: any) => {
    const sourceText = raw.source_text ?? "";
    const evidenceRaw = Array.isArray(raw.evidence)
      ? raw.evidence.map((e: any) =>
          typeof e === "string" ? { title: "", url: e, quote: "" } : e,
        )
      : [];
    const evidence = cleanEvidenceItems(evidenceRaw) as EvidenceItem[];
    const status = normalizeStatus(raw.status);
    const es = normalizeEvidenceStrength(raw.evidence_strength, status);
    const mj = normalizeMatchJudgment(raw.match_judgment, status);
    const review = normalizeNeedsHumanReview(raw.needs_human_review, status, es, raw.confidence);

    return {
      source_text: sourceText,
      action: raw.action,
      suggested_source_text: raw.suggested_source_text ?? null,
      surface_ja: surfaceJaMap.get(sourceText) ?? "",
      candidate_zh: raw.candidate_zh ?? null,
      candidate_ja: raw.candidate_ja ?? null,
      confidence: normalizeConfidence(raw.confidence),
      evidence_summary: raw.reason ?? "",
      evidence_urls: extractCleanUrls(evidence.map((e: any) => e.url)),
      status,
      source: "chatgpt" as WebTermResolution["source"],
      alternatives: Array.isArray(raw.alternatives) ? raw.alternatives : [],
      evidence,
      reason: raw.reason ?? "",
      normalized_source_text: raw.normalized_source_text ?? null,
      is_proper_noun: typeof raw.is_proper_noun === "boolean" ? raw.is_proper_noun : undefined,
      term_type: raw.term_type ?? "",
      evidence_strength: es,
      match_judgment: mj,
      needs_human_review: review,
      confidence_reason: raw.confidence_reason ?? "",
    };
  });
}

/**
 * Extract term fields directly from garbled JSON text using per-field regex.
 * Handles cases where markdown links [text](url) have swallowed adjacent
 * JSON structure, making the whole document unparseable as JSON.
 *
 * v3: Input-term-driven. Instead of scanning for all source_text occurrences
 * (which picks up phantom entries from decoded markdown link text) and trying
 * to segment between them, we search for EACH known input term individually
 * and extract a generous window around its position.
 */
function extractTermsFromGarbledText(
  jsonText: string,
  inputTerms: BatchTermRequest[],
  surfaceJaMap: Map<string, string>,
  warnings: string[],
): WebTermResolution[] {
  const results: WebTermResolution[] = [];

  // ChatGPT markdown links [text](url) can swallow JSON structure, URL-encoding
  // the `"` `,` `{` `}` `(` `)` inside the link text as %22 %2C etc.
  // Decode the text so our regexes can find the original field names.
  const decoded = tryUrlDecode(jsonText);
  if (decoded !== jsonText) {
    warnings.push("URLエンコードされたJSON構造をデコードしました");
  }

  const notFound: string[] = [];

  for (const term of inputTerms) {
    const pos = findTermPosition(decoded, term.source_text);
    if (!pos) {
      notFound.push(term.source_text);
      continue;
    }

    // Count how many phantom matches were skipped for the first term (diagnostic)
    if (results.length === 0 && pos.phantomCount !== undefined && pos.phantomCount > 0) {
      warnings.push(`  "${term.source_text}" の検出時に${pos.phantomCount}件の重複候補をスキップ（最初の妥当な位置を使用）`);
    }

    // Extract a generous window (3000 chars) from the term's position.
    // This avoids segment-boundary corruption caused by phantom source_text
    // entries from decoded markdown link text.
    const windowStart = pos.index;
    const windowEnd = Math.min(pos.index + 3000, decoded.length);
    const window = decoded.substring(windowStart, windowEnd);

    const candidateZh = extractStringField(window, "candidate_zh");
    const candidateJa = extractStringField(window, "candidate_ja");
    const action = extractStringField(window, "action");
    const suggestedSourceText = extractStringField(window, "suggested_source_text");
    const normalizedSourceText = extractStringField(window, "normalized_source_text");
    const isProperNounRaw = extractBooleanField(window, "is_proper_noun");
    const termType = extractStringField(window, "term_type");
    const statusRaw = extractStringField(window, "status");
    const status = normalizeStatus(statusRaw ?? "");
    const confidenceRaw = extractStringField(window, "confidence");
    const confidence = normalizeConfidence(confidenceRaw ?? "");
    const reason = extractStringField(window, "reason");
    const evidenceUrls = extractEvidenceUrls(window);

    const evidenceStrengthRaw = extractStringField(window, "evidence_strength");
    const matchJudgmentRaw = extractStringField(window, "match_judgment");
    const needsReviewRaw = extractBooleanField(window, "needs_human_review") ?? extractStringField(window, "needs_human_review");
    const confidenceReason = extractStringField(window, "confidence_reason");

    const es = normalizeEvidenceStrength(evidenceStrengthRaw, status);
    const mj = normalizeMatchJudgment(matchJudgmentRaw, status);
    const review = normalizeNeedsHumanReview(
      needsReviewRaw === "true" || needsReviewRaw === "false" ? needsReviewRaw : null,
      status,
      es,
      confidence,
    );

    const evidenceItems = cleanEvidenceItems(
      evidenceUrls.map((url) => ({ title: "", url, quote: "" })),
    );
    const cleanUrls = extractCleanUrls(evidenceItems.map((e) => e.url ?? ""));

    results.push({
      source_text: pos.text,
      action: action as WebTermResolution["action"],
      suggested_source_text: suggestedSourceText,
      surface_ja: surfaceJaMap.get(term.source_text) ?? "",
      candidate_zh: candidateZh || null,
      candidate_ja: candidateJa || null,
      confidence,
      evidence_summary: reason ?? "",
      evidence_urls: cleanUrls,
      status,
      source: "chatgpt" as WebTermResolution["source"],
      alternatives: [],
      evidence: evidenceItems,
      reason: reason ?? "",
      normalized_source_text: normalizedSourceText,
      is_proper_noun: isProperNounRaw === "true" ? true : isProperNounRaw === "false" ? false : undefined,
      term_type: termType ?? "",
      evidence_strength: es,
      match_judgment: mj,
      needs_human_review: review,
      confidence_reason: confidenceReason ?? "",
    });
  }

  if (notFound.length > 0) {
    warnings.push(`  source_text未検出: ${notFound.join(", ")}`);
  }
  warnings.push(`${inputTerms.length}件中${results.length}件の結果を抽出しました`);

  return results;
}

// ---------------------------------------------------------------------------
// Input-term-driven position search
// ---------------------------------------------------------------------------

interface TermPosition {
  index: number;          // char index where "source_text":"..." JSON pair starts
  text: string;           // the verified source_text value
  phantomCount?: number;  // how many total matches were found (for diagnostic)
}

/**
 * Find the position of a known source_text value in garbled ChatGPT JSON.
 *
 * Tactics (tried in order):
 * 1. Exact match: `"source_text":"<escaped value>"` — prefer first occurrence
 *    that isn't preceded by garbled text inside a markdown link body.
 * 2. Split reconstitution: for multi-word terms whose JSON pair got bisected
 *    by a markdown link boundary `](url)`, detect the prefix half and verify
 *    the suffix half exists nearby.
 */
function findTermPosition(decoded: string, sourceText: string): TermPosition | null {
  const escaped = escapeRegex(sourceText);
  const stRegex = new RegExp(`"source_text"\\s*:\\s*"(${escaped})"`, "g");

  // Collect all matches, score by context quality
  type ScoredMatch = { index: number; text: string; score: number };
  const matches: ScoredMatch[] = [];
  let m: RegExpExecArray | null;
  while ((m = stRegex.exec(decoded)) !== null) {
    const ctx = decoded.substring(Math.max(0, m.index - 20), m.index);
    let score = 0;
    // Prefer match preceded by `},{` (proper array object separator) or `[` (first element)
    if (/\},\s*$/.test(ctx)) score = 3;
    else if (/\[\s*$/.test(ctx)) score = 2;
    // Penalize match preceded by markdown link bracket debris
    if (/\]\(https?:\/\//.test(ctx)) score -= 5;
    if (/%22|%2C/.test(ctx)) score -= 3;
    matches.push({ index: m.index, text: m[1], score });
  }

  if (matches.length > 0) {
    // Pick the highest-scored match
    matches.sort((a, b) => b.score - a.score);
    const best = matches[0];
    return { index: best.index, text: best.text, phantomCount: matches.length > 1 ? matches.length - 1 : 0 };
  }

  // Strategy 2: split reconstitution for multi-word terms
  const words = sourceText.split(" ");
  if (words.length < 2) return null;

  const firstWord = escapeRegex(words[0]);

  // Pattern A: "source_text":"FirstWord](https?://...
  // The markdown link boundary ](http bisects the term name right after the
  // first word (or last word before the link). Capture the prefix side.
  const splitAPattern = `"source_text"\\s*:\\s*"(${firstWord})\\]\\(https?://`;
  const splitARe = new RegExp(splitAPattern);
  const splitAMatch = splitARe.exec(decoded);
  if (!splitAMatch) return null;

  // Pattern B: nearby, verify the suffix half exists
  // "source_text":"FirstWord) RestOfWords"
  const remaining = words.slice(1).map(escapeRegex).join("\\s+");
  const searchArea = decoded.substring(splitAMatch.index, splitAMatch.index + 800);
  const patternB = new RegExp(`"source_text"\\s*:\\s*"${firstWord}\\)\\s+${remaining}"`);
  if (patternB.test(searchArea)) {
    return { index: splitAMatch.index, text: sourceText };
  }

  return null;
}

/** Escape special regex characters in a plain string for literal matching. */
function escapeRegex(s: string): string {
  return s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

/**
 * Extract a string field value from garbled JSON text segment.
 * Matches "fieldName":"value" where value may contain markdown link debris
 * but stops at the next unescaped " that precedes a known next-field delimiter.
 */
function extractStringField(segment: string, fieldName: string): string | null {
  // Look for "fieldName":"... up to the next "," or "}
  const re = new RegExp(`"${fieldName}"\\s*:\\s*"((?:[^"\\\\]|\\\\.)*)"`);
  const m = re.exec(segment);
  if (!m) return null;
  const raw = m[1];
  // Unescape JSON escapes
  return unescapeJsonString(raw).trim() || null;
}

function extractBooleanField(segment: string, fieldName: string): "true" | "false" | null {
  const re = new RegExp(`"${fieldName}"\\s*:\\s*(true|false)`);
  const m = re.exec(segment);
  if (!m) return null;
  return m[1] as "true" | "false";
}

/** Extract http/https URLs from a garbled text segment. */
function extractEvidenceUrls(segment: string): string[] {
  // Match "url":"..." — extract the URL, handling markdown link wrappers
  const urls: string[] = [];
  // Pattern 1: "url":"URL" (clean)
  const cleanRe = /"url"\s*:\s*"(https?:\/\/[^"]+)"/g;
  let m: RegExpExecArray | null;
  while ((m = cleanRe.exec(segment)) !== null) {
    urls.push(m[1]);
  }
  // Pattern 2: markdown link — the URL in ](url) is the real evidence URL
  const mdRe = /\]\((https?:\/\/[^)]+)\)/g;
  while ((m = mdRe.exec(segment)) !== null) {
    const url = m[1];
    if (!urls.includes(url)) {
      urls.push(url);
    }
  }
  return urls;
}

/** Unescape \n \t \\ \" etc. in a JSON string value. */
function unescapeJsonString(s: string): string {
  return s.replace(/\\(.)/g, (_m, ch) => {
    if (ch === "n") return "\n";
    if (ch === "t") return "\t";
    if (ch === "\\") return "\\";
    if (ch === '"') return '"';
    return ch;
  });
}

/** Safely URL-decode text. Returns original on failure. */
function tryUrlDecode(text: string): string {
  try {
    return decodeURIComponent(text);
  } catch {
    return text;
  }
}

function isInsideString(text: string, pos: number): boolean {

  let inString = false;
  let stringChar = "";
  for (let i = 0; i < pos; i++) {
    const ch = text[i];
    if (!inString) {
      if (ch === '"' || ch === "'") {
        inString = true;
        stringChar = ch;
      }
    } else {
      if (ch === "\\" && i + 1 < pos) {
        i++; // skip escaped char
      } else if (ch === stringChar) {
        inString = false;
        stringChar = "";
      }
    }
  }
  return inString;
}

function normalizeStatus(s: string): WebTermResolution["status"] {
  if (s === "found" || s === "uncertain" || s === "not_found" || s === "candidate_found" || s === "error") {
    return s;
  }
  // Map unknown statuses
  if (s === "" || !s) return "not_found";
  return "not_found";
}

function normalizeConfidence(c: string): WebTermResolution["confidence"] {
  if (c === "high" || c === "medium" || c === "low" || c === "none") {
    return c;
  }
  return "none";
}

function normalizeEvidenceStrength(
  s: string | null | undefined,
  status: WebTermResolution["status"],
): "direct" | "indirect" | "none" {
  if (s === "direct" || s === "indirect" || s === "none") return s;
  // Backward-compatible default
  return status === "found" ? "indirect" : "none";
}

function normalizeMatchJudgment(
  s: string | null | undefined,
  status: WebTermResolution["status"],
): "exact" | "probable" | "weak" | "not_found" {
  if (s === "exact" || s === "probable" || s === "weak" || s === "not_found") return s;
  // Backward-compatible default
  return status === "found" ? "probable" : "not_found";
}

function normalizeNeedsHumanReview(
  raw: string | null | undefined,
  status: WebTermResolution["status"],
  evidenceStrength: "direct" | "indirect" | "none",
  confidence: WebTermResolution["confidence"],
): boolean {
  if (typeof raw === "boolean") return raw;
  if (typeof raw === "string") {
    const trimmed = raw.trim().toLowerCase();
    if (trimmed === "true") return true;
    if (trimmed === "false") return false;
  }
  // Backward-compatible default: needs review unless we have strong evidence
  if (status === "uncertain" || status === "not_found") return true;
  if (evidenceStrength === "indirect" || evidenceStrength === "none") return true;
  if (confidence === "low") return true;
  return true; // default true for backward compat
}
