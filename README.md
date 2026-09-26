# MarkIts — Semantic Annotation SVG Engine

**MarkIts** は、スクリーンショットや画像の上に重ねる説明用アノテーションを、意味的な指示（Semantic JSON）から高品質な **SVG** として生成する軽量な Rust ライブラリおよび CLI ツールです。

> **「AIは『何を説明するか』を決め、MarkItsは『どう綺麗に見せるか』を決める」**

---

## 主な特徴

- 🎯 **決定論的自動レイアウト**: ターゲット矩形の周囲8方向への候補生成、キャンバスはみ出し防止、衝突回避、矢印ルーティング
- 📐 **多様なアノテーション種別**:
  - `callout`: 説明テキストピル＋自動ルーティング矢印
  - `pin` (ピンタグ): アイコン/記号入りピンヘッド＋方向ポインター＋連結テキストピル（Skitch スタイル）
  - `label`: 白フチ（アウトライン）付きの読みやすい説明テキスト
  - `badge`: 手順番号などを表示する円形マーカー
  - `rect` / `rounded-rect`: 領域ハイライト枠（角丸対応）
  - `circle` / `ellipse`: 円形・楕円ハイライト
  - `arrow`: ターゲットを指す矢印
  - `bullseye`: 注目点を示す同心円＋中心ドット
  - `divider`: 画面や手順を分けるセパレーターライン
  - `spotlight`: 周囲を半透明マスクで暗くし、ターゲット領域をくり抜いて強調
- 🎨 **セマンティックスタイル & テーマ**: `primary`, `secondary`, `warning`, `danger`, `info`, `step`, `pink`（Skitch マゼンタ）
- 🔲 **視覚効果オプション**: 白フチ（`outline: true`）、ドロップシャドウの有無（`shadow: true / false`）のグローバルおよび要素別切り替え
- ⚡ **Pure Rust**: C依存関係ゼロで高速・クロスプラットフォーム

---

## インストール & CLI 利用方法

### ビルド済みバイナリのダウンロード
[GitHub Releases](https://github.com/tamon-ishii/markits/releases) より、各 OS 向けの最適化済み実行可能バイナリをダウンロードしてそのまま利用できます：
- **Linux**: `x86_64` (glibc / musl static), `aarch64` (ARM64)
- **macOS**: `universal` (Apple Silicon & Intel 両対応), `aarch64` (M1/M2/M3/M4), `x86_64` (Intel)
- **Windows**: `x86_64`

### ソースコードからのビルド
```bash
cargo build --release
```

### コマンドラインからのレンダリング
```bash
# JSON ファイルから SVG を生成して標準出力
markits render input.json > overlay.svg

# パイプ (stdin) 経由で生成
cat input.json | markits render - > overlay.svg
```

---

## 入力 JSON 例 (Semantic Intent)

```json
{
  "canvas": {
    "width": 1200,
    "height": 750
  },
  "shadow": true,
  "annotations": [
    {
      "type": "divider",
      "target": [50, 310, 900, 4],
      "style": "danger"
    },
    {
      "type": "pin",
      "target": [440, 140, 60, 60],
      "icon": "?",
      "text": "これは何？",
      "style": "info",
      "position": "left",
      "outline": true
    },
    {
      "type": "bullseye",
      "target": [470, 360, 50, 50],
      "style": "danger"
    },
    {
      "type": "callout",
      "target": [80, 80, 320, 140],
      "text": "① 検索フィールドをクリック",
      "style": "primary",
      "position": "bottom"
    }
  ]
}
```

---

## Rust ライブラリとしての利用

```rust
use markits::{render_from_json, Scene};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let json_data = r#"{
        "canvas": { "width": 800, "height": 600 },
        "annotations": [
            {
                "type": "callout",
                "target": [100, 100, 200, 50],
                "text": "保存ボタン",
                "style": "primary"
            }
        ]
    }"#;

    // 直接 JSON 文字列から SVG を出力
    let svg = render_from_json(json_data)?;
    println!("{}", svg);

    Ok(())
}
```

---

## ライセンス

MIT
