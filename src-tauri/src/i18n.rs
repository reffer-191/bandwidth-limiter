//! The few user-visible strings produced on the Rust side (tray menu, driver
//! and updater errors). The UI has its own dictionary in `src/lib/i18n.tsx`.

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Lang {
    Es,
    En,
}

/// Resolves the `language` setting ("system" | "es" | "en").
pub fn resolve(setting: &str) -> Lang {
    match setting {
        "es" => Lang::Es,
        "en" => Lang::En,
        _ => system_lang(),
    }
}

fn system_lang() -> Lang {
    // Primary language id 0x0A is Spanish (all regional variants).
    let id = unsafe { windows_sys::Win32::Globalization::GetUserDefaultUILanguage() };
    if id & 0x3ff == 0x0a { Lang::Es } else { Lang::En }
}

pub fn tr(lang: Lang, key: &str) -> &'static str {
    let (es, en): (&'static str, &'static str) = match key {
        "tray.show" => ("Mostrar Bandwidth Limiter", "Show Bandwidth Limiter"),
        "tray.master" => ("Limitador activo", "Limiter active"),
        "tray.quit" => ("Salir", "Quit"),
        "tray.limiting" => ("aplicando reglas", "applying rules"),
        "err.dll" => ("No se pudo cargar WinDivert.dll", "Could not load WinDivert.dll"),
        "err.sys" => ("No se encontró WinDivert64.sys junto a WinDivert.dll", "WinDivert64.sys not found next to WinDivert.dll"),
        "err.denied" => ("Acceso denegado: la aplicación debe ejecutarse como administrador", "Access denied: the application must run as administrator"),
        "err.filter" => ("Filtro de WinDivert inválido", "Invalid WinDivert filter"),
        "err.signature" => ("Windows rechazó la firma del driver WinDivert64.sys", "Windows rejected the WinDivert64.sys driver signature"),
        "err.version" => ("Versión incompatible del driver WinDivert (¿otra app usa una versión distinta?)", "Incompatible WinDivert driver version (is another app using a different one?)"),
        "err.blocked" => ("El driver fue bloqueado por el sistema (política de drivers)", "The driver was blocked by the system (driver policy)"),
        "err.bfe" => ("El servicio Base Filtering Engine (BFE) no está en ejecución", "The Base Filtering Engine (BFE) service is not running"),
        "err.unknown" => ("Error", "Error"),
        "upd.none" => ("No hay ninguna actualización pendiente", "No update is pending"),
        "upd.fetch" => ("No se pudo leer la lista de versiones (¿sin conexión?)", "Could not read the release list (offline?)"),
        "upd.signature" => ("La firma de la actualización no es válida", "The update signature is not valid"),
        "auto.run" => ("No se pudo ejecutar schtasks", "Could not run schtasks"),
        "auto.create" => ("No se pudo crear la tarea de inicio", "Could not create the startup task"),
        "auto.delete" => ("No se pudo eliminar la tarea de inicio", "Could not remove the startup task"),
        "cfg.save" => ("No se pudo guardar la configuración", "Could not save the configuration"),
        _ => (key_static(key), key_static(key)),
    };
    match lang {
        Lang::Es => es,
        Lang::En => en,
    }
}

fn key_static(_k: &str) -> &'static str {
    "?"
}
