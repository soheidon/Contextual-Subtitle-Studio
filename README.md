# Contextual Subtitle Studio

ドラマ特化型字幕翻訳ツール。英語SRT字幕を日本語字幕へ翻訳します。

## Features

### 人物辞書
- **多言語キャスト統合**: Douban（中国語）＋ TMDb（英語）＋ MDL のキャスト表を照合し、日中英＋ローマ字の4言語対応表を自動生成
- **LLM漢字変換**: 簡体字の役名をLLMで日本語漢字に変換（バッチ処理・pending管理）
- **キャラクター別名生成**: 翻訳辞書向けの名前バリエーション自動生成（ローマ字→カタカナ変換、肩書分解、反復パターン、所有格・接尾語分解、タイトル句分解）
- **辞書エイリアス自動補完**: 用語集エントリにカタカナ・ローマ字エイリアスを自動付与

### 翻訳パイプライン
- **JSON翻訳モード**: シーン単位で構造化JSON翻訳、タイムスタンプ保持、自動リトライ
- **品質管理**: 重大度レベル（high/medium/low）・自動リトライ・空字幕フィルタ・字幕クレジット分類（削除/保持）
- **バリデーション診断**: 中国語混入行の断片検出・シーン位置・タイムスタンプ詳細ログ出力・JSONレポート保存
- **バリデーション重複抑制**: エントリ単位の警告で説明済みのカウント不一致を抑制

### SRT解析パイプライン
- **あらすじ生成 (2.1)**: LLMによるドラマあらすじ要約と翻訳コンテキスト自動生成、中国語検出リトライ、既知名フィルタ
- **シーン検出 (2.2)**: 登場人物・関係性・辞書強制適用を含むシーン境界解析
- **シーン文脈解析 (2.3)**: シーン検出に統合、中国語散文検出・再試行
- **中国語字幕サポート**: 中国語SRTを読み込み、固有名詞の日中表記ゆれをLLMで曖昧性解消、敬称マッピング、部族パターン対応
- **表層形マッピング**: 中国語字幕からの日本語表層形自動生成（vitestテスト完備）
- **解析結果の保存・復元**: `.srt_analysis/` ディレクトリに解析状態を永続化

### 未解決固有名詞解決
- **ヒューリスティック抽出**: SRT本文全体から固有名詞候補を抽出（ノイズ除去パイプライン: 12段階フィルタ、所有格・短縮形・称号句フィルタ）
- **AI未解決語確認**: OpenAI Responses API / Chat Completions / Gemini による個別・バッチ未解決語確認（5件チャンク分割、429自動リトライ）
- **ChatGPT貼り付け解析**: ChatGPT返答を直接貼り付けて未解決固有名詞を一括解決（検索エイリアス・文脈付き）
- **検索エイリアス生成**: 汎用接尾語（City, Palace, River等）の分解、タイトル句（X of Y）の分解、所有格除去
- **エビデンスURL正規化**: 壊れたJSON断片を含むエビデンスURLを自動修復（3層防御）

### 字幕翻訳 (v0.4.0)
- **複数ファイル一括翻訳**: 翻訳準備状態スキャン、エピソード別進捗表示、一括翻訳・未翻訳のみ翻訳
- **翻訳準備状態チェック**: 解析JSON・翻訳プロンプト・既存日本語SRTの有無を判定
- **辞書強制適用**: 翻訳前のシーン検出時に辞書エイリアス・カタカナ表記を強制置換

### 翻訳エンジン
- **マルチプロバイダ対応**: DeepSeek, MiniMax, OpenAI互換API, Gemini, ローカルLLM
- **工程別モデル選択**: あらすじ生成・シーン検出・文脈解析・固有名詞確認・字幕翻訳・漢字変換の各工程でモデルを個別指定
- **プロバイダ別詳細設定**: モデル・base_url・APIキー・thinking mode をプロバイダごとに個別設定
- **サービス設定永続化**: TMDb APIキー・SRTファイルパターンなどの設定を保持

### その他
- **作品検索パネル**: Douban / TMDb / MDL の作品URLを横断検索
- **SRTフォルダ管理**: マルチファイル・パターンマッチ・エピソード抽出
- **Rustログシステム**: 全バックエンド処理のログをフロントエンドパネルにリアルタイム表示（レベルフィルタ・保存・エクスポート）

## Architecture

```
[Frontend] Tauri v2 + React + TypeScript + Vite + Zustand
[Backend]  Rust (scraper, merge, LLM client, translation pipeline, web search)
[Storage]  JSON-based dictionary files, .srt_analysis/ directory
[Testing]  Rust: cargo test (394 tests) | TypeScript: vitest
```

## Getting Started

### Prerequisites

- Node.js 18+
- Rust 1.70+
- Tauri v2 CLI

### Install

```bash
npm install
```

### Development

```bash
npm run tauri dev
```

### Build

```bash
npm run tauri build
```

### Test

```bash
npm test                  # vitest (TypeScript)
cd src-tauri && cargo test  # Rust
```

### TMDb API Key

TMDbキャスト自動取得には TMDb API key が必要です（無料）。

1. [themoviedb.org](https://www.themoviedb.org) でアカウント作成
2. API v3 Key を取得
3. アプリの設定画面で `TMDB_API_KEY` を保存

## License

MIT — see [LICENSE](./LICENSE) for details.

## Attribution

This product uses the TMDB API but is not endorsed or certified by TMDB.
