# pt-BR: janelas (revisão de instalação, mensagens, desinstalação, restaurar padrões, atualizar tudo, notas) e Adicionar do GitHub.

sheet-ok = OK
sheet-close = Fechar

sheet-choose-title = Vários pacotes servem
sheet-choose-body = Mais de um pacote serve para este dispositivo igualmente bem, e o Launcher não vai adivinhar qual você quer. Isso é uma lacuna na entrada do catálogo, não algo que você fez.

sheet-uninstall-title = Desinstalar { $name }?
sheet-uninstall-installed = Instalado em { $date }
sheet-uninstall-location = Onde fica
sheet-uninstall-what-happens = O que acontece:
sheet-uninstall-removes = Os arquivos que o Launcher instalou são removidos.
sheet-uninstall-keeps = Tudo o que você editou e os dados que o app salvou por conta própria são mantidos.
sheet-uninstall-report = Depois, você vê exatamente o que foi removido e o que ficou.
sheet-uninstall-confirm = Desinstalar

sheet-mode-intro = Antes de ativar, saiba que:
sheet-mode-risk-behavior = Uma nova versão pode mudar o comportamento de um app ou fazê-lo parar de funcionar.
sheet-mode-risk-compromised = Se a conta de um publicador for comprometida, uma versão nociva pode ser instalada.
sheet-mode-risk-migration = Uma atualização pode migrar os dados de um app de um jeito que versões antigas não conseguem ler.
sheet-mode-risk-unattended = As atualizações rodam sem perguntar, e nenhuma verificação garante que um software seja seguro.
sheet-mode-daemon-warning = Isso também registra um programa em segundo plano que inicia sempre que você entra na sua conta e continua rodando depois que você fecha o Launcher. Você pode desativá-lo aqui ou nos itens de login do sistema.
sheet-mode-acknowledge = Eu entendo e quero atualizações automáticas
sheet-mode-confirm = Ativar

sheet-update-all-title = Atualizar tudo?
sheet-update-all-line = { $name }: { $from } para { $to }
sheet-update-all-note = Cada app é verificado e substituído um de cada vez. Uma falha em um não interrompe os outros.
sheet-update-all-confirm = Atualizar tudo

sheet-notes-empty = Esta versão não tem notas.

sheet-restore-title = Restaurar padrões?
sheet-restore-intro = Isso devolve as configurações do próprio Launcher ao estado de uma instalação nova:
sheet-restore-appearance = A aparência volta a seguir o sistema.
sheet-restore-updates = As atualizações voltam a ser manuais, com idade mínima de uma hora.
sheet-restore-app-settings = As configurações de atualização de cada app são apagadas, inclusive as versões fixadas.
sheet-restore-daemon = Um atualizador em segundo plano, se você ativou algum, é desligado.
sheet-restore-note = Seus apps instalados e os arquivos deles não são alterados. Isso não pode ser desfeito.
sheet-restore-confirm = Restaurar padrões

sheet-removed-title = { $name } foi desinstalado
sheet-removed-count = { $count ->
    [one] { $count } arquivo removido.
   *[other] { $count } arquivos removidos.
}
sheet-removed-kept = Alguns arquivos foram mantidos para que nada do que você criou se perca:

sheet-review-install = Instalar
sheet-review-update = Atualizar
sheet-review-install-title = Instalar { $name }
sheet-review-update-title = Atualizar { $name }
sheet-review-source = Origem
sheet-review-unreviewed = A Remade não revisou esta origem. Instale software apenas de pessoas em quem você confia.
sheet-review-verified = Listado no catálogo verificado da Remade.
sheet-review-version = Versão
sheet-review-file = Arquivo
sheet-review-checked-against = Verificado com
sheet-review-skipped = Versão mais recente { $tag } ignorada: { $reason }
sheet-review-acknowledge = Entendo que esta origem não foi revisada pela Remade
sheet-evidence-host-digest = o checksum que o GitHub informa para o arquivo. Ele mostra que o arquivo está íntegro, não quem o criou
sheet-evidence-pinned = um checksum fixado no catálogo assinado da Remade
sheet-evidence-signature = a assinatura do próprio publicador
sheet-evidence-local = nada além do próprio download. Nenhum checksum foi publicado para comparação

add-title = Adicionar do GitHub
add-intro = Cole o link de um repositório ou de uma versão pública do GitHub. A Remade não revisou os apps adicionados dessa forma, e um checksum não mostra quem criou um arquivo nem se ele é seguro.
add-url-hint = https://github.com/owner/repository
add-look-up = Buscar
add-release = Versão { $tag }
add-partial = Apenas as versões mais recentes foram verificadas.
add-no-files = Esta versão não tem arquivos de app para baixar.
add-review-install = Revisar instalação
add-download-only = Somente download
add-fit-matches = Nomeado para este dispositivo
add-fit-unknown = O nome não diz para qual dispositivo
add-fit-mismatch = Nomeado para outro dispositivo
