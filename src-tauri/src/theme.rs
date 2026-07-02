use anyhow::Result;
use tauri::{
    image::Image,
    tray::TrayIconBuilder,
    AppHandle, Runtime,
};
use winreg::{enums::*, RegKey};

pub const MAIN_TRAY_ID: &str = "main-tray";

const LIGHT_ICON: &[u8] = include_bytes!("../icons/tray-light.ico");
const DARK_ICON: &[u8] = include_bytes!("../icons/tray-dark.ico");

pub fn build_tray<R: Runtime>(app: &AppHandle<R>) -> Result<()> {
    TrayIconBuilder::with_id(MAIN_TRAY_ID)
        .icon(current_tray_icon()?)
        .build(app)?;
    Ok(())
}

pub fn spawn_theme_watcher<R: Runtime>(app: AppHandle<R>) {
    std::thread::spawn(move || {
        let mut last_theme = is_system_light_theme();

        loop {
            std::thread::sleep(std::time::Duration::from_secs(3));

            let current_theme = is_system_light_theme();
            if current_theme == last_theme {
                continue;
            }

            last_theme = current_theme;
            if let Some(tray) = app.tray_by_id(MAIN_TRAY_ID) {
                match tray_icon(current_theme) {
                    Ok(icon) => {
                        if let Err(error) = tray.set_icon(Some(icon)) {
                            log::warn!("failed to update tray icon: {error}");
                        }
                    }
                    Err(error) => log::warn!("failed to load tray icon: {error}"),
                }
            }
        }
    });
}

pub fn is_system_light_theme() -> bool {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let value = hkcu
        .open_subkey_with_flags(
            "Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize",
            KEY_READ,
        )
        .ok()
        .and_then(|key| key.get_value::<u32, _>("SystemUsesLightTheme").ok());

    is_light_theme_value(value)
}

pub fn current_tray_icon() -> Result<Image<'static>> {
    tray_icon(is_system_light_theme())
}

pub fn tray_icon(light: bool) -> Result<Image<'static>> {
    Ok(Image::from_bytes(if light { LIGHT_ICON } else { DARK_ICON })?.to_owned())
}

fn is_light_theme_value(value: Option<u32>) -> bool {
    value.map(|value| value != 0).unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_registry_value_to_theme() {
        assert!(is_light_theme_value(Some(1)));
        assert!(!is_light_theme_value(Some(0)));
        assert!(!is_light_theme_value(None));
    }

    #[test]
    fn bundled_icons_are_loadable() {
        assert!(tray_icon(true).is_ok());
        assert!(tray_icon(false).is_ok());
    }
}
