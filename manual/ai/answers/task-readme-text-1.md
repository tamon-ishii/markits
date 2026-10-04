<!-- ai:answer id=task-readme-text-1 source-sha256=26894114f87b1efbc80f247d06bd9388626444b7bafff055a46f280be8c8a6fa created-at=2026-10-03T17:49:38Z -->
**対象読者:** スクリーンショットを使った操作案内の作成者と、画面上の要素を指定して注釈を付ける AI エージェント。<!-- ai:fact {"claim":"UI 要素を指定して注釈を追加できる","file":"src/main.rs","contains":"Quick command to add a mark to a specific UI target without writing JSON"} -->

**説明する操作手順:**

1. `markits capture screen.png --detect-ui` で画面を撮影し、検出した UI 要素の情報を PNG に埋め込む。<!-- ai:fact {"claim":"capture の detect-ui は UI 要素を検出して出力 PNG に埋め込む","file":"src/main.rs","contains":"Detect desktop UI elements and embed UIMap metadata in the output PNG"} -->
2. `markits uimap screen.png` で UI 要素の一覧を確認する。<!-- ai:fact {"claim":"uimap は画像内の UIMap を表示する","file":"src/main.rs","contains":"Present/extract UI elements map (UIMap) from an image"} -->
3. `markits annotate screen.png --target "保存ボタン" --mark rect -o annotated.png` で対象を四角で囲み、注釈画像を保存する。<!-- ai:fact {"claim":"annotate は対象名とマーク種別を指定して PNG を出力できる","file":"src/main.rs","contains":"Output image path (.png)"} -->
