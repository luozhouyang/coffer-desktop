//! `.env` import commands. Require an unlocked vault.

use tauri::State;
use zeroize::Zeroizing;

use crate::error::AppError;
use crate::import;
use crate::models::{ImportCommitInput, ImportPreview, ImportResult};
use crate::state::AppState;

/// `import_env`: parses uploaded text and returns a masked preview. The
/// plaintext values stay in a backend session (ten minute TTL).
#[tauri::command]
#[specta::specta]
pub fn import_env(
    state: State<'_, AppState>,
    text: String,
    file_name: Option<String>,
) -> Result<ImportPreview, AppError> {
    let db = state.require_db()?;
    // Own the only plaintext copy in a zeroizing buffer.
    let text = Zeroizing::new(text);
    let mut sessions = state
        .import_sessions
        .lock()
        .map_err(|_| AppError::internal("import sessions unavailable"))?;
    import::begin(&db, &mut sessions, &text, file_name.as_deref())
}

/// `import_commit`: writes the selected entries and consumes the session.
#[tauri::command]
#[specta::specta]
pub fn import_commit(
    state: State<'_, AppState>,
    input: ImportCommitInput,
) -> Result<ImportResult, AppError> {
    let db = state.require_db()?;
    let mut sessions = state
        .import_sessions
        .lock()
        .map_err(|_| AppError::internal("import sessions unavailable"))?;
    import::commit(&db, &mut sessions, &input)
}

/// `import_cancel`: drops a preview and its plaintext values.
#[tauri::command]
#[specta::specta]
pub fn import_cancel(state: State<'_, AppState>, session_id: String) -> Result<(), AppError> {
    let mut sessions = state
        .import_sessions
        .lock()
        .map_err(|_| AppError::internal("import sessions unavailable"))?;
    import::cancel(&mut sessions, &session_id);
    Ok(())
}
