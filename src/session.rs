//! Names the agent session a notification is about, and for a finished Claude
//! Code turn, says what it did.
//!
//! The title reads `🏁 [claude] - <session name>` (`✋` while blocked), the
//! session name being the pane's terminal title, which Claude Code sets to the
//! session's title. What the turn did is the first line of Claude's closing
//! message, read from the tail of the session transcript.

use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::executable::home_dir;

/// How much of a transcript's end is read; a turn's closing message sits at
/// the end, and transcripts can grow to many megabytes.
const TRANSCRIPT_TAIL_BYTES: u64 = 512 * 1024;

/// Longest summary shown, in characters; a banner shows about two lines.
const SUMMARY_MAX_CHARS: usize = 200;

/// `🏁 [claude] - Fix the parser`. The flag marks a turn that finished, not
/// one that succeeded; `✋` marks an agent waiting for the user.
pub(crate) fn session_title(status: &str, agent: &str, name: &str) -> String {
    let mark = if status == "done" { "🏁" } else { "✋" };
    format!("{mark} [{agent}] - {name}")
}

/// The first line of the closing message of a Claude Code session's last
/// turn, or `None` when the transcript cannot be read or the turn has no text.
pub(crate) fn claude_turn_summary(session_id: &str) -> Option<String> {
    let tail = read_tail(&claude_transcript_path(session_id)?)?;
    closing_line(&tail)
}

/// `~/.claude/projects/<project>/<session id>.jsonl`. The project directory is
/// named after the cwd the session started in, so it is searched for.
fn claude_transcript_path(session_id: &str) -> Option<PathBuf> {
    // The id becomes a file name; anything but a UUID's characters is refused.
    if session_id.is_empty()
        || !session_id
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() || byte == b'-')
    {
        return None;
    }
    let projects = home_dir()?.join(".claude/projects");
    let file_name = format!("{session_id}.jsonl");
    fs::read_dir(projects)
        .ok()?
        .flatten()
        .map(|entry| entry.path().join(&file_name))
        .find(|path| path.is_file())
}

fn read_tail(path: &Path) -> Option<String> {
    let mut file = File::open(path).ok()?;
    let len = file.metadata().ok()?.len();
    let start = len.saturating_sub(TRANSCRIPT_TAIL_BYTES);
    file.seek(SeekFrom::Start(start)).ok()?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).ok()?;
    let mut tail = String::from_utf8_lossy(&bytes).into_owned();
    if start > 0 {
        // The first line was cut by the seek.
        let first_newline = tail.find('\n')?;
        tail.drain(..=first_newline);
    }
    Some(tail)
}

/// The first line of the last turn's closing message, or `None` when the last
/// turn has no text yet: the newest prompt comes after the newest assistant
/// text, so whatever text precedes it belongs to an earlier turn.
fn closing_line(transcript: &str) -> Option<String> {
    for line in transcript.lines().rev() {
        let Ok(row) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        // Subagent rows, and rows the engine injects rather than the user
        // typed (skill bodies, command caveats), are no turn boundary.
        if row["isSidechain"].as_bool() == Some(true) || row["isMeta"].as_bool() == Some(true) {
            continue;
        }
        let content = &row["message"]["content"];
        match row["type"].as_str() {
            Some("assistant") => {
                let text = content
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter(|block| block["type"] == "text")
                    .filter_map(|block| block["text"].as_str())
                    .rfind(|text| !text.trim().is_empty());
                if let Some(text) = text {
                    return first_line(text);
                }
            }
            Some("user") => {
                // Tool results are user rows too; a prompt is text.
                let is_prompt = content.is_string()
                    || content
                        .as_array()
                        .is_some_and(|blocks| blocks.iter().any(|block| block["type"] == "text"));
                if is_prompt {
                    return None;
                }
            }
            _ => {}
        }
    }
    None
}

