//! Runner execution facade and submodules.

mod command;
mod json;
mod process;
mod response;
mod runners;
mod stream;

#[cfg(test)]
mod tests;

pub use response::extract_final_assistant_response;
pub use runners::{
    run_claude, run_claude_resume, run_codex, run_codex_resume, run_cursor, run_cursor_resume,
    run_gemini, run_gemini_resume, run_opencode, run_opencode_resume,
};
