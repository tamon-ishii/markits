# WindowsでJava製アプリのUI情報を取得する

PyCharmなどのJava/Swing製アプリは、通常のWindows UI Automationではウィンドウ枠だけが取得され、内部のボタンや入力欄が取得できない場合があります。Markitsは、対象アプリに同梱されたJava Access Bridgeを使って内部のUI部品を読み取ります。

## PyCharmの設定

1. PyCharmに同梱されたJavaの`bin/jabswitch.exe -enable`を実行します。標準のインストール先では`<PyCharmのインストール先>/jbr/bin/jabswitch.exe`です。
2. PyCharmの「設定 → 外観＆振る舞い → 外観」にある「スクリーンリーダーのサポート」を有効にします。バージョンによっては「アクセシビリティ」にあります。
3. 作業を保存し、PyCharmを再起動します。
4. Java Access Bridge対応のMarkitsで、PyCharmのウィンドウをキャプチャし直します。過去の画像に不足しているUI情報が自動で追加されるわけではありません。

スクリーンリーダー対応を有効にすると、PyCharmのツールチップや一部コントロールの動作も変わります。Markitsはこの設定のためにPyCharmを強制終了しません。

Java用の取得処理は専用スレッドでメッセージを受信し、対象ウィンドウのプロセス・範囲を照合します。読み取り時間・探索階層・要素数を制限し、内部部品が取得できない場合はウィンドウ枠だけを成功件数に数えません。

- [PyCharmのアクセシビリティ設定](https://www.jetbrains.com/help/pycharm/accessibility.html)
- [Java Access Bridgeの有効化](https://docs.oracle.com/en/java/javase/27/access/enabling-testing-java-access-bridge.html)
- [Java Access Bridge API](https://docs.oracle.com/en/java/javase/22/access/java-access-bridge-api.html)
