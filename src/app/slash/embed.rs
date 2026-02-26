use super::types::{SlashAction, SlashCommand};

pub(super) const CMD: SlashCommand = SlashCommand {
    name: "embed",
    description: "Embed block or page",
    action: SlashAction::InsertPair {
        open: "{{[[embed]]: ",
        close: "}}",
    },
};
