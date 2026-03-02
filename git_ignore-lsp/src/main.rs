//! git_ignore-lsp — a tiny Language Server that sorts .gitignore files.
//!
//! Capabilities exposed:
//!   • textDocument/formatting   – sort the whole file
//!   • textDocument/codeAction   – "Sort .gitignore" action (triggers formatting)
//!
//! The server speaks LSP over stdin/stdout.

use std::collections::HashMap;
use std::sync::Arc;

use dashmap::DashMap;

use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};

// ---------------------------------------------------------------------------
// Sorting logic (identical to the extension, kept self-contained)
// ---------------------------------------------------------------------------

#[derive(Debug)]
struct Block {
    header_comments: Vec<String>,
    patterns: Vec<String>,
    trailing_blanks: Vec<String>,
}

fn sort_block_patterns(patterns: Vec<String>) -> Vec<String> {
    let mut groups: Vec<Vec<String>> = Vec::new();
    let mut current_group: Vec<String> = Vec::new();

    for line in patterns {
        let trimmed = line.trim();
        if trimmed.starts_with('#') {
            current_group.push(line);
        } else {
            current_group.push(line);
            groups.push(current_group);
            current_group = Vec::new();
        }
    }
    if !current_group.is_empty() {
        groups.push(current_group);
    }

    groups.sort_by(|a, b| {
        let key_a = a
            .iter()
            .find(|l| !l.trim().starts_with('#') && !l.trim().is_empty())
            .map(|l| l.trim().trim_start_matches('!').to_ascii_lowercase())
            .unwrap_or_default();
        let key_b = b
            .iter()
            .find(|l| !l.trim().starts_with('#') && !l.trim().is_empty())
            .map(|l| l.trim().trim_start_matches('!').to_ascii_lowercase())
            .unwrap_or_default();
        key_a.cmp(&key_b)
    });

    groups.into_iter().flatten().collect()
}

fn parse_blocks(lines: &[&str]) -> Vec<Block> {
    let mut blocks: Vec<Block> = Vec::new();
    let mut i = 0;

    while i < lines.len() {
        let mut header_comments: Vec<String> = Vec::new();
        let mut patterns: Vec<String> = Vec::new();
        let mut trailing_blanks: Vec<String> = Vec::new();

        let header_start = i;
        while i < lines.len() && lines[i].trim().starts_with('#') {
            i += 1;
        }
        let header_end = i;

        if i >= lines.len() || lines[i].trim().is_empty() {
            for line in &lines[header_start..header_end] {
                patterns.push(line.to_string());
            }
        } else {
            for line in &lines[header_start..header_end] {
                header_comments.push(line.to_string());
            }
            while i < lines.len() && !lines[i].trim().is_empty() {
                patterns.push(lines[i].to_string());
                i += 1;
            }
        }

        while i < lines.len() && lines[i].trim().is_empty() {
            trailing_blanks.push(lines[i].to_string());
            i += 1;
        }

        if !header_comments.is_empty() || !patterns.is_empty() || !trailing_blanks.is_empty() {
            blocks.push(Block {
                header_comments,
                patterns,
                trailing_blanks,
            });
        }
    }

    blocks
}

fn sort_git_ignore(content: &str) -> String {
    let raw_lines: Vec<&str> = content.lines().collect();
    let blocks = parse_blocks(&raw_lines);

    let mut output_lines: Vec<String> = Vec::new();

    for block in blocks {
        for line in block.header_comments {
            output_lines.push(line);
        }
        let sorted = sort_block_patterns(block.patterns);
        for line in sorted {
            output_lines.push(line);
        }
        for line in block.trailing_blanks {
            output_lines.push(line);
        }
    }

    let mut result = output_lines.join("\n");
    if content.ends_with('\n') && !result.ends_with('\n') {
        result.push('\n');
    }
    result
}

// ---------------------------------------------------------------------------
// LSP backend
// ---------------------------------------------------------------------------

struct Backend {
    client: Client,
    /// uri → text of open documents
    documents: Arc<DashMap<Url, String>>,
}

impl Backend {
    fn new(client: Client) -> Self {
        Self {
            client,
            documents: Arc::new(DashMap::new()),
        }
    }

