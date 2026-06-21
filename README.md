# Contextual Subtitle Studio

ドラマ特化型字幕翻訳ツール。英語SRT字幕を日本語字幕へ翻訳します。

## Features

- **人物辞書ベースの翻訳**: 登場人物名・役名の固有名詞表記を統一
- **Douban + TMDb キャスト統合**: 中国語キャスト表（Douban）と英語キャスト表（TMDb）を照合し、4言語対応表を自動生成
- **LLM翻訳エンジン**: DeepSeek, MiniMax, OpenAI互換API, ローカルLLMをサポート
- **敬語制御**: 登場人物間の上下関係に基づく敬語ルール
- **用語集管理**: 作品固有の称号・呼称・固有名詞の辞書管理
- **品質検証**: 固有名詞ぶれ検出、敬語違反検出

## Architecture

```
[Frontend] Tauri v2 + React + TypeScript + Vite
[Backend]  Rust (scraper, merge, translation pipeline)
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

**v0.1.0** — 初期開発フェーズ。コア機能実装中。

- [x] SRT 読み込み/保存
- [x] 人物辞書管理
- [x] Douban スクレイピング
- [x] TMDb キャスト検索・取得
- [x] キャストマージ（pinyin + name_variants 照合）
- [x] LLM翻訳パイプライン
- [x] 用語集管理
- [ ] 敬語制御
- [ ] 品質検証
- [ ] バッチ翻訳

## License

MIT

## Attribution

This product uses the TMDB API but is not endorsed or certified by TMDB.
