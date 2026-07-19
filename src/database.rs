use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DbEmail {
    pub value: String,
    pub r#type: Option<String>,
    pub primary: bool,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DbUser {
    pub id: String,
    pub external_id: Option<String>,
    pub user_name: String,
    pub formatted_name: Option<String>,
    pub family_name: Option<String>,
    pub given_name: Option<String>,
    pub display_name: Option<String>,
    pub emails: Vec<DbEmail>,
    pub active: bool,
    pub created: DateTime<Utc>,
    pub last_modified: DateTime<Utc>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq)]
pub struct DbGroupMember {
    pub value: String, // User ID
    pub display: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DbGroup {
    pub id: String,
    pub display_name: String,
    pub members: Vec<DbGroupMember>,
    pub created: DateTime<Utc>,
    pub last_modified: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DbError {
    UserAlreadyExists(String),
    UserNotFound(String),
    GroupNotFound(String),
    GroupAlreadyExists(String),
}

impl std::fmt::Display for DbError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DbError::UserAlreadyExists(username) => write!(f, "User with userName '{}' already exists", username),
            DbError::UserNotFound(id) => write!(f, "User not found with ID '{}'", id),
            DbError::GroupNotFound(id) => write!(f, "Group not found with ID '{}'", id),
            DbError::GroupAlreadyExists(name) => write!(f, "Group with displayName '{}' already exists", name),
        }
    }
}

impl std::error::Error for DbError {}

#[derive(Default)]
struct DbState {
    users: HashMap<String, DbUser>,
    groups: HashMap<String, DbGroup>,
}

#[derive(Clone, Default)]
pub struct Database {
    state: Arc<RwLock<DbState>>,
}

impl Database {
    pub fn new() -> Self {
        Self {
            state: Arc::new(RwLock::new(DbState::default())),
        }
    }

    // --- User CRUD ---

    pub async fn create_user(
        &self,
        external_id: Option<String>,
        user_name: String,
        formatted_name: Option<String>,
        family_name: Option<String>,
        given_name: Option<String>,
        display_name: Option<String>,
        emails: Vec<DbEmail>,
        active: bool,
    ) -> Result<DbUser, DbError> {
        let mut state = self.state.write().await;

        // Enforce userName uniqueness
        if state.users.values().any(|u| u.user_name.eq_ignore_ascii_case(&user_name)) {
            return Err(DbError::UserAlreadyExists(user_name));
        }

        let now = Utc::now();
        let user = DbUser {
            id: Uuid::new_v4().to_string(),
            external_id,
            user_name,
            formatted_name,
            family_name,
            given_name,
            display_name,
            emails,
            active,
            created: now,
            last_modified: now,
        };

        state.users.insert(user.id.clone(), user.clone());
        Ok(user)
    }

    pub async fn get_user(&self, id: &str) -> Option<DbUser> {
        let state = self.state.read().await;
        state.users.get(id).cloned()
    }

    pub async fn update_user(
        &self,
        id: &str,
        external_id: Option<String>,
        user_name: String,
        formatted_name: Option<String>,
        family_name: Option<String>,
        given_name: Option<String>,
        display_name: Option<String>,
        emails: Vec<DbEmail>,
        active: bool,
    ) -> Result<DbUser, DbError> {
        let mut state = self.state.write().await;

        // Check if user exists
        let existing = state.users.get(id).ok_or_else(|| DbError::UserNotFound(id.to_string()))?;

        // Check if userName changes and conflicts with another user
        if !existing.user_name.eq_ignore_ascii_case(&user_name) 
            && state.users.values().any(|u| u.id != id && u.user_name.eq_ignore_ascii_case(&user_name)) 
        {
            return Err(DbError::UserAlreadyExists(user_name));
        }

        let now = Utc::now();
        let updated_user = DbUser {
            id: id.to_string(),
            external_id,
            user_name,
            formatted_name,
            family_name,
            given_name,
            display_name,
            emails,
            active,
            created: existing.created,
            last_modified: now,
        };

        state.users.insert(id.to_string(), updated_user.clone());

        // Update display names in groups if they cached it
        for group in state.groups.values_mut() {
            for member in &mut group.members {
                if member.value == id {
                    member.display = updated_user.display_name.clone().or_else(|| Some(updated_user.user_name.clone()));
                }
            }
        }

        Ok(updated_user)
    }

    pub async fn delete_user(&self, id: &str) -> Result<(), DbError> {
        let mut state = self.state.write().await;

        if state.users.remove(id).is_none() {
            return Err(DbError::UserNotFound(id.to_string()));
        }

        // Remove user from all groups
        for group in state.groups.values_mut() {
            group.members.retain(|m| m.value != id);
            group.last_modified = Utc::now();
        }

        Ok(())
    }

