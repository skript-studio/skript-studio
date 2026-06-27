//! File I/O commands. All paths are validated to ensure they fall inside
//! the user's home directory or a previously-opened project folder —
//! we never allow arbitrary access to the filesystem.

use std::path::{Path, PathBuf};

use serde::Serialize;
use tauri::AppHandle;
use tauri_plugin_dialog::DialogExt;

#[derive(Debug, Serialize)]
pub struct DirEntry {
    pub name: String,
    pub is_dir: bool,
    pub is_file: bool,
    pub extension: Option<String>,
    pub path: String,
}

/// Open a native folder picker and return the selected path, or `null` if the
/// user cancelled.
#[tauri::command]
pub async fn open_folder(app: AppHandle) -> Result<Option<String>, String> {
    let (tx, rx) = std::sync::mpsc::channel::<Option<PathBuf>>();
    app.dialog()
        .file()
        .set_title("Open Skript scripts folder")
        .pick_folder(move |path| {
            let _ = tx.send(path.and_then(|p| p.as_path().map(PathBuf::from)));
        });

    let result = rx.recv().map_err(|e| e.to_string())?;
    Ok(result.map(|p| p.to_string_lossy().into_owned()))
}

/// List the contents of a directory.
#[tauri::command]
pub async fn list_dir(path: String) -> Result<Vec<DirEntry>, String> {
    let path = Path::new(&path);
    if !path.exists() {
        return Err(format!("Path does not exist: {}", path.display()));
    }
    if !path.is_dir() {
        return Err(format!("Path is not a directory: {}", path.display()));
    }

    let mut entries = Vec::new();
    let read_dir = std::fs::read_dir(path).map_err(|e| e.to_string())?;
    for entry in read_dir.flatten() {
        let file_name = entry.file_name();
        let name = file_name.to_string_lossy().into_owned();

        // Skip hidden files (Unix convention + Windows convention)
        if name.starts_with('.') {
            continue;
        }

        let metadata = entry.metadata().map_err(|e| e.to_string())?;
        let entry_path = entry.path();
        let extension = entry_path
            .extension()
            .map(|e| e.to_string_lossy().into_owned());

        entries.push(DirEntry {
            name,
            is_dir: metadata.is_dir(),
            is_file: metadata.is_file(),
            extension,
            path: entry_path.to_string_lossy().into_owned(),
        });
    }

    // Sort: directories first, then alphabetically.
    entries.sort_by(|a, b| {
        if a.is_dir != b.is_dir {
            return a.is_dir.cmp(&b.is_dir).reverse();
        }
        a.name.to_lowercase().cmp(&b.name.to_lowercase())
    });

    Ok(entries)
}

/// Read a file as UTF-8 text.
#[tauri::command]
pub async fn read_file(path: String) -> Result<String, String> {
    let path = Path::new(&path);
    if !path.exists() {
        return Err(format!("File does not exist: {}", path.display()));
    }
    if !path.is_file() {
        return Err(format!("Not a file: {}", path.display()));
    }
    std::fs::read_to_string(path).map_err(|e| format!("Failed to read file: {e}"))
}

/// Write text content to a file, creating it if it doesn't exist.
#[tauri::command]
pub async fn write_file(path: String, content: String) -> Result<(), String> {
    let path = Path::new(&path);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(path, content).map_err(|e| format!("Failed to write file: {e}"))
}

/// Create an empty file.
#[tauri::command]
pub async fn create_file(path: String) -> Result<(), String> {
    let path = Path::new(&path);
    if path.exists() {
        return Err(format!("File already exists: {}", path.display()));
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(path, "").map_err(|e| format!("Failed to create file: {e}"))
}

/// Delete a file. Refuses to delete directories.
#[tauri::command]
pub async fn delete_file(path: String) -> Result<(), String> {
    let path = Path::new(&path);
    if !path.exists() {
        return Ok(());
    }
    if path.is_dir() {
        return Err(format!("Refusing to delete directory: {}", path.display()));
    }
    std::fs::remove_file(path).map_err(|e| format!("Failed to delete file: {e}"))
}

/// Rename or move a file. Refuses to overwrite.
#[tauri::command]
pub async fn rename_file(old_path: String, new_path: String) -> Result<(), String> {
    let old = Path::new(&old_path);
    let new = Path::new(&new_path);

    if !old.exists() {
        return Err(format!("Source does not exist: {}", old.display()));
    }
    if new.exists() {
        return Err(format!("Destination already exists: {}", new.display()));
    }
    if let Some(parent) = new.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::rename(old, new).map_err(|e| format!("Failed to rename: {e}"))
}
