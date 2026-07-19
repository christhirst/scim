use crate::grpc_server::proto::scim_control_client::ScimControlClient;
use crate::grpc_server::proto::*;
use clap::Subcommand;

#[derive(Subcommand, Debug, Clone)]
pub enum ClientCommand {
    /// Create a new SCIM User
    CreateUser {
        #[arg(long)]
        username: String,
        #[arg(long)]
        email: Vec<String>,
        #[arg(long)]
        display_name: Option<String>,
        #[arg(long, action = clap::ArgAction::Set, default_value = "true")]
        active: bool,
        #[arg(long)]
        external_id: Option<String>,
        #[arg(long)]
        given_name: Option<String>,
        #[arg(long)]
        family_name: Option<String>,
    },
    /// Retrieve a SCIM User by ID
    GetUser {
        id: String,
    },
    /// Update an existing SCIM User
    UpdateUser {
        id: String,
        #[arg(long)]
        username: String,
        #[arg(long)]
        email: Vec<String>,
        #[arg(long)]
        display_name: Option<String>,
        #[arg(long, action = clap::ArgAction::Set, default_value = "true")]
        active: bool,
        #[arg(long)]
        external_id: Option<String>,
        #[arg(long)]
        given_name: Option<String>,
        #[arg(long)]
        family_name: Option<String>,
    },
    /// Delete a SCIM User by ID
    DeleteUser {
        id: String,
    },
    /// List SCIM Users
    ListUsers {
        #[arg(long)]
        filter: Option<String>,
        #[arg(long, default_value = "1")]
        start_index: i32,
        #[arg(long, default_value = "20")]
        count: i32,
    },
    /// Create a new SCIM Group
    CreateGroup {
        #[arg(long)]
        name: String,
        #[arg(long)]
        member: Vec<String>, // List of User IDs
    },
    /// Retrieve a SCIM Group by ID
    GetGroup {
        id: String,
    },
    /// Update an existing SCIM Group
    UpdateGroup {
        id: String,
        #[arg(long)]
        name: String,
        #[arg(long)]
        member: Vec<String>, // List of User IDs
    },
    /// Delete a SCIM Group by ID
    DeleteGroup {
        id: String,
    },
    /// List SCIM Groups
    ListGroups {
        #[arg(long)]
        filter: Option<String>,
        #[arg(long, default_value = "1")]
        start_index: i32,
        #[arg(long, default_value = "20")]
        count: i32,
    },
    /// Fetch SCIM Server database statistics
    Stats,
    /// Clear all users and groups in the database
    ClearAll,
}