    /// Build a whole-document TextEdit that replaces the content with the
    /// sorted version. Returns `None` when the file is already sorted.
    fn formatting_edits(&self, uri: &Url) -> Option<Vec<TextEdit>> {
        let text = self.documents.get(uri)?.clone();
        let sorted = sort_git_ignore(&text);
        if sorted == text {
            return Some(vec![]); // already sorted — nothing to do
        }

        // The replacement range spans the whole document.
        let line_count = text.lines().count() as u32;
        let last_line_len = text.lines().last().map(|l: &str| l.len()).unwrap_or(0) as u32;

        // If the file ends with a newline the final "line" is empty; LSP
        // clients expect the end position to be just after the last real
        // character, so we use the line *after* the last newline-terminated
        // line with character 0 — which is equivalent to the end of file.
        let end = if text.ends_with('\n') {
            Position {
                line: line_count,
                character: 0,
            }
        } else {
            Position {
                line: line_count.saturating_sub(1),
                character: last_line_len,
            }
        };

        Some(vec![TextEdit {
            range: Range {
                start: Position {
                    line: 0,
                    character: 0,
                },
                end,
            },
            new_text: sorted,
        }])
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, _params: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                // Receive the full text on every change.
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                // Advertise formatting support.
                document_formatting_provider: Some(OneOf::Left(true)),
                // Advertise code-action support.
                code_action_provider: Some(CodeActionProviderCapability::Simple(true)),
                ..Default::default()
            },
            server_info: Some(ServerInfo {
                name: "gitignore-lsp".to_string(),
                version: Some(env!("CARGO_PKG_VERSION").to_string()),
            }),
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "gitignore-lsp initialized")
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    // -----------------------------------------------------------------------
    // Document sync
    // -----------------------------------------------------------------------

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        self.documents
            .insert(params.text_document.uri, params.text_document.text);
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        // We asked for FULL sync so there is always exactly one content change.
        if let Some(change) = params.content_changes.into_iter().last() {
            self.documents
                .insert(params.text_document.uri, change.text);
        }
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        self.documents.remove(&params.text_document.uri);
    }

    // -----------------------------------------------------------------------
    // Formatting
    // -----------------------------------------------------------------------

    async fn formatting(&self, params: DocumentFormattingParams) -> Result<Option<Vec<TextEdit>>> {
        Ok(self.formatting_edits(&params.text_document.uri))
    }

    // -----------------------------------------------------------------------
    // Code actions
    // -----------------------------------------------------------------------

    async fn code_action(&self, params: CodeActionParams) -> Result<Option<CodeActionResponse>> {
        // Only offer the action when the document is open and unsorted.
        let edits = match self.formatting_edits(&params.text_document.uri) {
            Some(e) if !e.is_empty() => e,
            _ => return Ok(None),
        };

        let mut changes: HashMap<Url, Vec<TextEdit>> = HashMap::new();
        changes.insert(params.text_document.uri.clone(), edits);

        let action = CodeAction {
            title: "Sort .gitignore".to_string(),
            kind: Some(CodeActionKind::SOURCE_FIX_ALL),
            edit: Some(WorkspaceEdit {
                changes: Some(changes),
                ..Default::default()
            }),
            is_preferred: Some(true),
            ..Default::default()
        };

        Ok(Some(vec![CodeActionOrCommand::CodeAction(action)]))
    }
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

#[tokio::main]
async fn main() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::build(Backend::new).finish();
    Server::new(stdin, stdout, socket).serve(service).await;
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sort_simple_patterns() {
        let input = "vendor/\nnode_modules/\n.DS_Store\n";
        let output = sort_git_ignore(input);
        assert_eq!(output, ".DS_Store\nnode_modules/\nvendor/\n");
    }

    #[test]
    fn test_sort_preserves_sections() {
        let input = "\
# Build outputs
dist/
build/
target/

# Dependencies
vendor/
node_modules/
";
        let output = sort_git_ignore(input);
        let expected = "\
# Build outputs
build/
dist/
target/

# Dependencies
node_modules/
vendor/
";
        assert_eq!(output, expected);
    }

    #[test]
    fn test_negation_sort_key() {
        // !important should sort as "important", not by "!"
        let input = "zebra\n!important\napple\n";
        let output = sort_git_ignore(input);
        assert_eq!(output, "apple\n!important\nzebra\n");
    }

    #[test]
    fn test_case_insensitive() {
        let input = "Zebra\napple\nBanana\n";
        let output = sort_git_ignore(input);
        assert_eq!(output, "apple\nBanana\nZebra\n");
    }

    #[test]
    fn test_trailing_newline_preserved() {
        let input = "b\na\n";
        let output = sort_git_ignore(input);
        assert!(output.ends_with('\n'));
    }

    #[test]
    fn test_no_trailing_newline_preserved() {
        let input = "b\na";
        let output = sort_git_ignore(input);
        assert!(!output.ends_with('\n'));
    }

    #[test]
    fn test_blank_lines_as_separators() {
        let input = "c\nb\n\nz\na\n";
        let output = sort_git_ignore(input);
        assert_eq!(output, "b\nc\n\na\nz\n");
    }

    #[test]
    fn test_standalone_comment_preserved() {
        let input = "# This is a standalone comment\n\nb\na\n";
        let output = sort_git_ignore(input);
        assert_eq!(output, "# This is a standalone comment\n\na\nb\n");
    }

    #[test]
    fn test_already_sorted_returns_identical() {
        let input = ".DS_Store\nnode_modules/\nvendor/\n";
        let output = sort_git_ignore(input);
        assert_eq!(output, input);
    }

    #[test]
    fn test_inline_comment_attached_to_pattern() {
        // A comment immediately followed (no blank line) by patterns is a
        // section header and travels with those patterns as a unit.
        let input = "# Section\nzebra\napple\n";
        let output = sort_git_ignore(input);
        assert_eq!(output, "# Section\napple\nzebra\n");
    }
}
