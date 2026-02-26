use super::types::{DateOffset, SlashAction, SlashCommand};

pub(super) const CMD: SlashCommand = SlashCommand {
    name: "yesterday",
    description: "Insert yesterday's date",
    action: SlashAction::InsertDate(DateOffset::Yesterday),
};
