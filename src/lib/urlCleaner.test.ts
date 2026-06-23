/**
 * Tests for urlCleaner.ts — run with: npx tsx src/lib/urlCleaner.test.ts
 */
import { extractCleanUrls, cleanEvidenceItems } from "./urlCleaner";

let passed = 0;
let failed = 0;

function eq<T>(actual: T, expected: T, label: string): void {
  const a = JSON.stringify(actual);
  const e = JSON.stringify(expected);
  if (a === e) {
    passed++;
    console.log(`  PASS: ${label}`);
  } else {
    failed++;
    console.error(`  FAIL: ${label}`);
    console.error(`    expected: ${e}`);
    console.error(`    actual:   ${a}`);
  }
}

// Test 1: Plain URL passes through
(() => {
  console.log("Test 1: Plain URL");
  eq(
    extractCleanUrls(["https://www.iqiyi.com/v_1sicr4myg74.html"]),
    ["https://www.iqiyi.com/v_1sicr4myg74.html"],
    "plain URL unchanged",
  );
})();

// Test 2: Leading bracket stripped
(() => {
  console.log("Test 2: Leading bracket");
  eq(
    extractCleanUrls(["[https://www.newhanfu.com/81147.html"]),
    ["https://www.newhanfu.com/81147.html"],
    "leading bracket stripped",
  );
})();

// Test 3: JSON fragment stripped
(() => {
  console.log("Test 3: JSON fragment");
  eq(
    extractCleanUrls([
      'https://www.iqiyi.com/v_1sicr4myg74.html","quote":"大雍将军诸葛玥',
    ]),
    ["https://www.iqiyi.com/v_1sicr4myg74.html"],
    "JSON fragment stripped",
  );
})();

// Test 4: Markdown link converted
(() => {
  console.log("Test 4: Markdown link");
  eq(
    extractCleanUrls([
      "[腾讯新闻](https://news.qq.com/rain/a/20260420A07MNH00)",
    ]),
    ["https://news.qq.com/rain/a/20260420A07MNH00"],
    "markdown link extracted",
  );
})();

// Test 5: Dedupe across cleaned forms
(() => {
  console.log("Test 5: Dedupe");
  eq(
    extractCleanUrls([
      "https://www.iqiyi.com/v_1sicr4myg74.html",
      "https://www.iqiyi.com/v_1sicr4myg74.html",
      "[https://www.iqiyi.com/v_1sicr4myg74.html",
    ]),
    ["https://www.iqiyi.com/v_1sicr4myg74.html"],
    "duplicates collapsed",
  );
})();

// Test 6: Migration — broken existing entry
(() => {
  console.log("Test 6: Migration");
  const cleaned = extractCleanUrls([
    'https://www.iqiyi.com/v_1sicr4myg74.html","quote":"黑鹰军',
    "[https://news.qq.com/rain/a/20260420A07MNH00",
  ]);
  eq(cleaned.length, 2, "two URLs kept");
  eq(cleaned[0], "https://www.iqiyi.com/v_1sicr4myg74.html", "first URL clean");
  eq(cleaned[1], "https://news.qq.com/rain/a/20260420A07MNH00", "second URL clean");
})();

// Test 7: cleanEvidenceItems
(() => {
  console.log("Test 7: cleanEvidenceItems");
  const input = [
    {
      title: "iqiyi",
      url: 'https://www.iqiyi.com/v_1sicr4myg74.html","quote":"大雍将军',
      quote: "大雍将军...",
    },
  ];
  const result = cleanEvidenceItems(input);
  eq(result.length, 1, "one item kept");
  eq(result[0].title, "iqiyi", "title preserved");
  eq(
    result[0].url,
    "https://www.iqiyi.com/v_1sicr4myg74.html",
    "url cleaned",
  );
  eq(result[0].quote, "大雍将军...", "quote preserved");

  // Test item with completely broken URL that yields nothing
  const emptyResult = cleanEvidenceItems([
    { title: "bad", url: '","quote":"garbage"' },
  ]);
  eq(emptyResult.length, 0, "item with no valid URL is dropped");
})();

// Test 8: cleanEvidenceItems with null/undefined
(() => {
  console.log("Test 8: cleanEvidenceItems edge cases");
  eq(cleanEvidenceItems(null), [], "null returns empty");
  eq(cleanEvidenceItems(undefined), [], "undefined returns empty");
})();

// Test 9: extractCleanUrls with string input (not array)
(() => {
  console.log("Test 9: string input");
  eq(
    extractCleanUrls("https://example.com/foo"),
    ["https://example.com/foo"],
    "bare string works",
  );
})();

// Test 10: extractCleanUrls with garbage input
(() => {
  console.log("Test 10: garbage input");
  eq(extractCleanUrls(null), [], "null returns empty");
  eq(extractCleanUrls(42), [], "number returns empty");
  eq(extractCleanUrls("no url here"), [], "non-URL string returns empty");
})();

console.log(`\n${passed} passed, ${failed} failed`);
if (failed > 0) throw new Error(`${failed} test(s) failed`);