pub async fn run_client(endpoint: String, command: ClientCommand) -> Result<(), Box<dyn std::error::Error>> {
    println!("Connecting to SCIM gRPC endpoint: {}", endpoint);
    let mut client = ScimControlClient::connect(endpoint).await?;

    match command {
        ClientCommand::CreateUser {
            username,
            email,
            display_name,
            active,
            external_id,
            given_name,
            family_name,
        } => {
            let emails = email
                .into_iter()
                .map(|val| Email {
                    value: val,
                    r#type: "work".to_string(),
                    primary: false,
                })
                .collect();

            let formatted = match (&given_name, &family_name) {
                (Some(g), Some(f)) => format!("{} {}", g, f),
                (Some(g), None) => g.clone(),
                (None, Some(f)) => f.clone(),
                _ => "".to_string(),
            };

            let req = CreateUserRequest {
                user_name: username,
                emails,
                display_name: display_name.unwrap_or_default(),
                active,
                external_id: external_id.unwrap_or_default(),
                name: Some(Name {
                    formatted,
                    family_name: family_name.unwrap_or_default(),
                    given_name: given_name.unwrap_or_default(),
                }),
            };

            let response = client.create_user(req).await?;
            println!("User Created Successfully:\n{}", serde_json::to_string_pretty(&response.into_inner())?);
        }

        ClientCommand::GetUser { id } => {
            let req = GetUserRequest { id };
            let response = client.get_user(req).await?;
            println!("User Found:\n{}", serde_json::to_string_pretty(&response.into_inner())?);
        }

        ClientCommand::UpdateUser {
            id,
            username,
            email,
            display_name,
            active,
            external_id,
            given_name,
            family_name,
        } => {
            let emails = email
                .into_iter()
                .map(|val| Email {
                    value: val,
                    r#type: "work".to_string(),
                    primary: false,
                })
                .collect();

            let formatted = match (&given_name, &family_name) {
                (Some(g), Some(f)) => format!("{} {}", g, f),
                (Some(g), None) => g.clone(),
                (None, Some(f)) => f.clone(),
                _ => "".to_string(),
            };

            let req = UpdateUserRequest {
                id,
                user_name: username,
                emails,
                display_name: display_name.unwrap_or_default(),
                active,
                external_id: external_id.unwrap_or_default(),
                name: Some(Name {
                    formatted,
                    family_name: family_name.unwrap_or_default(),
                    given_name: given_name.unwrap_or_default(),
                }),
            };

            let response = client.update_user(req).await?;
            println!("User Updated Successfully:\n{}", serde_json::to_string_pretty(&response.into_inner())?);
        }

        ClientCommand::DeleteUser { id } => {
            let req = DeleteUserRequest { id };
            let response = client.delete_user(req).await?;
            println!("Delete Result:\n{}", serde_json::to_string_pretty(&response.into_inner())?);
        }

        ClientCommand::ListUsers { filter, start_index, count } => {
            let req = ListUsersRequest {
                filter: filter.unwrap_or_default(),
                start_index,
                count,
            };
            let response = client.list_users(req).await?;
            println!("Users List:\n{}", serde_json::to_string_pretty(&response.into_inner())?);
        }

        ClientCommand::CreateGroup { name, member } => {
            let members = member
                .into_iter()
                .map(|val| GroupMember {
                    value: val,
                    r#ref: "".to_string(),
                    display: "".to_string(),
                })
                .collect();

            let req = CreateGroupRequest {
                display_name: name,
                members,
            };
            let response = client.create_group(req).await?;
            println!("Group Created Successfully:\n{}", serde_json::to_string_pretty(&response.into_inner())?);
        }

        ClientCommand::GetGroup { id } => {
            let req = GetGroupRequest { id };
            let response = client.get_group(req).await?;
            println!("Group Found:\n{}", serde_json::to_string_pretty(&response.into_inner())?);
        }

        ClientCommand::UpdateGroup { id, name, member } => {
            let members = member
                .into_iter()
                .map(|val| GroupMember {
                    value: val,
                    r#ref: "".to_string(),
                    display: "".to_string(),
                })
                .collect();

            let req = UpdateGroupRequest {
                id,
                display_name: name,
                members,
            };
            let response = client.update_group(req).await?;
            println!("Group Updated Successfully:\n{}", serde_json::to_string_pretty(&response.into_inner())?);
        }

        ClientCommand::DeleteGroup { id } => {
            let req = DeleteGroupRequest { id };
            let response = client.delete_group(req).await?;
            println!("Delete Result:\n{}", serde_json::to_string_pretty(&response.into_inner())?);
        }

        ClientCommand::ListGroups { filter, start_index, count } => {
            let req = ListGroupsRequest {
                filter: filter.unwrap_or_default(),
                start_index,
                count,
            };
            let response = client.list_groups(req).await?;
            println!("Groups List:\n{}", serde_json::to_string_pretty(&response.into_inner())?);
        }

        ClientCommand::Stats => {
            let req = StatsRequest {};
            let response = client.get_stats(req).await?;
            println!("Server Database Stats:\n{}", serde_json::to_string_pretty(&response.into_inner())?);
        }

        ClientCommand::ClearAll => {
            let req = ClearAllRequest {};
            let response = client.clear_all(req).await?;
            println!("Clear All Result:\n{}", serde_json::to_string_pretty(&response.into_inner())?);
        }
    }

    Ok(())
}
