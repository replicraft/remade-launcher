# zh-CN: 弹窗（安装审阅、消息、卸载、恢复默认设置、全部更新、说明）以及“从 GitHub 添加”。

sheet-ok = 确定
sheet-close = 关闭

sheet-choose-title = 有多个软件包适用
sheet-choose-body = 有多个软件包同样适合此设备，启动器不会猜测你想要哪一个。这是目录条目的缺漏，不是你的操作所致。

sheet-uninstall-title = 卸载 { $name }？
sheet-uninstall-installed = 安装于 { $date }
sheet-uninstall-location = 所在位置
sheet-uninstall-what-happens = 将会发生：
sheet-uninstall-removes = 启动器安装的文件会被删除。
sheet-uninstall-keeps = 你编辑过的内容，以及应用自己保存的数据，会被保留。
sheet-uninstall-report = 完成后，你会看到究竟删除了什么、保留了什么。
sheet-uninstall-confirm = 卸载

sheet-mode-intro = 开启之前，请了解：
sheet-mode-risk-behavior = 新版本可能改变应用的行为，或使其无法运行。
sheet-mode-risk-compromised = 如果发布者的账户被盗用，有害的版本可能被安装。
sheet-mode-risk-migration = 更新可能以旧版本无法读取的方式迁移应用的数据。
sheet-mode-risk-unattended = 更新会在不询问的情况下运行，任何扫描都无法保证软件是安全的。
sheet-mode-daemon-warning = 这还会注册一个后台程序，它在每次登录时启动，并在你关闭启动器后继续运行。你可以在此处或系统的登录项中将其关闭。
sheet-mode-acknowledge = 我已了解，并希望使用自动更新
sheet-mode-confirm = 开启

sheet-update-all-title = 全部更新？
sheet-update-all-line = { $name }：{ $from } → { $to }
sheet-update-all-note = 每个应用会逐个检查并替换。其中一个失败不会影响其他应用。
sheet-update-all-confirm = 全部更新

sheet-notes-empty = 此版本没有说明。

sheet-restore-title = 恢复默认设置？
sheet-restore-intro = 这会把启动器自身的设置恢复为全新安装时的状态：
sheet-restore-appearance = 外观重新跟随系统。
sheet-restore-updates = 更新恢复为手动，最短发布时长为一小时。
sheet-restore-app-settings = 每个应用自己的更新设置都会被清除，包括已固定的版本。
sheet-restore-daemon = 如果你开启过后台更新程序，它会被关闭。
sheet-restore-note = 你已安装的应用及其文件不受影响。此操作无法撤销。
sheet-restore-confirm = 恢复默认设置

sheet-removed-title = { $name } 已卸载
sheet-removed-count = 已删除 { $count } 个文件。
sheet-removed-kept = 为避免丢失你创建的内容，部分文件已被保留：

sheet-review-install = 安装
sheet-review-update = 更新
sheet-review-install-title = 安装 { $name }
sheet-review-update-title = 更新 { $name }
sheet-review-source = 来源
sheet-review-unreviewed = Remade 未审核此来源。请只安装你信任的人提供的软件。
sheet-review-verified = 已列入经过验证的 Remade 目录。
sheet-review-version = 版本
sheet-review-file = 文件
sheet-review-checked-against = 校验依据
sheet-review-skipped = 已跳过较新的版本 { $tag }：{ $reason }
sheet-review-acknowledge = 我了解此来源未经 Remade 审核
sheet-evidence-host-digest = GitHub 报告的该文件的校验和。它只能说明文件完整，不能说明是谁制作的
sheet-evidence-pinned = 已固定在带签名的 Remade 目录中的校验和
sheet-evidence-signature = 发布者自己的签名
sheet-evidence-local = 除下载的文件本身外没有任何依据。没有可供比对的已发布校验和

add-title = 从 GitHub 添加
add-intro = 粘贴公开的 GitHub 仓库或版本链接。Remade 未审核以这种方式添加的应用，校验和也无法说明文件是谁制作的，或是否安全。
add-url-hint = https://github.com/owner/repository
add-look-up = 查找
add-release = 版本 { $tag }
add-partial = 只检查了最近的版本。
add-no-files = 此版本没有可下载的应用文件。
add-review-install = 审阅安装
add-download-only = 仅下载
add-fit-matches = 名称针对此设备
add-fit-unknown = 名称未说明适用于哪种设备
add-fit-mismatch = 名称针对其他设备