    pub async fn list_users(
        &self,
        filter: Option<String>,
        start_index: usize, // 1-based index in SCIM
        count: usize,
    ) -> (Vec<DbUser>, usize) {
        let state = self.state.read().await;
        let mut all_users: Vec<DbUser> = state.users.values().cloned().collect();

        // Sort by userName for consistency
        all_users.sort_by(|a, b| a.user_name.cmp(&b.user_name));

        // Basic SCIM filtering (e.g. userName eq "alice" or userName co "ali")
        let filtered_users = if let Some(ref filt) = filter {
            let parsed_filter = parse_scim_filter(filt);
            all_users
                .into_iter()
                .filter(|u| match &parsed_filter {
                    Some(Filter::Eq(field, val)) => {
                        if field.eq_ignore_ascii_case("username") {
                            u.user_name.eq_ignore_ascii_case(val)
                        } else if field.eq_ignore_ascii_case("externalid") {
                            u.external_id.as_deref().unwrap_or("").eq_ignore_ascii_case(val)
                        } else {
                            true
                        }
                    }
                    Some(Filter::Co(field, val)) => {
                        if field.eq_ignore_ascii_case("username") {
                            u.user_name.to_lowercase().contains(&val.to_lowercase())
                        } else {
                            true
                        }
                    }
                    None => true,
                })
                .collect()
        } else {
            all_users
        };

        let total_results = filtered_users.len();

        // Pagination
        // In SCIM, startIndex is 1-based.
        let skip = if start_index > 0 { start_index - 1 } else { 0 };
        let paginated = filtered_users
            .into_iter()
            .skip(skip)
            .take(count)
            .collect();

        (paginated, total_results)
    }

    // --- Group CRUD ---

    pub async fn create_group(
        &self,
        display_name: String,
        members: Vec<DbGroupMember>,
    ) -> Result<DbGroup, DbError> {
        let mut state = self.state.write().await;

        if state.groups.values().any(|g| g.display_name.eq_ignore_ascii_case(&display_name)) {
            return Err(DbError::GroupAlreadyExists(display_name));
        }

        // Hydrate display names of members if missing
        let mut hydrated_members = Vec::new();
        for m in members {
            let display = m.display.or_else(|| {
                state.users.get(&m.value).map(|u| u.display_name.clone().unwrap_or_else(|| u.user_name.clone()))
            });
            hydrated_members.push(DbGroupMember {
                value: m.value,
                display,
            });
        }

        let now = Utc::now();
        let group = DbGroup {
            id: Uuid::new_v4().to_string(),
            display_name,
            members: hydrated_members,
            created: now,
            last_modified: now,
        };

        state.groups.insert(group.id.clone(), group.clone());
        Ok(group)
    }

    pub async fn get_group(&self, id: &str) -> Option<DbGroup> {
        let state = self.state.read().await;
        state.groups.get(id).cloned()
    }

    pub async fn update_group(
        &self,
        id: &str,
        display_name: String,
        members: Vec<DbGroupMember>,
    ) -> Result<DbGroup, DbError> {
        let mut state = self.state.write().await;

        let existing = state.groups.get(id).ok_or_else(|| DbError::GroupNotFound(id.to_string()))?;

        if !existing.display_name.eq_ignore_ascii_case(&display_name) 
            && state.groups.values().any(|g| g.id != id && g.display_name.eq_ignore_ascii_case(&display_name)) 
        {
            return Err(DbError::GroupAlreadyExists(display_name));
        }

        // Hydrate display names
        let mut hydrated_members = Vec::new();
        for m in members {
            let display = m.display.or_else(|| {
                state.users.get(&m.value).map(|u| u.display_name.clone().unwrap_or_else(|| u.user_name.clone()))
            });
            hydrated_members.push(DbGroupMember {
                value: m.value,
                display,
            });
        }

        let now = Utc::now();
        let updated_group = DbGroup {
            id: id.to_string(),
            display_name,
            members: hydrated_members,
            created: existing.created,
            last_modified: now,
        };

        state.groups.insert(id.to_string(), updated_group.clone());
        Ok(updated_group)
    }

    pub async fn delete_group(&self, id: &str) -> Result<(), DbError> {
        let mut state = self.state.write().await;

        if state.groups.remove(id).is_none() {
            return Err(DbError::GroupNotFound(id.to_string()));
        }

        Ok(())
    }

