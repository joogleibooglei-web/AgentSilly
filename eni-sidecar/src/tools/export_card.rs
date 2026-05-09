//! Tool: export_card — assembles and exports a TavernCard V2 JSON file.
//!
//! Reads character data from SillyTavern via the ST REST API, assembles it
//! into the TavernCard V2 JSON structure, and writes the output to a
//! configurable export directory.

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;
use tokio::sync::Mutex;
use tracing::debug;

use super::dispatcher::{validate_against_schema, Tool};
use super::st_client::StClient;

/// Tool that assembles a TavernCard V2 JSON structure from character data
/// and writes it to a configurable export directory.
pub struct ExportCardTool {
    st_client: Arc<Mutex<StClient>>,
    /// Directory where exported cards are written.
    export_dir: PathBuf,
}

impl ExportCardTool {
    /// Create a new `ExportCardTool`.
    ///
    /// `export_dir` is the directory where exported card files will be written.
    /// If `None`, defaults to `./exports/`.
    pub fn new(st_client: Arc<Mutex<StClient>>, export_dir: Option<PathBuf>) -> Self {
        let export_dir = export_dir.unwrap_or_else(|| PathBuf::from("exports"));
        Self {
            st_client,
            export_dir,
        }
    }
}

#[async_trait]
impl Tool for ExportCardTool {
    fn name(&self) -> &str {
        "export_card"
    }

    fn description(&self) -> &str {
        "Export a character as a TavernCard V2 JSON file. Assembles the full character card structure and writes it to the export directory."
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "The character name to export"
                },
                "format": {
                    "type": "string",
                    "description": "Export format: 'json' (default) or 'png' (embed JSON in PNG tEXt chunk)",
                    "enum": ["json", "png"],
                    "default": "json"
                },
                "filename": {
                    "type": "string",
                    "description": "Optional custom filename (without extension). Defaults to character name."
                }
            },
            "required": ["name"]
        })
    }

    fn validate_args(&self, args: &Value) -> Result<()> {
        validate_against_schema(&self.parameters_schema(), args)
    }

    async fn execute(&self, args: Value) -> Result<Value> {
        let name = args["name"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: name"))?;

        let format = args
            .get("format")
            .and_then(|v| v.as_str())
            .unwrap_or("json");

        let filename = args
            .get("filename")
            .and_then(|v| v.as_str())
            .unwrap_or(name);

        debug!(name = %name, format = %format, "Exporting character card");

        // Fetch character data from SillyTavern
        let character = {
            let mut client = self.st_client.lock().await;
            client.get_character(name).await?
        };

        // Assemble TavernCard V2 JSON structure
        let tavern_card = assemble_tavern_card_v2(&character);

        // Ensure export directory exists
        std::fs::create_dir_all(&self.export_dir)
            .map_err(|e| anyhow::anyhow!("Failed to create export directory: {}", e))?;

        match format {
            "json" => {
                let file_path = self.export_dir.join(format!("{}.json", sanitize_filename(filename)));
                let json_str = serde_json::to_string_pretty(&tavern_card)?;
                std::fs::write(&file_path, &json_str)
                    .map_err(|e| anyhow::anyhow!("Failed to write export file: {}", e))?;

                debug!(path = %file_path.display(), "Character card exported as JSON");

                Ok(serde_json::json!({
                    "success": true,
                    "format": "json",
                    "path": file_path.to_string_lossy(),
                    "character": name,
                    "message": format!("Character '{}' exported to {}", name, file_path.display())
                }))
            }
            "png" => {
                // PNG export with embedded JSON in tEXt chunk
                // For now, we export the JSON and note that PNG embedding
                // requires additional image processing dependencies.
                let file_path = self.export_dir.join(format!("{}.json", sanitize_filename(filename)));
                let json_str = serde_json::to_string_pretty(&tavern_card)?;
                std::fs::write(&file_path, &json_str)
                    .map_err(|e| anyhow::anyhow!("Failed to write export file: {}", e))?;

                Ok(serde_json::json!({
                    "success": true,
                    "format": "json",
                    "path": file_path.to_string_lossy(),
                    "character": name,
                    "message": format!(
                        "Character '{}' exported as JSON to {}. \
                         PNG embedding is not yet supported — use the JSON file directly.",
                        name, file_path.display()
                    ),
                    "note": "PNG tEXt chunk embedding requires image processing support. Exported as JSON instead."
                }))
            }
            _ => {
                anyhow::bail!("Unsupported export format: '{}'. Use 'json' or 'png'.", format)
            }
        }
    }
}

