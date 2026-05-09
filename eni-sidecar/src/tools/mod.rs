//! Tool implementations module — tool trait, dispatcher, and individual tools.
//!
//! The dispatcher routes tool calls to registered implementations, validates
//! arguments against JSON schemas, and returns structured results.

pub mod dispatcher;
pub mod draft_file;
pub mod drafts;
pub mod export_card;
pub mod fetch_wiki_page;
pub mod list_characters;
pub mod persona;
pub mod project;
pub mod read_character;
pub mod search_local;
pub mod search_wiki;
pub mod st_client;
pub mod undo;
pub mod write_character;

pub use dispatcher::{Tool, ToolDispatcher, ToolResult, validate_against_schema};
pub use drafts::CreateWorldDraftTool;
pub use drafts::CreatePostHistoryDraftTool;
pub use drafts::EditWorldDraftTool;
pub use drafts::EditPostHistoryDraftTool;
pub use drafts::FinalizeWorldInfoTool;
pub use drafts::FinalizePostHistoryTool;
pub use export_card::ExportCardTool;
pub use fetch_wiki_page::FetchWikiPageTool;
pub use list_characters::ListCharactersTool;
pub use persona::{ListPersonasTool, ReadPersonaTool, UpdatePersonaTool, CreatePersonaTool};
pub use project::{CreateProjectTool, ManageTasksTool};
pub use read_character::ReadCharacterTool;
pub use search_local::SearchLocalTool;
pub use search_wiki::SearchWikiTool;
pub use st_client::{CharacterData, CharacterSummary, PersonaData, PersonaSummary, StClient};
pub use undo::{ListVersionsTool, UndoChangeTool};
pub use write_character::{UpdateCharacterTool, CreateCharacterTool};
