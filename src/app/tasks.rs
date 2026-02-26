use std::collections::{HashMap, HashSet};

use chrono::{Datelike, NaiveDate};
use tokio::sync::mpsc;

use crate::api::client::RoamClient;
use crate::api::queries;
use crate::api::types::{Block, DailyNote, WriteAction};
use crate::error::ErrorInfo;
use crate::markdown;

use super::state::{AppMessage, AppState};

pub(super) fn spawn_fetch_daily_note(
    client: &RoamClient,
    date: NaiveDate,
    tx: &mpsc::UnboundedSender<AppMessage>,
) {
    let uid = queries::daily_note_uid_for_date(date.month(), date.day(), date.year());
    let (eid, selector) = queries::pull_daily_note(&uid);
    let client_clone = client.clone();
    let tx_clone = tx.clone();
    let uid_clone = uid.clone();
    tokio::spawn(async move {
        match client_clone.pull(eid, &selector).await {
            Ok(resp) => {
                let note = DailyNote::from_pull_response(date, uid_clone, &resp.result);
                let _ = tx_clone.send(AppMessage::DailyNoteLoaded(note));
            }
            Err(e) => {
                let _ = tx_clone.send(AppMessage::ApiError(ErrorInfo::from_roam_error(&e)));
            }
        }
    });
}

pub(super) fn spawn_fetch_page(
    client: &RoamClient,
    title: &str,
    tx: &mpsc::UnboundedSender<AppMessage>,
) {
    let (eid, selector) = queries::pull_page_by_title(title);
    let client_clone = client.clone();
    let tx_clone = tx.clone();
    let title_owned = title.to_string();
    tokio::spawn(async move {
        match client_clone.pull(eid, &selector).await {
            Ok(resp) => {
                // Use a dummy date — the page is not date-based
                let dummy_date = chrono::NaiveDate::from_ymd_opt(2000, 1, 1).unwrap();
                let uid = resp
                    .result
                    .get(":block/uid")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let mut note = DailyNote::from_pull_response(dummy_date, uid, &resp.result);
                // Ensure title is set even if page doesn't exist yet
                if note.title.is_empty() {
                    note.title = title_owned;
                }
                let _ = tx_clone.send(AppMessage::PageLoaded(note));
            }
            Err(e) => {
                let _ = tx_clone.send(AppMessage::ApiError(ErrorInfo::from_roam_error(&e)));
            }
        }
    });
}

pub(super) fn spawn_fetch_linked_refs(
    client: &RoamClient,
    page_title: &str,
    tx: &mpsc::UnboundedSender<AppMessage>,
) {
    let query = queries::linked_refs_query(page_title);
    let client_clone = client.clone();
    let tx_clone = tx.clone();
    let title_owned = page_title.to_string();
    tokio::spawn(async move {
        match client_clone.query(query, vec![]).await {
            Ok(resp) => {
                let groups = crate::api::types::parse_linked_refs(&resp.result, &title_owned);
                let _ = tx_clone.send(AppMessage::LinkedRefsLoaded(title_owned, groups));
            }
            Err(e) => {
                let _ = tx_clone.send(AppMessage::ApiError(ErrorInfo::from_roam_error(&e)));
            }
        }
    });
}

/// Extract all ((uid)) references from block texts that aren't in the local block map.
pub(super) fn collect_unresolved_refs(state: &AppState) -> Vec<String> {
    let local_map = markdown::build_block_text_map(&state.days);
    let mut unresolved = Vec::new();

    for day in &state.days {
        collect_refs_from_blocks(
            &day.blocks,
            &local_map,
            &state.block_ref_cache,
            &state.pending_block_refs,
            &mut unresolved,
        );
    }
    unresolved
}

fn collect_refs_from_blocks(
    blocks: &[Block],
    local_map: &HashMap<String, String>,
    cache: &HashMap<String, String>,
    pending: &HashSet<String>,
    out: &mut Vec<String>,
) {
    for block in blocks {
        extract_uids_from_text(&block.string, local_map, cache, pending, out);
        collect_refs_from_blocks(&block.children, local_map, cache, pending, out);
    }
}

