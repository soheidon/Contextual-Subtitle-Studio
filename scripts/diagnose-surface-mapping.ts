import {
  applyZhSurfaceToTerm,
} from "../src/lib/srt/surfaceMapping";

interface DiagCase {
  source_text: string;
  zh_surface: string;
  surface_ja?: string;
  confirmed_surface?: string;
  expected_surface_ja: string;
  expected_confirmed_surface: string;
}

const cases: DiagCase[] = [
  // Episode 4 real data
  { source_text: "Lord Tu Mu", zh_surface: "图穆大人", expected_surface_ja: "図穆卿", expected_confirmed_surface: "図穆卿" },
  { source_text: "Lord Huo Sang", zh_surface: "霍桑大人", expected_surface_ja: "霍桑卿", expected_confirmed_surface: "霍桑卿" },
  { source_text: "Xiao Liu", zh_surface: "小六", expected_surface_ja: "小六", expected_confirmed_surface: "小六" },
  { source_text: "Commander Chen", zh_surface: "陈统领", expected_surface_ja: "陳統領", expected_confirmed_surface: "陳統領" },

  // Honorific regressions
  { source_text: "Young Master Huai", zh_surface: "怀少爷", expected_surface_ja: "懐若様", expected_confirmed_surface: "懐若様" },
  { source_text: "Lady Helian", zh_surface: "赫连夫人", expected_surface_ja: "赫連夫人", expected_confirmed_surface: "赫連夫人" },
  { source_text: "Lady Helian", zh_surface: "赫连小姐", expected_surface_ja: "赫連嬢", expected_confirmed_surface: "赫連嬢" },
  { source_text: "General Huan", zh_surface: "桓将军", expected_surface_ja: "桓将軍", expected_confirmed_surface: "桓将軍" },

  // Edge cases
  { source_text: "Lord Tu Mu", zh_surface: "图穆大人", surface_ja: "図穆さま", confirmed_surface: "図穆大人", expected_surface_ja: "図穆さま", expected_confirmed_surface: "図穆卿" },
  { source_text: "Lord Tu Mu", zh_surface: "图穆大人", surface_ja: "図穆さま", confirmed_surface: "図穆さま", expected_surface_ja: "図穆さま", expected_confirmed_surface: "図穆さま" },
];

const rows = cases.map((c) => {
  const result = applyZhSurfaceToTerm(
    { source_text: c.source_text, surface_ja: c.surface_ja, confirmed_surface: c.confirmed_surface },
    c.zh_surface,
  );
  const pass = result.surface_ja === c.expected_surface_ja && result.confirmed_surface === c.expected_confirmed_surface;
  return {
    source_text: c.source_text,
    zh_surface: c.zh_surface,
    surface_ja: result.surface_ja ?? "(none)",
    confirmed_surface: result.confirmed_surface ?? "(none)",
    pass,
  };
});

console.table(rows);

const failed = rows.filter((r) => !r.pass);
if (failed.length > 0) {
  console.error(`\n${failed.length} FAILED:`);
  console.table(failed);
  process.exitCode = 1;
} else {
  console.log(`\nAll ${rows.length} passed.`);
}
