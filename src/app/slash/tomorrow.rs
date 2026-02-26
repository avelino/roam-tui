use super::types::{DateOffset, SlashAction, SlashCommand};

pub(super) const CMD: SlashCommand = SlashCommand {
    name: "tomorrow",
    description: "Insert tomorrow's date",
    action: SlashAction::InsertDate(DateOffset::Tomorrow),
};
