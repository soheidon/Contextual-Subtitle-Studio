# Contextual Subtitle Studio

ドラマ特化型字幕翻訳ツール。英語SRT字幕を日本語字幕へ翻訳します。

## Features

- **人物辞書ベースの翻訳**: 登場人物名・役名の固有名詞表記を統一
- **Douban + TMDb キャスト統合**: 中国語キャスト表（Douban）と英語キャスト表（TMDb）を照合し、4言語対応表を自動生成
- **LLM漢字変換**: 簡体字の役名をLLMで日本語漢字に変換（バッチ処理）
- **LLM翻訳エンジン**: DeepSeek, MiniMax, OpenAI互換API, Gemini, ローカルLLMをサポート
- **SRT解析パイプライン**: あらすじ生成・シーン検出・文脈解析による翻訳補助情報の自動生成
- **未解決固有名詞検出**: SRT本文全体から固有名詞候補をヒューリスティック抽出（ノイズ除去パイプライン搭載）、AI未解決語確認（OpenAI Responses API + Web検索）で解決
- **作品検索パネル**: Douban/TMDb/MDLの作品URLを横断検索
- **あらすじ要約**: LLMあらすじ要約と翻訳コンテキスト自動生成
- **キャラクター別名生成**: 翻訳辞書向けのキャラクター名バリエーション自動生成（ローマ字→カタカナ変換含む）
- **翻訳パイプライン品質改善**: JSON翻訳モード・致命的エラーゲート制御・自動リトライ・空字幕フィルタ・字幕クレジット分類（削除/保持）・バリデーション重複抑制
- **ChatGPT貼り付け解析**: ChatGPT返答を直接貼り付けて未解決固有名詞を一括解決
- **エビデンスURL正規化**: 壊れたJSON断片を含むエビデンスURLを自動修復

## Architecture

```
[Frontend] Tauri v2 + React + TypeScript + Vite
[Backend]  Rust (scraper, merge, LLM client, translation pipeline)
[Storage]  JSON-based dictionary files
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

### TMDb API Key

TMDbキャスト自動取得には TMDb API key が必要です（無料）。

1. [themoviedb.org](https://www.themoviedb.org) でアカウント作成
2. API v3 Key を取得
3. アプリの設定画面で `TMDB_API_KEY` を保存

## Project Status

**v0.3.2** — 翻訳パイプライン品質改善: JSON翻訳モード・空字幕フィルタ・クレジット分類・バリデーション重複抑制・SRTパーサー行ベース書き直し 完了。

- [x] SRT 読み込み/保存
- [x] 人物辞書管理（3カラム表示・JSONエクスポート）
- [x] Douban スクレイピング
- [x] TMDb キャスト検索・取得（TV series aggregate_credits対応）
- [x] MDL HTMLペースト解析
- [x] キャストマージ（pinyin + name_variants + actor_en 照合）
- [x] LLM日本語漢字変換（バッチ処理・pending_llm管理）
- [x] 作品検索パネル（Douban/TMDb/MDL横断）
- [x] LLMあらすじ要約 + 翻訳コンテキスト生成
- [x] キャラクター別名生成
- [x] LLM翻訳パイプライン
- [x] 用語集管理
- [x] プロバイダ別LLM設定（DeepSeek thinking mode + Gemini + OpenAI 対応）
- [x] サービス設定永続化（TMDb API）
- [x] SRTフォルダ管理（マルチファイル・パターンマッチ）
- [x] SRT解析パイプライン（あらすじ生成・シーン検出・文脈解析）
- [x] 未解決固有名詞検出（SRT本文全体からヒューリスティック抽出 + あらすじ由来と統合）
- [x] OpenAI Responses API によるAI未解決語確認（個別・一括、Web検索付き）
- [x] Gemini プロバイダ対応
- [x] Rustログシステム（emit_log! + ログパネル）
- [x] カタカナ→漢字変換 + CJK用語バリアント検出
- [x] SRT解析結果の保存・復元
- [x] ChatGPT貼り付け解析（一括未解決語解決）
- [x] 辞書エイリアス自動補完（ローマ字→カタカナ、反復名前パターン）
- [x] エビデンスURL正規化パイプライン
- [x] シーン検出への辞書強制適用・関係性表示
- [ ] 敬語制御
- [ ] 品質検証
- [ ] バッチ翻訳

## License

MIT

## Attribution

This product uses the TMDB API but is not endorsed or certified by TMDB.
