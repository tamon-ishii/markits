## Context

MarkIts は意味的なアノテーションモデル（`Annotation`, `TargetRect`, `PositionHint` 等）、決定論的レイアウトエンジン、SVG レンダラー、ラスター合成エンジンを備えた Rust ライブラリおよび CLI ツールです（詳細は `proposal.md` 参照）。
本設計では、既存のコアライブラリおよび CLI の軽量性と単体提供を完全に維持しながら、Tauri 2.0 を基盤としたデスクトップ常駐アプリケーションを Cargo ワークスペースとして統合・構築するためのアーキテクチャを定義します。

## Goals / Non-Goals

**Goals:**
- **Cargo ワークスペース構成**: コアの `markits`（CLI / lib）と GUI アプリケーション（`apps/desktop`）を分離し、コアの単体性・軽量性を担保。
- **システムトレイ常駐 & グローバルホットキー**: バックグラウンドで待機し、PrintScreen キーまたはカスタムキーバインドで即座にキャプチャを起動。
- **マルチモード画面キャプチャ**: 全画面、個別ウィンドウ、ドラッグ矩形選択（Snipping Tool ライクな半透明オーバーレイ）による高精度キャプチャ。
- **直感的なベクターエディタ**:
  - 直線矢印およびベジェ矢印の始点・終点・制御点ハンドルをドラッグ操作。
  - テキスト内容の編集、フォントファミリー・フォントサイズ・セマンティックスタイルの調整。
  - マークの選択、移動、削除、Undo / Redo。
- **再編集可能な PNG 保存**: PNG の `tEXt` メタデータチャンクに MarkIts のアノテーション JSON を埋め込み、保存済み画像からの完全な状態復元・再編集を実現。
- **高速クリップボード連携**: ワンクリックでアノテーション合成画像をクリップボードにコピー。

**Non-Goals:**
- コアクレートへの GUI 関連の重い依存関係（WebKit、GTK、Cairo など）の混入。
- クラウドストレージ同期やソーシャル共有機能（ローカルファーストで完結）。
- 複雑なペイントツール（自由曲線ブラシ、レイヤー合成モードなど）の実装（MarkIts のセマンティックアノテーションに特化）。

## Architecture Overview

```mermaid
flowchart TD
    subgraph Core ["markits Core (Rust Lib / CLI)"]
        Model[Annotation Model / JSON]
        Layout[Layout Engine]
        Renderer[SVG / Raster Engine]
    end

    subgraph DesktopApp ["apps/desktop (Tauri 2.0 Desktop)"]
        subgraph Backend ["Rust Backend (Tauri)"]
            Tray[Tray & Global Shortcut]
            Capture[Screen Capture Engine (xcap)]
            Meta[PNG Metadata Handler]
            IPC[Tauri IPC Bridge]
        end
        subgraph Frontend ["Web Frontend (Editor UI)"]
            Overlay[Capture Overlay]
            Editor[Interactive Canvas]
            Handles[Handle Drag & Typography Controls]
        end
    end

    Tray -->|PrintScreen| Overlay
    Overlay -->|Selected Area| Capture
    Capture -->|Raw Pixels| Editor
    Editor -->|Edit Scene JSON| IPC
    IPC --> Model
    IPC --> Layout
    IPC --> Renderer
    IPC --> Meta
    Meta -->|Save with Metadata| File[(PNG File)]
    Meta -->|Read & Restore| Editor
```

## Decisions

### 1. Cargo ワークスペースによるクレート分離
- **決定**: リポジトリを Cargo ワークスペース化し、既存の `markits`（コアライブラリ & CLI）と `apps/desktop`（Tauri アプリ）を明確に分割する。
- **理由**: コアモジュールの単体提供および軽量性を厳格に維持するため。CLI のみのユーザーは重い GUI / WebKit 依存を一切ビルドする必要がない。
- **代替案**: 単一クレートで `cargo --features gui`。しかし Tauri は Web アセットのバンドルやネイティブフックが必要なため、ワークスペース分離の方がクリーンで保守性が高い。

