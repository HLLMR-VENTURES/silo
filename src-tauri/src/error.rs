//! Errors the frontend can tell apart. A command returning `Result<T, AppError>` rejects with
//! `{ code, message }`: `message` is for the user, `code` is for code. Commands still returning
//! `Result<T, String>` reject with the bare string; the frontend accepts both (src/lib/errors.ts).
//!
//! Convention for new code: a command whose failure the UI must branch on returns `AppError`
//! with a code from `code`; never match on message text, in Rust or TypeScript.

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct AppError {
    pub code: &'static str,
    pub message: String,
}

pub mod code {
    pub const STILL_LOADING: &str = "STILL_LOADING";
    pub const LIBRARY_BUSY: &str = "LIBRARY_BUSY";
    pub const GAME_RUNNING: &str = "GAME_RUNNING";
    pub const OTHER: &str = "OTHER";
}

impl AppError {
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        AppError {
            code,
            message: message.into(),
        }
    }
}

impl From<String> for AppError {
    fn from(message: String) -> Self {
        AppError::new(code::OTHER, message)
    }
}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn errors_reach_the_webview_as_code_and_message() {
        let e = AppError::new(code::STILL_LOADING, "Silo is still loading");
        assert_eq!(
            serde_json::to_value(&e).unwrap(),
            serde_json::json!({ "code": "STILL_LOADING", "message": "Silo is still loading" })
        );
        let plain: AppError = String::from("boom").into();
        assert_eq!((plain.code, plain.message.as_str()), (code::OTHER, "boom"));
    }
}
