use tauri::{AppHandle, Runtime};
use tauri_plugin_dialog::{DialogExt, MessageDialogKind};

pub fn info<R: Runtime>(app: &AppHandle<R>, title: &str, message: &str) {
    show(app, title, message, MessageDialogKind::Info);
}

pub fn warning<R: Runtime>(app: &AppHandle<R>, title: &str, message: &str) {
    show(app, title, message, MessageDialogKind::Warning);
}

fn show<R: Runtime>(app: &AppHandle<R>, title: &str, message: &str, kind: MessageDialogKind) {
    app.dialog()
        .message(message)
        .title(title)
        .kind(kind)
        .show(|_| {});
}
