## Why

AIやアプリケーションがチュートリアルやUI解説画像を生成する際、AIにSVGの絶対座標や矢印のベジェ曲線を直接生成させると、ラベルの重なり、キャンバス外へのはみ出し、デザインの不整合などの問題が発生しやすくなります。
MarkItsは「AIは『何を説明するか（意味的意図）』を決め、MarkItsは『どう綺麗に見せるか（レイアウト・描画）』を決める」という責務の分離を実現し、意味的なアノテーションJSONから高品質なオーバーレイSVGを決定論的かつ自動的に生成する軽量RustライブラリおよびCLIを提供します。

## What Changes

- **Rustコアライブラリ (`markits`) の基本実装**:
  - セマンティックアノテーションとシーン（Canvas, Target Rect, Style, Position Hint）を表すデータモデルおよびJSONの相互変換
  - 自動レイアウトエンジン（候補位置生成、衝突検知、キャンバスはみ出し防止、複数アノテーションのスコアリング、矢印経路決定）
  - 8種類の基本アノテーション描画（Arrow, Rect, RoundedRect, Circle/Ellipse, Label, Callout, Badge, Spotlight）
  - 意味的スタイル（primary, secondary, warning, danger, info, step）を管理するテーマ機構と、透明背景のベクターSVGレンダラー
- **CLIツール (`markits`) の提供**:
  - ファイルおよび標準入力（stdin）からアノテーションJSONを受け取り、標準出力にSVG文字列を出力する `markits render` コマンド

## Capabilities

### New Capabilities
- `annotation-model`: キャンバス、ターゲット矩形、アノテーション種別、セマンティックスタイル、配置ヒントを表現するデータモデルとJSON仕様
- `layout-engine`: 候補位置生成、境界・重なり評価、衝突検知、矢印ルーティング、最適配置アルゴリズム
- `svg-renderer`: 意味的テーマに基づくSVG要素生成、Spotlight用SVGマスク、透明背景SVG出力
- `cli`: CLI引数パース、ファイル/標準入力読み込み、標準出力へのSVG出力インターフェース

### Modified Capabilities
<!-- 新規作成のため既存の変更はなし -->

## Impact

- 新規Rustクレートの作成（`lib.rs` および `main.rs`）
- 必要な依存ライブラリの追加（JSON直列化・逆直列化のための `serde`, `serde_json` など）
- 画像データそのものは扱わず、JSON -> SVG のテキストベース処理として完結