/// The first line of prose in a Markdown message, without emphasis, code
/// marks or list and heading markers, cut to [`SUMMARY_MAX_CHARS`].
fn first_line(text: &str) -> Option<String> {
    // A line of markers alone, such as a `---` rule, is skipped for the next.
    let plain = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.starts_with("```"))
        .map(|line| {
            line.replace("**", "")
                .replace('`', "")
                .trim_start_matches(['#', '>', '-', '*', '→', ' '])
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
        })
        .find(|plain| !plain.is_empty())?;
    if plain.chars().count() <= SUMMARY_MAX_CHARS {
        return Some(plain);
    }
    let cut: String = plain.chars().take(SUMMARY_MAX_CHARS - 1).collect();
    Some(format!("{}…", cut.trim_end()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(value: Value) -> String {
        value.to_string()
    }

    fn assistant_text(text: &str) -> String {
        row(serde_json::json!({
            "type": "assistant",
            "isSidechain": false,
            "message": {"content": [{"type": "text", "text": text}]}
        }))
    }

    fn prompt(text: &str) -> String {
        row(serde_json::json!({
            "type": "user",
            "isSidechain": false,
            "message": {"content": text}
        }))
    }

    fn tool_result() -> String {
        row(serde_json::json!({
            "type": "user",
            "isSidechain": false,
            "message": {"content": [{"type": "tool_result", "content": "ok"}]}
        }))
    }

    #[test]
    fn reads_the_first_line_of_the_closing_message() {
        let transcript = [
            prompt("fix the parser"),
            assistant_text("Looking at the parser."),
            tool_result(),
            assistant_text("**Fixed the `parser`.** It now skips blank lines.\n\nDetails follow."),
            row(serde_json::json!({"type": "system"})),
        ]
        .join("\n");

        assert_eq!(
            closing_line(&transcript).as_deref(),
            Some("Fixed the parser. It now skips blank lines.")
        );
    }

    #[test]
    fn ignores_text_from_an_earlier_turn() {
        let transcript = [
            assistant_text("Earlier answer."),
            prompt("next task"),
            tool_result(),
        ]
        .join("\n");

        assert_eq!(closing_line(&transcript), None);
    }

    #[test]
    fn skips_subagent_rows_and_unparsable_lines() {
        let sidechain = row(serde_json::json!({
            "type": "assistant",
            "isSidechain": true,
            "message": {"content": [{"type": "text", "text": "Subagent report."}]}
        }));
        let transcript = [
            assistant_text("Main answer."),
            sidechain,
            "{not json".to_string(),
        ]
        .join("\n");

        assert_eq!(closing_line(&transcript).as_deref(), Some("Main answer."));
    }

    #[test]
    fn skips_injected_meta_rows() {
        let meta = row(serde_json::json!({
            "type": "user",
            "isMeta": true,
            "message": {"content": "<command-name>/compact</command-name>"}
        }));
        let transcript = [assistant_text("Final answer."), meta].join("\n");

        assert_eq!(closing_line(&transcript).as_deref(), Some("Final answer."));
    }

    #[test]
    fn titles_by_status_agent_and_session() {
        assert_eq!(
            session_title("done", "claude", "Fix the parser"),
            "🏁 [claude] - Fix the parser"
        );
        assert_eq!(
            session_title("blocked", "codex", "Fix the parser"),
            "✋ [codex] - Fix the parser"
        );
    }

    #[test]
    fn strips_markers_and_cuts_long_lines() {
        assert_eq!(first_line("```\ncode\n```").as_deref(), Some("code"));
        assert_eq!(first_line("## Heading").as_deref(), Some("Heading"));
        assert_eq!(
            first_line("**→ Done.** rest").as_deref(),
            Some("Done. rest")
        );
        assert_eq!(first_line("  \n\n"), None);
        assert_eq!(
            first_line("---\n\nReal line.").as_deref(),
            Some("Real line.")
        );
        assert_eq!(
            first_line("***\n# \nAfter rules.").as_deref(),
            Some("After rules.")
        );

        let long = "가".repeat(300);
        let cut = first_line(&long).unwrap();
        assert_eq!(cut.chars().count(), SUMMARY_MAX_CHARS);
        assert!(cut.ends_with('…'));
    }

    #[test]
    fn refuses_session_ids_that_are_not_uuids() {
        assert_eq!(claude_transcript_path("../../etc/passwd"), None);
        assert_eq!(claude_transcript_path(""), None);
    }
}
