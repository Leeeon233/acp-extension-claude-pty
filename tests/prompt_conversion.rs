use agent_client_protocol::schema::{
    ContentBlock, EmbeddedResource, EmbeddedResourceResource, PromptRequest, ResourceLink,
    SessionId, TextResourceContents,
};
use claude_code_cli_acp::acp::updates::prompt_text;

#[test]
fn prompt_text_formats_resource_links_embedded_text_and_mcp_commands() {
    let request = PromptRequest::new(
        SessionId::new("10000000-0000-4000-8000-000000000001"),
        vec![
            ContentBlock::from("/mcp:filesystem:read_file README.md"),
            ContentBlock::ResourceLink(ResourceLink::new("", "file:///tmp/project/src/lib.rs")),
            ContentBlock::Resource(EmbeddedResource::new(
                EmbeddedResourceResource::TextResourceContents(TextResourceContents::new(
                    "fn main() {}",
                    "file:///tmp/project/src/main.rs",
                )),
            )),
        ],
    );

    let text = prompt_text(&request);

    assert!(text.contains("/filesystem:read_file (MCP) README.md"));
    assert!(text.contains("[@lib.rs](file:///tmp/project/src/lib.rs)"));
    assert!(text.contains("<context ref=\"file:///tmp/project/src/main.rs\">"));
    assert!(text.contains("fn main() {}"));
}