pub(super) fn extract_uids_from_text(
    text: &str,
    local_map: &HashMap<String, String>,
    cache: &HashMap<String, String>,
    pending: &HashSet<String>,
    out: &mut Vec<String>,
) {
    let chars: Vec<char> = text.chars().collect();
    let len = chars.len();
    let mut i = 0;
    while i + 3 < len {
        if chars[i] == '(' && chars[i + 1] == '(' {
            if let Some(end) = chars[i + 2..].iter().position(|c| *c == ')').and_then(|p| {
                let pos = i + 2 + p;
                if pos + 1 < len && chars[pos + 1] == ')' {
                    Some(pos)
                } else {
                    None
                }
            }) {
                let uid: String = chars[i + 2..end].iter().collect();
                if !uid.is_empty()
                    && !local_map.contains_key(&uid)
                    && !cache.contains_key(&uid)
                    && !pending.contains(&uid)
                {
                    out.push(uid);
                }
                i = end + 2;
                continue;
            }
        }
        // Also check embeds: {{embed: ((uid))}}
        if chars[i] == '{' && chars[i + 1] == '{' {
            if let Some(end_brace) = chars[i + 2..]
                .windows(2)
                .position(|w| w[0] == '}' && w[1] == '}')
            {
                let inner: String = chars[i + 2..i + 2 + end_brace].iter().collect();
                if let Some(uid) = extract_embed_uid(&inner) {
                    if !local_map.contains_key(&uid)
                        && !cache.contains_key(&uid)
                        && !pending.contains(&uid)
                    {
                        out.push(uid);
                    }
                }
                i = i + 2 + end_brace + 2;
                continue;
            }
        }
        i += 1;
    }
}

pub(super) fn extract_embed_uid(inner: &str) -> Option<String> {
    let trimmed = inner
        .strip_prefix("[[embed]]:")
        .or_else(|| inner.strip_prefix("embed:"))?
        .trim();
    trimmed
        .strip_prefix("((")
        .and_then(|s| s.strip_suffix("))"))
        .map(|s| s.to_string())
}

pub(super) fn spawn_resolve_block_refs(
    client: &RoamClient,
    uids: Vec<String>,
    state: &mut AppState,
    tx: &mpsc::UnboundedSender<AppMessage>,
) {
    for uid in uids {
        state.pending_block_refs.insert(uid.clone());
        let client = client.clone();
        let tx = tx.clone();
        let uid_clone = uid.clone();
        tokio::spawn(async move {
            let eid = serde_json::Value::String(format!("[:block/uid \"{}\"]", uid_clone));
            let selector = "[:block/string]";
            match client.pull(eid, selector).await {
                Ok(resp) => {
                    let text = resp
                        .result
                        .get(":block/string")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    if !text.is_empty() {
                        let _ = tx.send(AppMessage::BlockRefResolved(uid_clone, text));
                    }
                }
                Err(_) => {
                    // Silently ignore — will just show the UID
                }
            }
        });
    }
}

pub(super) fn spawn_refresh_daily_note(
    client: &RoamClient,
    date: NaiveDate,
    tx: &mpsc::UnboundedSender<AppMessage>,
) {
    let uid = queries::daily_note_uid_for_date(date.month(), date.day(), date.year());
    let (eid, selector) = queries::pull_daily_note(&uid);
    let client_clone = client.clone();
    let tx_clone = tx.clone();
    let uid_clone = uid.clone();
    tokio::spawn(async move {
        match client_clone.pull(eid, &selector).await {
            Ok(resp) => {
                let note = DailyNote::from_pull_response(date, uid_clone, &resp.result);
                let _ = tx_clone.send(AppMessage::RefreshLoaded(note));
            }
            Err(_) => {
                // Refresh errors are silent — don't disturb the user
            }
        }
    });
}

pub(super) fn spawn_write(
    client: &RoamClient,
    action: WriteAction,
    tx: &mpsc::UnboundedSender<AppMessage>,
) {
    let client = client.clone();
    let tx = tx.clone();
    tokio::spawn(async move {
        if let Err(e) = client.write(action).await {
            let _ = tx.send(AppMessage::ApiError(ErrorInfo::Write(e.to_string())));
        }
    });
}
