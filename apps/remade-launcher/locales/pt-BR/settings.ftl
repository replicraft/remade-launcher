# Configurações: idioma e região.

settings-language = Idioma
    .description = O idioma da interface. Automático segue o sistema ({ $system }).
settings-language-auto = Automático
settings-time-format = Formato de hora
    .description = Automático segue a região do seu sistema.
time-format-system = Sistema
time-format-12h = 12 horas (3:42 PM)
time-format-24h = 24 horas (15:42)
settings-date-format = Formato de data
    .description = Automático segue a região do seu sistema.
date-format-system = Sistema
date-format-mdy = out. 24, 2026
date-format-dmy = 24 de out. de 2026
date-format-iso = 2026-10-24
settings-region-example = Exemplo: { $today }   ·   { $earlier }

# Configurações: painéis e o restante.

settings-title = Configurações
settings-appearance = Aparência
settings-region-panel = Idioma e região
settings-updates-panel = Atualizações
    .description = Como os apps instalados são atualizados. O Launcher se atualiza separadamente, mais abaixo.
settings-launcher-panel = Atualizações do Launcher
    .description = Como o Launcher se mantém atualizado. Isso não muda como seus apps são atualizados.
settings-notifications-panel = Notificações
settings-network-panel = Rede
settings-storage-panel = Armazenamento
settings-support-panel = Suporte
settings-about-panel = Sobre

settings-theme = Tema
theme-system = Sistema
theme-light = Claro
theme-dark = Escuro
settings-scale = Escala da interface
    .description = Deixa tudo maior ou menor. Útil em telas de alta resolução.
settings-start-page = Abrir em
    .description = A página que o Launcher mostra ao iniciar.
start-page-auto = Biblioteca se houver apps, senão Explorar
start-page-library = Biblioteca
start-page-explore = Explorar
start-page-last-used = Onde parei
settings-layout-note = Seu layout, como a barra lateral recolhida, grade ou lista e o tamanho da janela, é lembrado automaticamente.

update-mode-pinned = Fixado
    .description = Fica na versão atual. Nada o atualiza, e nenhum botão de atualização é mostrado, até você escolher outro nível.
update-mode-manual = Atualização manual (padrão)
    .description = Procura atualizações quando você abre o Launcher. Os apps instalados mostram um botão Atualizar, e você decide quando instalar.
update-mode-on-launch = Atualizar automaticamente ao iniciar o Launcher
    .description = Toda vez que você inicia o Launcher, ele instala automaticamente as atualizações de apps elegíveis. Não continua em execução nem atualiza apps em segundo plano depois que você o fecha.
update-mode-daemon = Atualizar automaticamente com um atualizador sempre ativo em segundo plano (perigoso)
    .description = Inicia um atualizador leve em segundo plano quando você entra na sessão. Ele verifica ao iniciar e a cada 24 horas enquanto estiver ativo, e também quando você abre o Launcher. As atualizações são instaladas sem confirmação individual. Versões maliciosas, comprometidas ou com defeito da origem podem ser instaladas automaticamente e danificar programas ou dados.
settings-min-age = Idade mínima padrão da versão para atualização automática
    .automatic = Uma versão precisa ter pelo menos essa idade para ser instalada por uma atualização automática. Cada app pode ter a sua.
    .manual = Usado apenas nos modos automáticos. Nunca segura uma atualização que você inicia.
minimum-age-0 = Sem espera
minimum-age-3600 = 1 hora
minimum-age-21600 = 6 horas
minimum-age-43200 = 12 horas
minimum-age-86400 = 24 horas
minimum-age-259200 = 3 dias
minimum-age-604800 = 7 dias
settings-min-age-warning = Sem espera, a versão é instalada assim que é publicada. Isso é mais arriscado.
settings-background = Atualizador em segundo plano
settings-background-checking = Verificando…
settings-background-active = Ativo
settings-background-stopped = Parado
    .detail = Ele inicia quando você entrar na sessão.
settings-background-pending = Aguardando aprovação do sistema
    .detail = Aprove nos itens de login do sistema.
settings-background-disabled = Desativado pelo sistema
    .detail = Você o desativou nas configurações do sistema. O Launcher não vai reativá-lo.
settings-background-not-installed = Não instalado
settings-background-failed = Falhou
settings-background-unavailable = Indisponível aqui
settings-background-last-check = Última verificação: { $when }
settings-background-last-result = Último resultado: { $outcome }
settings-background-next-check = Próxima verificação: { $when }
settings-background-stop = Parar atualizações em segundo plano

