use super::types::{SlashAction, SlashCommand};

pub(super) const CMD: SlashCommand = SlashCommand {
    name: "h1",
    description: "Heading 1",
    action: SlashAction::PrependText("# "),
};
