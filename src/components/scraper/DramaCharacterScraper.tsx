import { useCallback, useEffect } from "react";
import { FileUp, GitMerge, Save, BookOpen } from "lucide-react";
import { Link } from "react-router-dom";
import { useSrtStore } from "../../stores/useSrtStore";
import { useScraperStore } from "../../stores/useScraperStore";
import { parseSrtFile, mergeCharacters, saveMergedCharacters, loadMergedCharacters, mergedToDictionary, loadCharacterDictionary, saveCharacterDictionary } from "../../lib/tauri";
import ScraperColumn from "./ScraperColumn";
import MergedCharacterTable from "./MergedCharacterTable";
import type { MatchStatus, MergedCharacter } from "../../types";

export default function DramaCharacterScraper() {
  const srt = useSrtStore();
  const scraper = useScraperStore();

  // Auto-load saved merged characters when SRT is loaded
  useEffect(() => {
    if (srt.filePath) {
      const dir = getProjectDir(srt.filePath);
      loadMergedCharacters(dir)
        .then((chars) => {
          if (chars && chars.length > 0) {
            scraper.setMergedCharacters(chars);
          }
        })
        .catch(() => {
          // No saved characters yet, that's fine
        });
    }
  }, [srt.filePath]);

  const getProjectDir = (srtPath: string): string => {
    // Given D:/dramas/Show S01E01/en.srt, return D:/dramas/Show S01E01
    const parts = srtPath.replace(/\\/g, "/").split("/");
    parts.pop(); // remove filename
    return parts.join("/");
  };

  const handleSrtOpen = async () => {
    try {
      const { open } = await import("@tauri-apps/plugin-dialog");
      const selected = await open({
        multiple: false,
        filters: [{ name: "SRTファイル", extensions: ["srt"] }],
      });
      if (selected) {
        const path = selected as string;
        const entries = await parseSrtFile(path);
        const name = path.split(/[/\\]/).pop() || "不明";
        srt.setEntries(entries, name, path);
      }
    } catch (e) {
      scraper.setError(String(e));
    }
  };

  const handleMerge = useCallback(async () => {
    scraper.setIsMerging(true);
    try {
      const merged = await mergeCharacters(
        scraper.mdlResult,
        scraper.cnCastResult,
        scraper.cnMetaResult,
      );
      scraper.setMergedCharacters(merged);
    } catch (e) {
      scraper.setError(String(e));
    } finally {
      scraper.setIsMerging(false);
    }
  }, [scraper.mdlResult, scraper.cnCastResult, scraper.cnMetaResult]);

  const handleSave = async () => {
    if (!srt.filePath) return;
    try {
      const dir = getProjectDir(srt.filePath);
      await saveMergedCharacters(dir, scraper.mergedCharacters);
    } catch (e) {
      scraper.setError(String(e));
    }
  };

  const handleLoadDictionary = async () => {
    if (!srt.filePath) return;
    try {
      const dir = getProjectDir(srt.filePath);
      // First save the merged characters
      await saveMergedCharacters(dir, scraper.mergedCharacters);
      // Convert to Character format and save as characters.json
      const _chars = await mergedToDictionary(scraper.mergedCharacters);
      // Save as characters.json in dictionaries/
      const dictPath = `${dir}/dictionaries/characters.json`;
      await saveCharacterDictionary(dictPath, _chars);
      // Load into the character dictionary store
      await loadCharacterDictionary(dictPath);
    } catch (e) {
      scraper.setError(String(e));
    }
  };

  const handleFilterChange = (status: MatchStatus | "All") => {
    scraper.setFilterStatus(status);
  };

  const handleUpdateCharacter = (index: number, updates: Partial<MergedCharacter>) => {
    scraper.updateMergedCharacter(index, updates);
  };

  return (
    <div>
      {/* SRT Loader Bar */}
      <div className="card">
        <div style={{ display: "flex", alignItems: "center", justifyContent: "space-between" }}>
        <div style={{ display: "flex", alignItems: "center", gap: 12 }}>
          <button className="btn btn-primary" onClick={handleSrtOpen}>
            <FileUp size={16} />
            SRTファイルを開く
          </button>
          {srt.isLoaded ? (
            <div style={{ display: "flex", alignItems: "center", gap: 12, flex: 1 }}>
              <span style={{ fontSize: 13, color: "var(--text-secondary)" }}>
                <strong>{srt.fileName}</strong>
              </span>
              <span style={{ fontSize: 12, color: "var(--text-muted)" }}>
                Project: {srt.filePath ? getProjectDir(srt.filePath) : "—"}
              </span>
            </div>
          ) : (
            <span style={{ fontSize: 13, color: "var(--text-muted)" }}>
              開始するにはSRTファイルを選択してください
            </span>
          )}
        </div>
        <Link to="/" style={{ fontSize: 12, color: "var(--text-link)", whiteSpace: "nowrap" }}>
          貼り付け版へ →
        </Link>
        </div>
      </div>

      {/* Source columns */}
      <div style={{ display: "flex", gap: 16, marginBottom: 16 }}>
        <ScraperColumn
          title="English DB"
          subtitle="MyDramaList"
          source="MyDramaList"
          result={scraper.mdlResult}
          onResult={scraper.setMdlResult}
        />
        <ScraperColumn
          title="Chinese Metadata"
          subtitle="豆瓣"
          source="Douban"
          result={scraper.cnMetaResult}
          onResult={scraper.setCnMetaResult}
          allowManualPaste
        />
      </div>

      {/* Merge button */}
      <div style={{ marginBottom: 16 }}>
        <button
          className="btn btn-primary"
          onClick={handleMerge}
          disabled={
            scraper.isMerging ||
            (!scraper.mdlResult && !scraper.cnCastResult && !scraper.cnMetaResult)
          }
          style={{ fontSize: 14, padding: "8px 20px" }}
        >
          <GitMerge size={18} />
          {scraper.isMerging ? "マージ中..." : "マージ"}
        </button>
      </div>

      {/* Error display */}
      {scraper.error && (
        <div
          className="card"
          style={{
            borderColor: "var(--error)",
            color: "var(--error)",
            marginBottom: 16,
          }}
        >
          {scraper.error}
        </div>
      )}

      {/* Merged character table */}
      {scraper.mergedCharacters.length > 0 && (
        <>
          <MergedCharacterTable
            characters={scraper.mergedCharacters}
            filterStatus={scraper.filterStatus}
            onFilterChange={handleFilterChange}
            onUpdateCharacter={handleUpdateCharacter}
          />

          {/* Action buttons */}
          <div style={{ display: "flex", gap: 8, marginTop: 12 }}>
            <button className="btn btn-primary" onClick={handleSave}>
              <Save size={16} />
              保存
            </button>
            <button
              className="btn btn-secondary"
              onClick={handleLoadDictionary}
              disabled={!srt.filePath}
            >
              <BookOpen size={16} />
              辞書に読み込む
            </button>
          </div>
        </>
      )}
    </div>
  );
}
