//! Template commands: thin adapters over `core::templates`.

use crate::core::templates::{self, Created, TemplateInfo, TemplateList};

#[tauri::command]
pub fn templates_list() -> TemplateList {
    templates::list()
}

/// Make `parent/name` from a template.
#[tauri::command]
pub fn template_instantiate(
    template: String,
    parent: String,
    name: String,
) -> Result<Created, String> {
    templates::instantiate(&template, &parent, &name)
}

/// Save a granted project as a user template.
#[tauri::command]
pub fn template_save_project(root_id: String, info: TemplateInfo) -> Result<TemplateInfo, String> {
    templates::save_project(&root_id, info)
}

/// Import a folder as a user template.
#[tauri::command]
pub fn template_import_folder(dir: String, info: TemplateInfo) -> Result<TemplateInfo, String> {
    templates::import_folder(&dir, info)
}

/// The welcome project, written into app data on first use.
#[tauri::command]
pub fn template_welcome() -> Result<Created, String> {
    templates::welcome()
}
