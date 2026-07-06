use crate::quick_access::{self, QaCounts, QaItem};

#[tauri::command]
pub(crate) fn list_qa_items(qa_type: String) -> Result<Vec<QaItem>, String> {
    quick_access::list_items(&qa_type).map_err(|error| error.to_string())
}

#[tauri::command]
pub(crate) fn get_qa_counts() -> Result<QaCounts, String> {
    quick_access::get_counts().map_err(|error| error.to_string())
}
