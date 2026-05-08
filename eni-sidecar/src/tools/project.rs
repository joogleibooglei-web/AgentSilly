//! Tools: create_project and manage_tasks — project and task management.
//!
//! `CreateProjectTool` inserts a new project into the projects table.
//! `ManageTasksTool` provides CRUD operations on the tasks table scoped to a project.

use std::sync::{Arc, Mutex};

use anyhow::Result;
use async_trait::async_trait;
use serde_json::Value;
use tracing::debug;

use super::dispatcher::{validate_against_schema, Tool};
use crate::db::Database;

// ---------------------------------------------------------------------------
// CreateProjectTool
// ---------------------------------------------------------------------------

/// Tool that creates a new world-building project.
pub struct CreateProjectTool {
    db: Arc<Mutex<Database>>,
}

impl CreateProjectTool {
    /// Create a new `CreateProjectTool`.
    pub fn new(db: Arc<Mutex<Database>>) -> Self {
        Self { db }
    }
}

#[async_trait]
impl Tool for CreateProjectTool {
    fn name(&self) -> &str {
        "create_project"
    }

    fn description(&self) -> &str {
        "Create a new world-building project with a name, description, and optional metadata (genre, setting, tone)"
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "name": {
                    "type": "string",
                    "description": "Project name"
                },
                "description": {
                    "type": "string",
                    "description": "Project description"
                },
                "metadata": {
                    "type": "object",
                    "description": "Optional metadata (genre, setting, tone, etc.)"
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

        let description = args.get("description").and_then(|v| v.as_str()).unwrap_or("");
        let metadata = args.get("metadata").map(|v| serde_json::to_string(v).unwrap_or_default());

        let id = uuid::Uuid::new_v4().to_string();

        {
            let db = self.db.lock().map_err(|e| anyhow::anyhow!("Database lock poisoned: {}", e))?;
            db.conn().execute(
                "INSERT INTO projects (id, name, description, metadata) VALUES (?1, ?2, ?3, ?4)",
                rusqlite::params![id, name, description, metadata],
            )?;
        }

        debug!(id = %id, name = %name, "Project created");

        Ok(serde_json::json!({
            "success": true,
            "id": id,
            "name": name,
            "message": format!("Project '{}' created", name)
        }))
    }
}

// ---------------------------------------------------------------------------
// ManageTasksTool
// ---------------------------------------------------------------------------

/// Tool that provides CRUD operations on tasks within a project.
///
/// Supports actions: create, update_status, list, delete.
pub struct ManageTasksTool {
    db: Arc<Mutex<Database>>,
}

impl ManageTasksTool {
    /// Create a new `ManageTasksTool`.
    pub fn new(db: Arc<Mutex<Database>>) -> Self {
        Self { db }
    }
}

#[async_trait]
impl Tool for ManageTasksTool {
    fn name(&self) -> &str {
        "manage_tasks"
    }

    fn description(&self) -> &str {
        "Create, update, list, or delete tasks within a project. Actions: create, update_status, list, delete."
    }