/// Assemble a TavernCard V2 JSON structure from character data.
///
/// Follows the TavernCard V2 specification:
/// https://github.com/malfoyslastname/character-card-spec-v2
fn assemble_tavern_card_v2(character: &super::st_client::CharacterData) -> Value {
    serde_json::json!({
        "spec": "chara_card_v2",
        "spec_version": "2.0",
        "data": {
            "name": character.name,
            "description": character.description,
            "personality": character.personality,
            "scenario": character.scenario,
            "first_mes": character.first_mes,
            "mes_example": character.mes_example,
            "creator_notes": character.creator_notes,
            "system_prompt": character.system_prompt,
            "post_history_instructions": character.post_history_instructions,
            "tags": character.tags,
            "creator": "",
            "character_version": "",
            "extensions": {}
        }
    })
}

/// Sanitize a filename by removing/replacing unsafe characters.
fn sanitize_filename(name: &str) -> String {
    name.chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            _ => c,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::st_client::CharacterData;

    #[test]
    fn test_assemble_tavern_card_v2() {
        let character = CharacterData {
            name: "Kael".to_string(),
            description: "A cyberpunk warrior".to_string(),
            personality: "Brave and resourceful".to_string(),
            scenario: "Sector 7, year 2077".to_string(),
            first_mes: "Hey, you looking for trouble?".to_string(),
            mes_example: "{{char}}: *leans against the wall*".to_string(),
            creator_notes: "Created for testing".to_string(),
            system_prompt: "".to_string(),
            post_history_instructions: "".to_string(),
            tags: vec!["cyberpunk".to_string(), "warrior".to_string()],
            avatar: "kael.png".to_string(),
            alternate_greetings: vec![],
            character_book: None,
            extensions: None,
            creator: "".to_string(),
            character_version: "".to_string(),
            talkativeness: None,
        };

        let card = assemble_tavern_card_v2(&character);

        assert_eq!(card["spec"], "chara_card_v2");
        assert_eq!(card["spec_version"], "2.0");
        assert_eq!(card["data"]["name"], "Kael");
        assert_eq!(card["data"]["description"], "A cyberpunk warrior");
        assert_eq!(card["data"]["personality"], "Brave and resourceful");
        assert_eq!(card["data"]["first_mes"], "Hey, you looking for trouble?");
        assert_eq!(card["data"]["tags"][0], "cyberpunk");
        assert_eq!(card["data"]["tags"][1], "warrior");
    }

    #[test]
    fn test_sanitize_filename() {
        assert_eq!(sanitize_filename("normal_name"), "normal_name");
        assert_eq!(sanitize_filename("has/slashes"), "has_slashes");
        assert_eq!(sanitize_filename("has:colons"), "has_colons");
        assert_eq!(sanitize_filename("a*b?c\"d"), "a_b_c_d");
        assert_eq!(sanitize_filename("hello world"), "hello world");
    }

    #[test]
    fn test_schema_validation() {
        // Instead, test schema validation with the raw schema
        let schema = serde_json::json!({
            "type": "object",
            "properties": {
                "name": { "type": "string" },
                "format": { "type": "string", "enum": ["json", "png"] },
                "filename": { "type": "string" }
            },
            "required": ["name"]
        });

        use super::super::dispatcher::validate_against_schema;

        // Valid
        let valid = serde_json::json!({"name": "Kael"});
        assert!(validate_against_schema(&schema, &valid).is_ok());

        // Valid with format
        let valid_fmt = serde_json::json!({"name": "Kael", "format": "json"});
        assert!(validate_against_schema(&schema, &valid_fmt).is_ok());

        // Invalid: missing name
        let invalid = serde_json::json!({"format": "json"});
        assert!(validate_against_schema(&schema, &invalid).is_err());

        // Invalid: bad format enum
        let bad_fmt = serde_json::json!({"name": "Kael", "format": "xml"});
        assert!(validate_against_schema(&schema, &bad_fmt).is_err());
    }
}
