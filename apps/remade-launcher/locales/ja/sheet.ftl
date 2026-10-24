# ja: シート（インストールの確認、メッセージ、アンインストール、初期設定に戻す、すべてアップデート、ノート）と GitHub から追加。

sheet-ok = OK
sheet-close = 閉じる

sheet-choose-title = 合うパッケージが複数あります
sheet-choose-body = このデバイスに同じくらい合うパッケージが複数あり、ランチャーはどれを選ぶべきか推測しません。これはカタログ項目の不備であり、あなたの操作によるものではありません。

sheet-uninstall-title = { $name } をアンインストールしますか？
sheet-uninstall-installed = インストール日: { $date }
sheet-uninstall-location = 保存場所
sheet-uninstall-what-happens = 実行される内容:
sheet-uninstall-removes = ランチャーがインストールしたファイルは削除されます。
sheet-uninstall-keeps = あなたが編集したものと、アプリ自身が保存したデータは残ります。
sheet-uninstall-report = 完了後、削除されたものと残ったものが正確に表示されます。
sheet-uninstall-confirm = アンインストール

sheet-mode-intro = オンにする前に、次の点をご確認ください。
sheet-mode-risk-behavior = 新しいバージョンにより、アプリの動作が変わったり、動かなくなったりすることがあります。
sheet-mode-risk-compromised = 公開元のアカウントが乗っ取られた場合、有害なリリースがインストールされるおそれがあります。
sheet-mode-risk-migration = アップデートにより、古いバージョンでは読み取れない形でアプリのデータが移行されることがあります。
sheet-mode-risk-unattended = アップデートは確認なしで実行され、どのスキャンもソフトウェアの安全性を保証できません。
sheet-mode-daemon-warning = さらに、サインインのたびに起動し、ランチャーを閉じた後も動き続けるバックグラウンドプログラムが登録されます。オフにするには、ここか、システムのログイン項目で操作してください。
sheet-mode-acknowledge = 内容を理解したうえで、自動アップデートを有効にします
sheet-mode-confirm = オンにする

sheet-update-all-title = すべてアップデートしますか？
sheet-update-all-line = { $name }: { $from } → { $to }
sheet-update-all-note = アプリは1つずつ確認され、置き換えられます。1つが失敗しても、ほかのアプリは続行されます。
sheet-update-all-confirm = すべてアップデート

sheet-notes-empty = このリリースにはノートがありません。

sheet-restore-title = 初期設定に戻しますか？
sheet-restore-intro = ランチャー自身の設定を、新規インストール時の状態に戻します。
sheet-restore-appearance = 外観はシステムに合わせる設定に戻ります。
sheet-restore-updates = アップデートは手動に戻り、最低経過時間は1時間になります。
sheet-restore-app-settings = 固定を含め、各アプリのアップデート設定がすべてクリアされます。
sheet-restore-daemon = バックグラウンドアップデーターをオンにしていた場合は、オフになります。
sheet-restore-note = インストール済みのアプリとそのファイルには影響しません。この操作は取り消せません。
sheet-restore-confirm = 初期設定に戻す

sheet-removed-title = { $name } をアンインストールしました
sheet-removed-count = { $count } 個のファイルを削除しました。
sheet-removed-kept = 作成したものが失われないよう、一部のファイルを残しました。

sheet-review-install = インストール
sheet-review-update = アップデート
sheet-review-install-title = { $name } のインストール
sheet-review-update-title = { $name } のアップデート
sheet-review-source = 提供元
sheet-review-unreviewed = Remade はこの提供元を審査していません。信頼できる相手のソフトウェアだけをインストールしてください。
sheet-review-verified = 検証済みの Remade カタログに掲載されています。
sheet-review-version = バージョン
sheet-review-file = ファイル
sheet-review-checked-against = 確認の根拠
sheet-review-skipped = 新しいリリース { $tag } をスキップしました: { $reason }
sheet-review-acknowledge = この提供元が Remade に審査されていないことを理解しました
sheet-evidence-host-digest = GitHub が報告するファイルのチェックサム。ファイルが破損していないことは分かりますが、作成者は分かりません
sheet-evidence-pinned = 署名済みの Remade カタログに固定されたチェックサム
sheet-evidence-signature = 公開元自身の署名
sheet-evidence-local = ダウンロードしたファイル以外にありません。比較用のチェックサムは公開されていません

add-title = GitHub から追加
add-intro = 公開されている GitHub リポジトリまたはリリースのリンクを貼り付けてください。この方法で追加したアプリは Remade の審査を受けておらず、チェックサムでは、ファイルの作成者や安全性は分かりません。
add-url-hint = https://github.com/owner/repository
add-look-up = 検索
add-release = リリース { $tag }
add-partial = 最近のリリースのみ確認しました。
add-no-files = このリリースにはダウンロードできるアプリのファイルがありません。
add-review-install = インストールを確認
add-download-only = ダウンロードのみ
add-fit-matches = このデバイス向けの名前です
add-fit-unknown = 名前からはどのデバイス向けか分かりません
add-fit-mismatch = 別のデバイス向けの名前です
