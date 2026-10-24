# Ajustes: idioma y región.

settings-language = Idioma
    .description = El idioma de la interfaz. Automático sigue el de tu sistema ({ $system }).
settings-language-auto = Automático
settings-time-format = Formato de hora
    .description = Automático sigue la región de tu sistema.
time-format-system = Sistema
time-format-12h = 12 horas (3:42 p. m.)
time-format-24h = 24 horas (15:42)
settings-date-format = Formato de fecha
    .description = Automático sigue la región de tu sistema.
date-format-system = Sistema
date-format-mdy = oct 24, 2026
date-format-dmy = 24 oct 2026
date-format-iso = 2026-10-24
settings-region-example = Ejemplo: { $today }   ·   { $earlier }

# Ajustes: paneles y el resto.

settings-title = Ajustes
settings-appearance = Apariencia
settings-region-panel = Idioma y región
settings-updates-panel = Actualizaciones
    .description = Cómo se actualizan las apps instaladas. El Launcher se actualiza por separado, más abajo.
settings-launcher-panel = Actualizaciones del Launcher
    .description = Cómo se mantiene al día el Launcher. Esto no cambia cómo se actualizan tus apps.
settings-notifications-panel = Notificaciones
settings-network-panel = Red
settings-storage-panel = Almacenamiento
settings-support-panel = Soporte
settings-about-panel = Acerca de

settings-theme = Tema
theme-system = Sistema
theme-light = Claro
theme-dark = Oscuro
settings-scale = Escala de la interfaz
    .description = Hace todo más grande o más pequeño. Útil en pantallas de alta resolución.
settings-start-page = Abrir en
    .description = La página que muestra el Launcher al iniciar.
start-page-auto = Biblioteca si tengo apps; si no, Explorar
start-page-library = Biblioteca
start-page-explore = Explorar
start-page-last-used = Donde me quedé
settings-layout-note = Tu diseño, como la barra lateral plegada, la cuadrícula o la lista, y el tamaño de la ventana, se recuerda automáticamente.

update-mode-manual = Actualizaciones manuales (predeterminado)
    .description = Busca actualizaciones cuando abres el Launcher. Las apps instaladas muestran un botón Actualizar para que decidas cuándo instalar.
update-mode-on-launch = Actualizar automáticamente al iniciar el Launcher
    .description = Cada vez que inicias el Launcher, instala automáticamente las actualizaciones de apps que cumplan los requisitos. No sigue ejecutándose ni actualiza apps en segundo plano después de que lo cierres.
update-mode-daemon = Actualizar automáticamente con un actualizador siempre activo en segundo plano (peligroso)
    .description = Inicia un actualizador ligero en segundo plano cuando inicias sesión. Busca actualizaciones al arrancar y cada 24 horas mientras está activo, y también cuando abres el Launcher. Las actualizaciones se instalan sin confirmación individual. Versiones maliciosas, comprometidas o defectuosas del origen podrían instalarse automáticamente y dañar el software o los datos.
settings-min-age = Antigüedad mínima predeterminada de las versiones para actualizar automáticamente
    .automatic = Una versión debe tener al menos esta antigüedad para que una actualización automática la instale. Cada app puede usar la suya.
    .manual = Solo lo usan los modos automáticos. Nunca retrasa una actualización que inicias tú.
minimum-age-0 = Sin espera
minimum-age-3600 = 1 hora
minimum-age-21600 = 6 horas
minimum-age-43200 = 12 horas
minimum-age-86400 = 24 horas
minimum-age-259200 = 3 días
minimum-age-604800 = 7 días
settings-min-age-warning = Sin espera, una versión se instala en el momento en que se publica. Eso es más arriesgado.
settings-background = Actualizador en segundo plano
settings-background-checking = Comprobando…
settings-background-active = Activo
settings-background-stopped = Detenido
    .detail = Se iniciará cuando inicies sesión.
settings-background-pending = Pendiente de aprobación del sistema
    .detail = Apruébalo en los elementos de inicio de sesión de tu sistema.
settings-background-disabled = Desactivado por el sistema
    .detail = Lo desactivaste en los ajustes del sistema. El Launcher no lo volverá a activar.
settings-background-not-installed = No instalado
settings-background-failed = Con error
settings-background-unavailable = No disponible aquí
settings-background-last-check = Última comprobación: { $when }
settings-background-last-result = Último resultado: { $outcome }
settings-background-next-check = Próxima comprobación: { $when }
settings-background-stop = Detener las actualizaciones en segundo plano