### 2. GUI スタック: Tauri 2.0 + Web フロントエンド (TypeScript / Canvas / SVG)
- **決定**: デスクトップ基盤に Tauri 2.0 を採用。フロントエンドには Vite + TypeScript による軽量なベクターキャンバスエディタを構築。
- **理由**:
  - システムトレイ、グローバルショートカット、マルチウィンドウ（透明オーバーレイとエディタ）、クリップボードが成熟したプラグインエコシステム（`tauri-plugin-global-shortcut`, `tauri-plugin-clipboard-manager` 等）で提供されている。
  - SVG ハンドルやアンカーポイントのドラッグ操作、タイポグラフィ（システムフォント選択、リアルタイムプレビュー）の構築において Web 技術が極めて高い表現力と生産性を持つ。
- **代替案**: egui (eframe)。軽量だが、テキストレンダリングや日本語などのシステムフォントピッカー、リッチなトレイ連携において Web 技術ベースの Tauri の方が柔軟。

### 3. クロスプラットフォーム画面キャプチャ: `xcap` クレート
- **決定**: 画面キャプチャバックエンドに Rust の `xcap` クレートを採用。
- **理由**: Windows、Linux (X11/Wayland)、macOS のマルチモニター、ウィンドウ列挙、特定領域のキャプチャをクロスプラットフォームでネイティブにサポートしている。
- **代替案**: OS 個別のネイティブ API（DirectX/GDI, X11/PipeWire）の手動実装。工数が肥大化し保守性が低下するため、実績のあるクロスプラットフォーム抽象ライブラリを利用する。

### 4. 再編集データ保存: PNG `tEXt` チャンク埋め込み
- **決定**: PNG 標準仕様の `tEXt` チャンクに `markits:annotations` キーで JSON データを埋め込む。
- **理由**:
  - 単一の `.png` ファイルでありながら、通常の画像ビューアやブラウザでは完成画像としてそのまま閲覧可能。
  - MarkIts Desktop でファイルを開いた場合、メタデータからセマンティックアノテーションと背景画像を自動復元して再編集可能となる。
  - 外部の別ファイル（`.json` など）が不要で、ユーザーのファイル管理体験が極めてスムーズ。
- **代替案**: `.markits` プロジェクトファイルと `.png` 画像の分離。2つのファイルが必要になり、画像単体の移動時にアノテーション情報が失われやすい。

### 5. レンダリング一貫性の担保: コアエンジンのネイティブ呼び出し
- **決定**: エディタ画面での最終保存およびクリップボードコピー時の画像生成は、Tauri IPC 経由で既存の `markits` コアレンダラー（`renderer.rs`, `raster.rs`）を直接呼び出す。
- **理由**: CLI やライブラリで出力される SVG / PNG と完全に 100% 同一のフォント描画・ストローク・テーマスタイルを保証できる。

## Risks / Trade-offs

- **[Risk] Linux (Wayland) 環境での画面キャプチャおよびグローバルショートカットの制約**
  → **Mitigation**: Wayland セキュリティモデルへの対応として、XDG Desktop Portal を活用しつつ、X11 と Wayland の両方で graceful なエラーハンドリングとパーミッション案内を提供する。
- **[Risk] フロントエンドビルド依存（Node.js / npm）の追加**
  → **Mitigation**: コアの `markits` ビルドには一切影響を与えず、デスクトップアプリのビルド時のみフロントエンドビルドステップを走らせる独立した構成にする。
- **[Risk] 巨大な画面キャプチャ時のメモリ消費**
  → **Mitigation**: キャプチャ画像バッファをメモリ内で効率的に扱い、エディタキャンバスへの受け渡しには共有メモリまたは最適化されたデータURL/バイナリストリームを使用する。