    fn parameters_schema(&self) -> Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "action": {
                    "type": "string",
                    "description": "The action to perform",
                    "enum": ["create", "update_status", "list", "delete"]
                },
                "project_id": {
                    "type": "string",
                    "description": "The project ID to scope tasks to"
                },
                "task_id": {
                    "type": "string",
                    "description": "Task ID (required for update_status and delete)"
                },
                "title": {
                    "type": "string",
                    "description": "Task title (required for create)"
                },
                "description": {
                    "type": "string",
                    "description": "Task description (optional for create)"
                },
                "status": {
                    "type": "string",
                    "description": "Task status (for update_status): planned, in_progress, complete",
                    "enum": ["planned", "in_progress", "complete"]
                }
            },
            "required": ["action", "project_id"]
        })
    }

    fn validate_args(&self, args: &Value) -> Result<()> {
        validate_against_schema(&self.parameters_schema(), args)?;

        let action = args["action"].as_str().unwrap_or("");

        match action {
            "create" => {
                if args.get("title").and_then(|v| v.as_str()).is_none() {
                    anyhow::bail!("'title' is required for the 'create' action");
                }
            }
            "update_status" => {
                if args.get("task_id").and_then(|v| v.as_str()).is_none() {
                    anyhow::bail!("'task_id' is required for the 'update_status' action");
                }
                if args.get("status").and_then(|v| v.as_str()).is_none() {
                    anyhow::bail!("'status' is required for the 'update_status' action");
                }
            }
            "delete" => {
                if args.get("task_id").and_then(|v| v.as_str()).is_none() {
                    anyhow::bail!("'task_id' is required for the 'delete' action");
                }
            }
            "list" => {} // No additional params needed
            _ => anyhow::bail!("Invalid action: '{}'. Must be one of: create, update_status, list, delete", action),
        }

        Ok(())
    }

    async fn execute(&self, args: Value) -> Result<Value> {
        let action = args["action"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: action"))?;

        let project_id = args["project_id"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Missing required parameter: project_id"))?;

        match action {
            "create" => self.create_task(project_id, &args).await,
            "update_status" => self.update_task_status(project_id, &args).await,
            "list" => self.list_tasks(project_id).await,
            "delete" => self.delete_task(project_id, &args).await,
            _ => anyhow::bail!("Invalid action: '{}'", action),
        }
    }
}

impl ManageTasksTool {
    async fn create_task(&self, project_id: &str, args: &Value) -> Result<Value> {
        let title = args["title"].as_str().unwrap(); // validated
        let description = args.get("description").and_then(|v| v.as_str()).unwrap_or("");
        let id = uuid::Uuid::new_v4().to_string();

        {
            let db = self.db.lock().map_err(|e| anyhow::anyhow!("Database lock poisoned: {}", e))?;

            // Verify project exists
            let project_exists: bool = db.conn().query_row(
                "SELECT COUNT(*) > 0 FROM projects WHERE id = ?1",
                rusqlite::params![project_id],
                |row| row.get(0),
            )?;

            if !project_exists {
                anyhow::bail!("Project not found: {}", project_id);
            }

            db.conn().execute(
                "INSERT INTO tasks (id, project_id, title, description, status) VALUES (?1, ?2, ?3, ?4, 'planned')",
                rusqlite::params![id, project_id, title, description],
            )?;
        }

        debug!(id = %id, project_id = %project_id, title = %title, "Task created");

        Ok(serde_json::json!({
            "success": true,
            "id": id,
            "project_id": project_id,
            "title": title,
            "status": "planned",
            "message": format!("Task '{}' created", title)
        }))
    }

    async fn update_task_status(&self, project_id: &str, args: &Value) -> Result<Value> {
        let task_id = args["task_id"].as_str().unwrap(); // validated
        let status = args["status"].as_str().unwrap(); // validated

        let rows_affected = {
            let db = self.db.lock().map_err(|e| anyhow::anyhow!("Database lock poisoned: {}", e))?;
            db.conn().execute(
                "UPDATE tasks SET status = ?1 WHERE id = ?2 AND project_id = ?3",
                rusqlite::params![status, task_id, project_id],
            )?
        };

        if rows_affected == 0 {
            anyhow::bail!("Task not found: {} in project {}", task_id, project_id);
        }

        debug!(task_id = %task_id, status = %status, "Task status updated");

        Ok(serde_json::json!({
            "success": true,
            "task_id": task_id,
            "status": status,
            "message": format!("Task status updated to '{}'", status)
        }))
    }

    async fn list_tasks(&self, project_id: &str) -> Result<Value> {
        let db = self.db.lock().map_err(|e| anyhow::anyhow!("Database lock poisoned: {}", e))?;

        let mut stmt = db.conn().prepare(
            "SELECT id, title, description, status, created_at FROM tasks WHERE project_id = ?1 ORDER BY created_at",
        )?;

        let tasks: Vec<Value> = stmt
            .query_map(rusqlite::params![project_id], |row| {
                Ok(serde_json::json!({
                    "id": row.get::<_, String>(0)?,
                    "title": row.get::<_, String>(1)?,
                    "description": row.get::<_, Option<String>>(2)?,
                    "status": row.get::<_, String>(3)?,
                    "created_at": row.get::<_, String>(4)?,
                }))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(serde_json::json!({
            "project_id": project_id,
            "tasks": tasks,
            "total": tasks.len()
        }))
    }

    async fn delete_task(&self, project_id: &str, args: &Value) -> Result<Value> {
        let task_id = args["task_id"].as_str().unwrap(); // validated

        let rows_affected = {
            let db = self.db.lock().map_err(|e| anyhow::anyhow!("Database lock poisoned: {}", e))?;
            db.conn().execute(
                "DELETE FROM tasks WHERE id = ?1 AND project_id = ?2",
                rusqlite::params![task_id, project_id],
            )?
        };

        if rows_affected == 0 {
            anyhow::bail!("Task not found: {} in project {}", task_id, project_id);
        }

        debug!(task_id = %task_id, "Task deleted");

        Ok(serde_json::json!({
            "success": true,
            "task_id": task_id,
            "message": "Task deleted"
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_db() -> Arc<Mutex<Database>> {
        let db = Database::open(":memory:").unwrap();
        Arc::new(Mutex::new(db))
    }

    // --- CreateProjectTool tests ---

    #[test]
    fn test_create_project_schema_validation() {
        let db = setup_db();
        let tool = CreateProjectTool::new(db);

        // Valid: has name
        let valid = serde_json::json!({"name": "Neon Veins"});
        assert!(tool.validate_args(&valid).is_ok());

        // Valid: with description and metadata
        let valid_full = serde_json::json!({
            "name": "Neon Veins",
            "description": "A cyberpunk world",
            "metadata": {"genre": "cyberpunk", "tone": "dark"}
        });
        assert!(tool.validate_args(&valid_full).is_ok());

        // Invalid: missing name
        let invalid = serde_json::json!({"description": "A project"});
        assert!(tool.validate_args(&invalid).is_err());
    }

    #[tokio::test]
    async fn test_create_project() {
        let db = setup_db();
        let tool = CreateProjectTool::new(db.clone());

        let args = serde_json::json!({
            "name": "Neon Veins",
            "description": "A cyberpunk world-building project",
            "metadata": {"genre": "cyberpunk"}
        });

        let result = tool.execute(args).await.unwrap();
        assert_eq!(result["success"], true);
        assert_eq!(result["name"], "Neon Veins");
        assert!(result["id"].as_str().is_some());

        // Verify in database
        let db_lock = db.lock().unwrap();
        let name: String = db_lock.conn().query_row(
            "SELECT name FROM projects WHERE id = ?1",
            rusqlite::params![result["id"].as_str().unwrap()],
            |row| row.get(0),
        ).unwrap();
        assert_eq!(name, "Neon Veins");
    }

    // --- ManageTasksTool tests ---

    #[test]
    fn test_manage_tasks_schema_validation() {
        let db = setup_db();
        let tool = ManageTasksTool::new(db);

        // Valid: create
        let valid_create = serde_json::json!({
            "action": "create",
            "project_id": "proj-1",
            "title": "Build characters"
        });
        assert!(tool.validate_args(&valid_create).is_ok());

        // Valid: list
        let valid_list = serde_json::json!({
            "action": "list",
            "project_id": "proj-1"
        });
        assert!(tool.validate_args(&valid_list).is_ok());

        // Valid: update_status
        let valid_update = serde_json::json!({
            "action": "update_status",
            "project_id": "proj-1",
            "task_id": "task-1",
            "status": "complete"
        });
        assert!(tool.validate_args(&valid_update).is_ok());

        // Valid: delete
        let valid_delete = serde_json::json!({
            "action": "delete",
            "project_id": "proj-1",
            "task_id": "task-1"
        });
        assert!(tool.validate_args(&valid_delete).is_ok());

        // Invalid: create without title
        let invalid_create = serde_json::json!({
            "action": "create",
            "project_id": "proj-1"
        });
        assert!(tool.validate_args(&invalid_create).is_err());

        // Invalid: update_status without task_id
        let invalid_update = serde_json::json!({
            "action": "update_status",
            "project_id": "proj-1",
            "status": "complete"
        });
        assert!(tool.validate_args(&invalid_update).is_err());

        // Invalid: missing action
        let no_action = serde_json::json!({"project_id": "proj-1"});
        assert!(tool.validate_args(&no_action).is_err());
    }

    #[tokio::test]
    async fn test_create_and_list_tasks() {
        let db = setup_db();

        // First create a project
        let create_project = CreateProjectTool::new(db.clone());
        let project_result = create_project.execute(serde_json::json!({
            "name": "Test Project"
        })).await.unwrap();
        let project_id = project_result["id"].as_str().unwrap().to_string();

        // Create tasks
        let tool = ManageTasksTool::new(db.clone());

        let task1 = tool.execute(serde_json::json!({
            "action": "create",
            "project_id": project_id,
            "title": "Build characters",
            "description": "Create 3 main characters"
        })).await.unwrap();
        assert_eq!(task1["success"], true);
        assert_eq!(task1["status"], "planned");

        let task2 = tool.execute(serde_json::json!({
            "action": "create",
            "project_id": project_id,
            "title": "Write lore"
        })).await.unwrap();
        assert_eq!(task2["success"], true);

        // List tasks
        let list_result = tool.execute(serde_json::json!({
            "action": "list",
            "project_id": project_id
        })).await.unwrap();
        assert_eq!(list_result["total"], 2);
        let tasks = list_result["tasks"].as_array().unwrap();
        assert_eq!(tasks[0]["title"], "Build characters");
        assert_eq!(tasks[1]["title"], "Write lore");
    }

    #[tokio::test]
    async fn test_update_task_status() {
        let db = setup_db();

        let create_project = CreateProjectTool::new(db.clone());
        let project_result = create_project.execute(serde_json::json!({
            "name": "Test Project"
        })).await.unwrap();
        let project_id = project_result["id"].as_str().unwrap().to_string();

        let tool = ManageTasksTool::new(db.clone());

        // Create a task
        let task_result = tool.execute(serde_json::json!({
            "action": "create",
            "project_id": project_id,
            "title": "Build characters"
        })).await.unwrap();
        let task_id = task_result["id"].as_str().unwrap().to_string();

        // Update status
        let update_result = tool.execute(serde_json::json!({
            "action": "update_status",
            "project_id": project_id,
            "task_id": task_id,
            "status": "in_progress"
        })).await.unwrap();
        assert_eq!(update_result["success"], true);
        assert_eq!(update_result["status"], "in_progress");

        // Verify via list
        let list_result = tool.execute(serde_json::json!({
            "action": "list",
            "project_id": project_id
        })).await.unwrap();
        assert_eq!(list_result["tasks"][0]["status"], "in_progress");
    }

    #[tokio::test]
    async fn test_delete_task() {
        let db = setup_db();

        let create_project = CreateProjectTool::new(db.clone());
        let project_result = create_project.execute(serde_json::json!({
            "name": "Test Project"
        })).await.unwrap();
        let project_id = project_result["id"].as_str().unwrap().to_string();

        let tool = ManageTasksTool::new(db.clone());

        // Create a task
        let task_result = tool.execute(serde_json::json!({
            "action": "create",
            "project_id": project_id,
            "title": "To be deleted"
        })).await.unwrap();
        let task_id = task_result["id"].as_str().unwrap().to_string();

        // Delete it
        let delete_result = tool.execute(serde_json::json!({
            "action": "delete",
            "project_id": project_id,
            "task_id": task_id
        })).await.unwrap();
        assert_eq!(delete_result["success"], true);

        // Verify it's gone
        let list_result = tool.execute(serde_json::json!({
            "action": "list",
            "project_id": project_id
        })).await.unwrap();
        assert_eq!(list_result["total"], 0);
    }

    #[tokio::test]
    async fn test_create_task_nonexistent_project() {
        let db = setup_db();
        let tool = ManageTasksTool::new(db);

        let result = tool.execute(serde_json::json!({
            "action": "create",
            "project_id": "nonexistent",
            "title": "A task"
        })).await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[tokio::test]
    async fn test_update_nonexistent_task() {
        let db = setup_db();

        let create_project = CreateProjectTool::new(db.clone());
        let project_result = create_project.execute(serde_json::json!({
            "name": "Test Project"
        })).await.unwrap();
        let project_id = project_result["id"].as_str().unwrap().to_string();

        let tool = ManageTasksTool::new(db);

        let result = tool.execute(serde_json::json!({
            "action": "update_status",
            "project_id": project_id,
            "task_id": "nonexistent",
            "status": "complete"
        })).await;

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }
}
