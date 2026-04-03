use crate::api::types::Block;

/// Parse a markdown file (exported by blocks_to_markdown) back into a block tree.
/// Format:
///   # Page Title
///
///   - Block content
///     - Child block
///       - Grandchild
///   - Another block
///
/// Returns (title, blocks).
pub fn markdown_to_blocks(md: &str) -> (String, Vec<Block>) {
    let mut lines = md.lines();
    let mut title = String::new();

    // Extract title from first "# Title" line
    for line in lines.by_ref() {
        let trimmed = line.trim();
        if let Some(t) = trimmed.strip_prefix("# ") {
            title = t.to_string();
            break;
        }
    }

    let mut blocks: Vec<Block> = Vec::new();
    let mut stack: Vec<(usize, usize)> = Vec::new(); // (depth, index in parent's children)

    for line in lines {
        if line.trim().is_empty() {
            continue;
        }

        let (depth, content) = parse_list_line(line);
        if depth.is_none() {
            continue;
        }
        let depth = depth.unwrap();

        let block = Block {
            uid: String::new(),
            string: content,
            order: 0,
            children: Vec::new(),
            open: true,
            refs: Vec::new(),
        };

        if depth == 0 {
            let idx = blocks.len();
            blocks.push(block);
            stack.clear();
            stack.push((0, idx));
        } else {
            // Find parent: walk stack back to find depth - 1
            while stack.last().is_some_and(|(d, _)| *d >= depth) {
                stack.pop();
            }

            if stack.last().is_some() {
                let child_idx = insert_child_at(&mut blocks, &stack, block);
                stack.push((depth, child_idx));
            } else {
                let idx = blocks.len();
                blocks.push(block);
                stack.clear();
                stack.push((0, idx));
            }
        }
    }

    set_order_recursive(&mut blocks);
    (title, blocks)
}

fn parse_list_line(line: &str) -> (Option<usize>, String) {
    let without_indent = line.trim_start_matches(' ');
    let indent = line.len() - without_indent.len();

    if let Some(content) = without_indent.strip_prefix("- ") {
        (Some(indent / 2), content.to_string())
    } else if without_indent == "-" {
        (Some(indent / 2), String::new())
    } else {
        (None, String::new())
    }
}

fn insert_child_at(blocks: &mut [Block], stack: &[(usize, usize)], child: Block) -> usize {
    if stack.len() == 1 {
        let parent = &mut blocks[stack[0].1];
        let idx = parent.children.len();
        parent.children.push(child);
        idx
    } else {
        let mut current = &mut blocks[stack[0].1];
        for &(_, idx) in &stack[1..] {
            current = &mut current.children[idx];
        }
        let idx = current.children.len();
        current.children.push(child);
        idx
    }
}

fn set_order_recursive(blocks: &mut [Block]) {
    for (i, block) in blocks.iter_mut().enumerate() {
        block.order = i as i64;
        set_order_recursive(&mut block.children);
    }
}

/// Diff two block trees. `old` has UIDs (from Roam), `new` has no UIDs (from markdown).
/// `parent_uid` is the UID of the parent block (or page UID for top-level).
pub fn diff_blocks(old: &[Block], new: &[Block], parent_uid: &str) -> Vec<BlockDiff> {
    let mut diffs = Vec::new();
    diff_recursive(old, new, parent_uid, &mut diffs);
    diffs
}

#[derive(Debug, PartialEq)]
pub enum BlockDiff {
    Updated {
        uid: String,
        new_content: String,
    },
    Added {
        parent_uid: String,
        order: i64,
        content: String,
        children: Vec<NewBlockTree>,
    },
    Deleted {
        uid: String,
    },
}

/// A new block with its children (for recursive creation).
#[derive(Debug, PartialEq)]
pub struct NewBlockTree {
    pub content: String,
    pub children: Vec<NewBlockTree>,
}

fn diff_recursive(old: &[Block], new: &[Block], parent_uid: &str, diffs: &mut Vec<BlockDiff>) {
    let max = old.len().max(new.len());
    for i in 0..max {
        match (old.get(i), new.get(i)) {
            (Some(o), Some(n)) => {
                if o.string != n.string {
                    diffs.push(BlockDiff::Updated {
                        uid: o.uid.clone(),
                        new_content: n.string.clone(),
                    });
                }
                // Recurse into children with old block's UID as parent
                diff_recursive(&o.children, &n.children, &o.uid, diffs);
            }
            (Some(o), None) => {
                collect_deleted(o, diffs);
            }
            (None, Some(n)) => {
                diffs.push(BlockDiff::Added {
                    parent_uid: parent_uid.to_string(),
                    order: i as i64,
                    content: n.string.clone(),
                    children: collect_new_tree(&n.children),
                });
            }
            (None, None) => break,
        }
    }
}

fn collect_new_tree(blocks: &[Block]) -> Vec<NewBlockTree> {
    blocks
        .iter()
        .map(|b| NewBlockTree {
            content: b.string.clone(),
            children: collect_new_tree(&b.children),
        })
        .collect()
}

