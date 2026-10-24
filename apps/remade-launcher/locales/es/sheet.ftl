# es: hoja (revisión de instalación, mensajes, desinstalación, restablecer valores, actualizar todo, notas) y Agregar desde GitHub.

sheet-ok = Aceptar
sheet-close = Cerrar

sheet-choose-title = Hay varios paquetes que sirven
sheet-choose-body = Más de un paquete encaja igual de bien con este dispositivo, y el Launcher no va a adivinar cuál quieres. Es un hueco en la entrada del catálogo, no algo que hiciste tú.

sheet-uninstall-title = ¿Desinstalar { $name }?
sheet-uninstall-installed = Instalada { $date }
sheet-uninstall-location = Dónde está
sheet-uninstall-what-happens = Qué pasará:
sheet-uninstall-removes = Se eliminan los archivos que instaló el Launcher.
sheet-uninstall-keeps = Se conserva todo lo que editaste y los datos que la app guardó por su cuenta.
sheet-uninstall-report = Después verás exactamente qué se eliminó y qué se conservó.
sheet-uninstall-confirm = Desinstalar

sheet-mode-intro = Antes de activar esto, ten en cuenta que:
sheet-mode-risk-behavior = Una versión nueva puede cambiar el comportamiento de una app, o hacer que deje de funcionar.
sheet-mode-risk-compromised = Si la cuenta de un publicador se ve comprometida, se podría instalar una versión dañina.
sheet-mode-risk-migration = Una actualización puede migrar los datos de una app de un modo que las versiones anteriores no pueden leer.
sheet-mode-risk-unattended = Las actualizaciones se ejecutan sin preguntar, y ningún análisis puede garantizar que un software sea seguro.
sheet-mode-daemon-warning = Esto también registra un programa en segundo plano que se inicia cada vez que inicias sesión y sigue ejecutándose después de que cierres el Launcher. Puedes desactivarlo aquí o en los elementos de inicio de sesión de tu sistema.
sheet-mode-acknowledge = Lo entiendo y quiero actualizaciones automáticas
sheet-mode-confirm = Activar

sheet-update-all-title = ¿Actualizar todo?
sheet-update-all-line = { $name }: { $from } a { $to }
sheet-update-all-note = Cada app se comprueba y se reemplaza una por una. Si una falla, las demás continúan.
sheet-update-all-confirm = Actualizar todo

sheet-notes-empty = Esta versión no tiene notas.

sheet-restore-title = ¿Restablecer los valores predeterminados?
sheet-restore-intro = Esto deja los ajustes propios del Launcher como los tiene una instalación nueva:
sheet-restore-appearance = La apariencia vuelve a seguir a tu sistema.
sheet-restore-updates = Las actualizaciones vuelven a ser manuales, con una antigüedad mínima de una hora.
sheet-restore-app-settings = Se borran los ajustes de actualización propios de cada app, incluidos los fijados.
sheet-restore-daemon = Si activaste un actualizador en segundo plano, se desactiva.
sheet-restore-note = Tus apps instaladas y sus archivos no se tocan. Esto no se puede deshacer.
sheet-restore-confirm = Restablecer valores

sheet-removed-title = Se desinstaló { $name }
sheet-removed-count = { $count ->
    [one] Se eliminó { $count } archivo.
   *[other] Se eliminaron { $count } archivos.
}
sheet-removed-kept = Se conservaron algunos archivos para que no se pierda nada de lo que creaste:

sheet-review-install = Instalar
sheet-review-update = Actualizar
sheet-review-install-title = Instalar { $name }
sheet-review-update-title = Actualizar { $name }
sheet-review-source = Origen
sheet-review-unreviewed = Remade no ha revisado este origen. Instala software solo de personas en las que confíes.
sheet-review-verified = Aparece en el catálogo verificado de Remade.
sheet-review-version = Versión
sheet-review-file = Archivo
sheet-review-checked-against = Verificado con
sheet-review-skipped = Se omitió la versión más reciente { $tag }: { $reason }
sheet-review-acknowledge = Entiendo que Remade no revisó este origen
sheet-evidence-host-digest = la suma de verificación que GitHub informa para el archivo. Demuestra que el archivo está intacto, no quién lo creó
sheet-evidence-pinned = una suma de verificación fijada en el catálogo firmado de Remade
sheet-evidence-signature = la firma del propio publicador
sheet-evidence-local = nada más allá de la propia descarga. No se publicó ninguna suma de verificación con la que comparar

add-title = Agregar desde GitHub
add-intro = Pega el enlace de un repositorio o de una versión pública de GitHub. Remade no ha revisado las apps que agregas de esta forma, y una suma de verificación no puede demostrar quién creó un archivo ni que sea seguro.
add-url-hint = https://github.com/owner/repository
add-look-up = Buscar
add-release = Versión { $tag }
add-partial = Solo se comprobaron las versiones más recientes.
add-no-files = Esta versión no tiene archivos de app descargables.
add-review-install = Revisar instalación
add-download-only = Solo descargar
add-fit-matches = Nombrado para este dispositivo
add-fit-unknown = El nombre no indica para qué dispositivo es
add-fit-mismatch = Nombrado para otro dispositivo
