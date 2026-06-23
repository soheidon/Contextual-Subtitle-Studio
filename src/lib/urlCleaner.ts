/**
 * Clean broken JSON fragments from evidence URLs.
 * Handles: trailing JSON debris, leading brackets, markdown links, dedup.
 */

function flattenInput(input: unknown): string[] {
  if (Array.isArray(input)) {
    return input.map((e) => String(e ?? "")).filter((s) => s.length > 0);
  }
  if (typeof input === "string" && input.length > 0) {
    return [input];
  }
  return [];
}

const URL_RE = /(https?:\/\/[^\s<>"'\]\[{}()]+)/g;

/** Characters that can trail a URL in broken JSON/markdown contexts but
 *  are NEVER valid at the end of a real URL path/query/fragment. */
function stripTrailingGarbage(raw: string): string {
  return raw.replace(/["'\\,}\]);.:;，。]+$/, "");
}

function stripLeadingGarbage(raw: string): string {
  return raw.replace(/^[\[("']+/, "");
}

/** Extract plain URL from markdown link `[text](url)` if the entire string
 *  is a markdown link. */
function extractMarkdownLink(s: string): string | null {
  const m = s.match(/^\[.*?\]\((https?:\/\/[^)]+)\)$/);
  return m ? m[1] : null;
}

export function extractCleanUrls(input: unknown): string[] {
  const candidates = flattenInput(input);
  const seen = new Set<string>();
  const result: string[] = [];

  for (const raw of candidates) {
    // Check if the whole string is a markdown link
    const mdUrl = extractMarkdownLink(raw);
    if (mdUrl) {
      const clean = stripTrailingGarbage(mdUrl);
      if (clean && !seen.has(clean)) {
        seen.add(clean);
        result.push(clean);
      }
      continue;
    }

    // Extract all https?:// runs within the string
    const matches = raw.matchAll(URL_RE);
    for (const m of matches) {
      let url = m[0];
      url = stripTrailingGarbage(url);
      url = stripLeadingGarbage(url);
      if (url && !seen.has(url)) {
        seen.add(url);
        result.push(url);
      }
    }
  }

  return result;
}

/**
 * Clean the `url` field of each evidence item in-place.
 * Preserves `title`, `quote`, and all other fields.
 * Items whose cleaned URL is empty are dropped.
 */
export function cleanEvidenceItems<T extends { url?: unknown }>(
  evidence: T[] | undefined | null,
): T[] {
  if (!Array.isArray(evidence)) return [];

  return evidence
    .map((item) => {
      const urls = extractCleanUrls(item.url);
      return { ...item, url: (urls[0] ?? "") as T["url"] };
    })
    .filter((item) => item.url);
}