launcher-updates-automatic = Automático
    .description = Baixa um novo Launcher em segundo plano e troca para ele quando você fechar este.
launcher-updates-notify = Avisar
    .description = Avisa quando houver um novo Launcher e deixa o resto com você.
launcher-updates-off = Desligado
    .description = Nunca procura um novo Launcher.
settings-launcher-last-checked = Última verificação: { $when }
settings-launcher-staged = A versão { $version } foi baixada. Ela inicia na próxima vez que você abrir o Launcher.
settings-launcher-available = A versão { $version } está disponível.
settings-launcher-applied = Trocou da versão { $from } para a { $to } em { $when }.
settings-launcher-checking = Verificando…
settings-launcher-check-now = Verificar agora
settings-launcher-download = Baixar agora
settings-launcher-roll-back = Voltar à versão anterior

settings-notify-updates = Avisar quando houver atualizações
settings-notify-finished = Avisar quando uma instalação ou atualização terminar
settings-notify-note = As notificações aparecem apenas enquanto o Launcher está em segundo plano. O sistema pode pedir permissão na primeira vez.
settings-notify-test = Enviar uma notificação de teste

settings-proxy = Proxy
    .description = Automático usa as configurações de proxy do sistema, incluindo as variáveis de ambiente usuais.
settings-proxy-auto = Automático (sistema)
settings-proxy-none = Sem proxy
settings-proxy-custom = Endereço personalizado
settings-proxy-hint = http://, https:// ou socks5://. Não inclua usuário e senha: eles seriam salvos sem proteção.
settings-proxy-use = Usar este proxy
settings-proxy-problem-empty = Digite um endereço, como http://proxy.example:8080.
settings-proxy-problem-address = Esse não é um endereço válido.
settings-proxy-problem-scheme = Use um endereço http://, https:// ou socks5://.
settings-proxy-problem-credentials = Não inclua usuário e senha. Eles seriam salvos sem proteção.
settings-timeout = Tempo limite das requisições
    .description = Quanto esperar por um servidor antes de desistir. Conexões lentas podem precisar de mais tempo.
settings-seconds = { $count ->
    [one] { $count } segundo
   *[other] { $count } segundos
}
settings-github = Acesso ao GitHub
settings-github-intro = O GitHub permite 60 consultas por hora para qualquer pessoa e 5.000 com um token gratuito. O Launcher lembra o que já perguntou, então a maioria das verificações não gasta nada. Adicione um token só se você tem muitos apps e ainda atinge o limite. Um token sem permissões basta, pois tudo o que o Launcher lê é público.
settings-token-stored = Há um token salvo no repositório de credenciais do sistema.
settings-token-remove = Remover token
settings-token-environment = Um token vem da configuração { $name } do seu ambiente.
settings-token-unavailable = O repositório de credenciais do sistema não está disponível, então não dá para guardar um token com segurança.
settings-token-paste = Cole um token
settings-token-save = Salvar token
settings-rate-check = Ver meu limite
settings-rate-left = { $remaining } de { $limit } consultas restantes. Reinicia { $resets }. { $note }
settings-rate-token = Usando seu token.
settings-rate-anonymous = Sem login.

settings-measuring = Medindo…
settings-show-folder = Mostrar no gerenciador de arquivos
settings-storage-apps = Seus apps
    .description = Tudo o que o Launcher instalou. Desinstale um app para liberar o espaço dele.
settings-storage-leftover = Downloads restantes
    .description = Arquivos de instalações interrompidas. É seguro removê-los; nada em execução é afetado.
settings-clear-leftovers = Limpar restos
settings-storage-cache = Consultas lembradas
    .description = Respostas do GitHub guardadas para deixar as verificações mais rápidas e baratas. São recarregadas conforme preciso.
settings-clear-cache = Limpar consultas lembradas
settings-storage-data = Dados do Launcher
    .description = Os registros do próprio Launcher: o que está instalado, configurações e histórico.

settings-support-copy = Copiar relatório de suporte
settings-support-note = O relatório lista apenas versões e contagens. Não tem caminhos de arquivos, links nem nomes de apps.

settings-about-by = { $name } por Replicraft
settings-about-version = Versão { $version }
settings-about-platform = Plataforma
settings-about-license = Licença
settings-about-website = Site
settings-about-source = Ver o código-fonte no GitHub
settings-about-restore = Restaurar padrões
