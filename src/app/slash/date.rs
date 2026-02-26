use super::types::{DateOffset, SlashAction, SlashCommand};

pub(super) const CMD: SlashCommand = SlashCommand {
    name: "date",
    description: "Insert today's date",
    action: SlashAction::InsertDate(DateOffset::Today),
};