launcher-updates-automatic = Automáticas
    .description = Descarga un Launcher nuevo en segundo plano y cambia a él cuando cierres este.
launcher-updates-notify = Avisarme
    .description = Avisa cuando haya un Launcher nuevo disponible y deja el resto en tus manos.
launcher-updates-off = Desactivadas
    .description = Nunca busca un Launcher nuevo.
settings-launcher-last-checked = Última comprobación { $when }
settings-launcher-staged = { $version } está descargada. Se iniciará la próxima vez que abras el Launcher.
settings-launcher-available = { $version } está disponible.
settings-launcher-applied = Se cambió de { $from } a { $to } el { $when }.
settings-launcher-checking = Comprobando…
settings-launcher-check-now = Comprobar ahora
settings-launcher-download = Descargar ahora
settings-launcher-roll-back = Volver a la versión anterior

settings-notify-updates = Avisarme cuando haya actualizaciones disponibles
settings-notify-finished = Avisarme cuando termine una instalación o actualización
settings-notify-note = Las notificaciones aparecen solo mientras el Launcher está en segundo plano. Es posible que tu sistema pida permiso la primera vez.
settings-notify-test = Enviar una notificación de prueba

settings-proxy = Proxy
    .description = Automático usa los ajustes de proxy de tu sistema, incluidas las variables de entorno habituales.
settings-proxy-auto = Automático (sistema)
settings-proxy-none = Sin proxy
settings-proxy-custom = Dirección personalizada
settings-proxy-hint = http://, https:// o socks5://. No incluyas nombres de usuario ni contraseñas: se guardarían sin protección.
settings-proxy-use = Usar este proxy
settings-proxy-problem-empty = Escribe una dirección, como http://proxy.example:8080.
settings-proxy-problem-address = Esa dirección no es válida.
settings-proxy-problem-scheme = Usa una dirección http://, https:// o socks5://.
settings-proxy-problem-credentials = No incluyas el nombre de usuario ni la contraseña. Se guardarían sin protección.
settings-timeout = Tiempo de espera de las solicitudes
    .description = Cuánto esperar a un servidor antes de rendirse. Las conexiones lentas pueden necesitar más tiempo.
settings-seconds = { $count ->
    [one] { $count } segundo
   *[other] { $count } segundos
}
settings-github = Acceso a GitHub
settings-github-intro = GitHub permite 60 consultas por hora a cualquier persona, y 5000 con un token gratuito. El Launcher recuerda lo que ya preguntó, así que la mayoría de las comprobaciones no cuestan nada. Agrega un token solo si tienes muchas apps y aun así llegas al límite. Basta con un token sin permisos, porque todo lo que lee el Launcher es público.
settings-token-stored = Hay un token guardado en el almacén de credenciales de tu sistema.
settings-token-remove = Quitar token
settings-token-environment = Un token proviene del ajuste { $name } de tu entorno.
settings-token-unavailable = El almacén de credenciales de tu sistema no está disponible, así que no se puede guardar un token de forma segura.
settings-token-paste = Pega un token
settings-token-save = Guardar token
settings-rate-check = Comprobar mi límite
settings-rate-left = Quedan { $remaining } de { $limit } consultas. Se restablece { $resets }. { $note }
settings-rate-token = Usando tu token.
settings-rate-anonymous = Sin sesión iniciada.

settings-measuring = Midiendo…
settings-show-folder = Mostrar en el explorador de archivos
settings-storage-apps = Tus apps
    .description = Todo lo que el Launcher ha instalado. Desinstala una app para liberar su espacio.
settings-storage-leftover = Descargas sobrantes
    .description = Archivos de instalaciones que se interrumpieron. Se pueden quitar sin riesgo; no se toca nada que esté en ejecución.
settings-clear-leftovers = Borrar sobrantes
settings-storage-cache = Consultas recordadas
    .description = Respuestas de GitHub que se guardan para que las comprobaciones sean más rápidas y baratas. Se vuelven a llenar cuando hace falta.
settings-clear-cache = Borrar consultas recordadas
settings-storage-data = Datos del Launcher
    .description = Los registros propios del Launcher: qué está instalado, los ajustes y el historial.

settings-support-copy = Copiar informe de soporte
settings-support-note = El informe solo incluye versiones y recuentos. No tiene rutas de archivos, enlaces ni nombres de apps.

settings-about-by = { $name } de Replicraft
settings-about-version = Versión { $version }
settings-about-platform = Plataforma
settings-about-license = Licencia
settings-about-website = Sitio web
settings-about-source = Ver el código fuente en GitHub
settings-about-restore = Restablecer valores predeterminados