    pub async fn list_groups(
        &self,
        filter: Option<String>,
        start_index: usize,
        count: usize,
    ) -> (Vec<DbGroup>, usize) {
        let state = self.state.read().await;
        let mut all_groups: Vec<DbGroup> = state.groups.values().cloned().collect();

        all_groups.sort_by(|a, b| a.display_name.cmp(&b.display_name));

        let filtered_groups = if let Some(ref filt) = filter {
            let parsed_filter = parse_scim_filter(filt);
            all_groups
                .into_iter()
                .filter(|g| match &parsed_filter {
                    Some(Filter::Eq(field, val)) => {
                        if field.eq_ignore_ascii_case("displayname") {
                            g.display_name.eq_ignore_ascii_case(val)
                        } else {
                            true
                        }
                    }
                    Some(Filter::Co(field, val)) => {
                        if field.eq_ignore_ascii_case("displayname") {
                            g.display_name.to_lowercase().contains(&val.to_lowercase())
                        } else {
                            true
                        }
                    }
                    None => true,
                })
                .collect()
        } else {
            all_groups
        };

        let total_results = filtered_groups.len();
        let skip = if start_index > 0 { start_index - 1 } else { 0 };
        let paginated = filtered_groups
            .into_iter()
            .skip(skip)
            .take(count)
            .collect();

        (paginated, total_results)
    }

    pub async fn get_user_groups(&self, user_id: &str) -> Vec<DbGroup> {
        let state = self.state.read().await;
        state.groups
            .values()
            .filter(|g| g.members.iter().any(|m| m.value == user_id))
            .cloned()
            .collect()
    }

    pub async fn get_stats(&self) -> (usize, usize) {
        let state = self.state.read().await;
        (state.users.len(), state.groups.len())
    }

    pub async fn clear_all(&self) {
        let mut state = self.state.write().await;
        state.users.clear();
        state.groups.clear();
    }
}

// --- Simplified SCIM Filter Parser ---
// We support standard SCIM filter operators like 'eq' and 'co' for userName and displayName
// Example: userName eq "bjensen"
// Example: displayName co "Admin"
enum Filter {
    Eq(String, String),
    Co(String, String),
}

fn parse_scim_filter(filter_str: &str) -> Option<Filter> {
    let parts: Vec<&str> = filter_str.split_whitespace().collect();
    if parts.len() < 3 {
        return None;
    }

    let field = parts[0].to_string();
    let op = parts[1].to_lowercase();
    // Reconstruct value which might contain spaces if it was quoted, and strip the quotes
    let val_str = parts[2..].join(" ");
    let val = val_str.trim_matches(|c| c == '"' || c == '\'').to_string();

    match op.as_str() {
        "eq" => Some(Filter::Eq(field, val)),
        "co" => Some(Filter::Co(field, val)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_create_and_get_user() {
        let db = Database::new();
        let user = db.create_user(
            None,
            "bob".to_string(),
            Some("Bob Smith".to_string()),
            Some("Smith".to_string()),
            Some("Bob".to_string()),
            Some("Bob Smith".to_string()),
            vec![],
            true,
        ).await.unwrap();

        assert_eq!(user.user_name, "bob");
        
        let found = db.get_user(&user.id).await.unwrap();
        assert_eq!(found.user_name, "bob");
    }

    #[tokio::test]
    async fn test_unique_username() {
        let db = Database::new();
        db.create_user(
            None,
            "bob".to_string(),
            None, None, None, None,
            vec![],
            true,
        ).await.unwrap();

        let result = db.create_user(
            None,
            "Bob".to_string(), // case-insensitive check
            None, None, None, None,
            vec![],
            true,
        ).await;

        assert!(matches!(result, Err(DbError::UserAlreadyExists(_))));
    }

    #[tokio::test]
    async fn test_group_referential_integrity() {
        let db = Database::new();
        let user1 = db.create_user(None, "user1".to_string(), None, None, None, None, vec![], true).await.unwrap();
        let user2 = db.create_user(None, "user2".to_string(), None, None, None, None, vec![], true).await.unwrap();

        let group = db.create_group(
            "Admins".to_string(),
            vec![
                DbGroupMember { value: user1.id.clone(), display: None },
                DbGroupMember { value: user2.id.clone(), display: None },
            ],
        ).await.unwrap();

        assert_eq!(group.members.len(), 2);

        // Delete user1
        db.delete_user(&user1.id).await.unwrap();

        // Get group and verify user1 was removed
        let updated_group = db.get_group(&group.id).await.unwrap();
        assert_eq!(updated_group.members.len(), 1);
        assert_eq!(updated_group.members[0].value, user2.id);
    }
}