fn collect_deleted(block: &Block, diffs: &mut Vec<BlockDiff>) {
    for child in &block.children {
        collect_deleted(child, diffs);
    }
    if !block.uid.is_empty() {
        diffs.push(BlockDiff::Deleted {
            uid: block.uid.clone(),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple_page() {
        let md = "# My Page\n\n- First block\n- Second block\n";
        let (title, blocks) = markdown_to_blocks(md);
        assert_eq!(title, "My Page");
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0].string, "First block");
        assert_eq!(blocks[1].string, "Second block");
    }

    #[test]
    fn parse_nested_blocks() {
        let md = "# Page\n\n- Parent\n  - Child\n    - Grandchild\n- Sibling\n";
        let (_, blocks) = markdown_to_blocks(md);
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0].string, "Parent");
        assert_eq!(blocks[0].children.len(), 1);
        assert_eq!(blocks[0].children[0].string, "Child");
        assert_eq!(blocks[0].children[0].children.len(), 1);
        assert_eq!(blocks[0].children[0].children[0].string, "Grandchild");
        assert_eq!(blocks[1].string, "Sibling");
    }

    #[test]
    fn parse_empty_block() {
        let md = "# Page\n\n-\n- Content\n";
        let (_, blocks) = markdown_to_blocks(md);
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0].string, "");
        assert_eq!(blocks[1].string, "Content");
    }

    #[test]
    fn parse_preserves_roam_syntax() {
        let md = "# Page\n\n- Link to [[Other Page]] and ((block-ref))\n";
        let (_, blocks) = markdown_to_blocks(md);
        assert_eq!(blocks[0].string, "Link to [[Other Page]] and ((block-ref))");
    }

    #[test]
    fn order_is_set_correctly() {
        let md = "# Page\n\n- A\n- B\n- C\n";
        let (_, blocks) = markdown_to_blocks(md);
        assert_eq!(blocks[0].order, 0);
        assert_eq!(blocks[1].order, 1);
        assert_eq!(blocks[2].order, 2);
    }

    #[test]
    fn diff_detects_update() {
        let old = vec![block("b1", "old content")];
        let new = vec![block("", "new content")];
        let diffs = diff_blocks(&old, &new, "page-uid");
        assert_eq!(diffs.len(), 1);
        assert_eq!(
            diffs[0],
            BlockDiff::Updated {
                uid: "b1".into(),
                new_content: "new content".into()
            }
        );
    }

    #[test]
    fn diff_detects_no_change() {
        let old = vec![block("b1", "same")];
        let new = vec![block("", "same")];
        let diffs = diff_blocks(&old, &new, "page-uid");
        assert!(diffs.is_empty());
    }

    #[test]
    fn diff_detects_deletion() {
        let old = vec![block("b1", "keep"), block("b2", "delete me")];
        let new = vec![block("", "keep")];
        let diffs = diff_blocks(&old, &new, "page-uid");
        assert_eq!(diffs.len(), 1);
        assert_eq!(diffs[0], BlockDiff::Deleted { uid: "b2".into() });
    }

    #[test]
    fn diff_detects_addition_with_parent_uid() {
        let old = vec![block("b1", "existing")];
        let new = vec![block("", "existing"), block("", "new block")];
        let diffs = diff_blocks(&old, &new, "page-uid");
        assert_eq!(diffs.len(), 1);
        match &diffs[0] {
            BlockDiff::Added {
                parent_uid,
                order,
                content,
                ..
            } => {
                assert_eq!(parent_uid, "page-uid");
                assert_eq!(*order, 1);
                assert_eq!(content, "new block");
            }
            _ => panic!("expected Added"),
        }
    }

    #[test]
    fn diff_addition_nested_has_parent_uid() {
        let old = vec![Block {
            uid: "parent".into(),
            string: "parent".into(),
            order: 0,
            children: vec![block("c1", "child")],
            open: true,
            refs: vec![],
        }];
        let new = vec![Block {
            uid: String::new(),
            string: "parent".into(),
            order: 0,
            children: vec![block("", "child"), block("", "new child")],
            open: true,
            refs: vec![],
        }];
        let diffs = diff_blocks(&old, &new, "page-uid");
        assert_eq!(diffs.len(), 1);
        match &diffs[0] {
            BlockDiff::Added {
                parent_uid,
                content,
                ..
            } => {
                assert_eq!(parent_uid, "parent");
                assert_eq!(content, "new child");
            }
            _ => panic!("expected Added"),
        }
    }

    #[test]
    fn diff_addition_with_children() {
        let old = vec![block("b1", "existing")];
        let new = vec![
            block("", "existing"),
            Block {
                uid: String::new(),
                string: "new parent".into(),
                order: 1,
                children: vec![block("", "new child")],
                open: true,
                refs: vec![],
            },
        ];
        let diffs = diff_blocks(&old, &new, "page-uid");
        assert_eq!(diffs.len(), 1);
        match &diffs[0] {
            BlockDiff::Added {
                content, children, ..
            } => {
                assert_eq!(content, "new parent");
                assert_eq!(children.len(), 1);
                assert_eq!(children[0].content, "new child");
            }
            _ => panic!("expected Added"),
        }
    }

    #[test]
    fn roundtrip_blocks_to_markdown_and_back() {
        use crate::export::blocks_to_markdown;

        let original = vec![
            Block {
                uid: "b1".into(),
                string: "First".into(),
                order: 0,
                children: vec![Block {
                    uid: "c1".into(),
                    string: "Child".into(),
                    order: 0,
                    children: vec![],
                    open: true,
                    refs: vec![],
                }],
                open: true,
                refs: vec![],
            },
            Block {
                uid: "b2".into(),
                string: "Second".into(),
                order: 1,
                children: vec![],
                open: true,
                refs: vec![],
            },
        ];

        let md = blocks_to_markdown("Test", &original);
        let (title, parsed) = markdown_to_blocks(&md);

        assert_eq!(title, "Test");
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].string, "First");
        assert_eq!(parsed[0].children.len(), 1);
        assert_eq!(parsed[0].children[0].string, "Child");
        assert_eq!(parsed[1].string, "Second");
    }

    fn block(uid: &str, content: &str) -> Block {
        Block {
            uid: uid.into(),
            string: content.into(),
            order: 0,
            children: vec![],
            open: true,
            refs: vec![],
        }
    }
}
