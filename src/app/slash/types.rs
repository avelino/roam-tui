#[derive(Debug, Clone, PartialEq)]
pub struct SlashMenuState {
    pub query: String,
    pub commands: Vec<SlashCommand>,
    pub selected: usize,
    pub slash_pos: usize, // position of '/' in buffer
}

#[derive(Debug, Clone, PartialEq)]
pub struct SlashCommand {
    pub name: &'static str,
    pub description: &'static str,
    pub action: SlashAction,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SlashAction {
    PrependText(&'static str),
    InsertText(&'static str),
    InsertPair {
        open: &'static str,
        close: &'static str,
    },
    InsertDate(DateOffset),
    InsertTime,
    InsertCodeBlock,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DateOffset {
    Today,
    Yesterday,
    Tomorrow,
}
