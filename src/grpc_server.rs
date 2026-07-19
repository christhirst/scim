use std::net::SocketAddr;
use tonic::{transport::Server, Request, Response, Status};
use crate::database::{Database, DbEmail, DbError, DbGroupMember, DbUser, DbGroup};
use crate::settings::Settings;

pub mod proto {
    tonic::include_proto!("scim");
}

use proto::scim_control_server::{ScimControl, ScimControlServer};
use proto::*;

#[derive(Clone)]
pub struct ScimControlService {
    db: Database,
}

impl ScimControlService {
    pub fn new(db: Database) -> Self {
        Self { db }
    }
}

// Map DbError to tonic Status
fn map_db_error(err: DbError) -> Status {
    match err {
        DbError::UserNotFound(id) => Status::not_found(format!("User not found: {}", id)),
        DbError::GroupNotFound(id) => Status::not_found(format!("Group not found: {}", id)),
        DbError::UserAlreadyExists(username) => Status::already_exists(format!("User with username '{}' already exists", username)),
        DbError::GroupAlreadyExists(name) => Status::already_exists(format!("Group with displayName '{}' already exists", name)),
    }
}

// Map DbUser to Proto User
fn db_user_to_proto(u: DbUser) -> User {
    User {
        id: u.id,
        external_id: u.external_id.unwrap_or_default(),
        user_name: u.user_name,
        name: Some(Name {
            formatted: u.formatted_name.unwrap_or_default(),
            family_name: u.family_name.unwrap_or_default(),
            given_name: u.given_name.unwrap_or_default(),
        }),
        display_name: u.display_name.unwrap_or_default(),
        emails: u.emails
            .into_iter()
            .map(|e| Email {
                value: e.value,
                r#type: e.r#type.unwrap_or_default(),
                primary: e.primary,
            })
            .collect(),
        active: u.active,
    }
}

// Map DbGroup to Proto Group
fn db_group_to_proto(g: DbGroup) -> Group {
    Group {
        id: g.id,
        display_name: g.display_name,
        members: g.members
            .into_iter()
            .map(|m| GroupMember {
                value: m.value,
                r#ref: "".to_string(), // Not needed internally for gRPC
                display: m.display.unwrap_or_default(),
            })
            .collect(),
    }
}

#[tonic::async_trait]
impl ScimControl for ScimControlService {
    async fn create_user(
        &self,
        request: Request<CreateUserRequest>,
    ) -> Result<Response<UserResponse>, Status> {
        let req = request.into_inner();

        let db_emails: Vec<DbEmail> = req.emails
            .into_iter()
            .map(|e| DbEmail {
                value: e.value,
                r#type: Some(e.r#type),
                primary: e.primary,
            })
            .collect();

        let name = req.name.unwrap_or_default();
        let external_id = if req.external_id.is_empty() { None } else { Some(req.external_id) };
        let display_name = if req.display_name.is_empty() { None } else { Some(req.display_name) };

        match self.db.create_user(
            external_id,
            req.user_name,
            Some(name.formatted),
            Some(name.family_name),
            Some(name.given_name),
            display_name,
            db_emails,
            req.active,
        ).await {
            Ok(user) => Ok(Response::new(UserResponse {
                user: Some(db_user_to_proto(user)),
            })),
            Err(e) => Err(map_db_error(e)),
        }
    }

    async fn get_user(
        &self,
        request: Request<GetUserRequest>,
    ) -> Result<Response<UserResponse>, Status> {
        let req = request.into_inner();
        if let Some(user) = self.db.get_user(&req.id).await {
            Ok(Response::new(UserResponse {
                user: Some(db_user_to_proto(user)),
            }))
        } else {
            Err(Status::not_found(format!("User not found: {}", req.id)))
        }
    }

    async fn update_user(
        &self,
        request: Request<UpdateUserRequest>,
    ) -> Result<Response<UserResponse>, Status> {
        let req = request.into_inner();

        let db_emails: Vec<DbEmail> = req.emails
            .into_iter()
            .map(|e| DbEmail {
                value: e.value,
                r#type: Some(e.r#type),
                primary: e.primary,
            })
            .collect();

        let name = req.name.unwrap_or_default();
        let external_id = if req.external_id.is_empty() { None } else { Some(req.external_id) };
        let display_name = if req.display_name.is_empty() { None } else { Some(req.display_name) };

        match self.db.update_user(
            &req.id,
            external_id,
            req.user_name,
            Some(name.formatted),
            Some(name.family_name),
            Some(name.given_name),
            display_name,
            db_emails,
            req.active,
        ).await {
            Ok(user) => Ok(Response::new(UserResponse {
                user: Some(db_user_to_proto(user)),
            })),
            Err(e) => Err(map_db_error(e)),
        }
    }

    async fn delete_user(
        &self,
        request: Request<DeleteUserRequest>,
    ) -> Result<Response<DeleteResponse>, Status> {
        let req = request.into_inner();
        match self.db.delete_user(&req.id).await {
            Ok(_) => Ok(Response::new(DeleteResponse {
                success: true,
                message: "User deleted successfully".to_string(),
            })),
            Err(e) => Err(map_db_error(e)),
        }
    }

    async fn list_users(
        &self,
        request: Request<ListUsersRequest>,
    ) -> Result<Response<ListUsersResponse>, Status> {
        let req = request.into_inner();
        let filter = if req.filter.is_empty() { None } else { Some(req.filter) };
        let start_index = if req.start_index <= 0 { 1 } else { req.start_index as usize };
        let count = if req.count <= 0 { 20 } else { req.count as usize };

        let (users, total) = self.db.list_users(filter, start_index, count).await;
        let proto_users = users.into_iter().map(db_user_to_proto).collect();

        Ok(Response::new(ListUsersResponse {
            users: proto_users,
            total_results: total as i32,
            items_per_page: count as i32,
            start_index: start_index as i32,
        }))
    }

    async fn create_group(
        &self,
        request: Request<CreateGroupRequest>,
    ) -> Result<Response<GroupResponse>, Status> {
        let req = request.into_inner();
        let db_members = req.members
            .into_iter()
            .map(|m| DbGroupMember {
                value: m.value,
                display: if m.display.is_empty() { None } else { Some(m.display) },
            })
            .collect();

        match self.db.create_group(req.display_name, db_members).await {
            Ok(group) => Ok(Response::new(GroupResponse {
                group: Some(db_group_to_proto(group)),
            })),
            Err(e) => Err(map_db_error(e)),
        }
    }

    async fn get_group(
        &self,
        request: Request<GetGroupRequest>,
    ) -> Result<Response<GroupResponse>, Status> {
        let req = request.into_inner();
        if let Some(group) = self.db.get_group(&req.id).await {
            Ok(Response::new(GroupResponse {
                group: Some(db_group_to_proto(group)),
            }))
        } else {
            Err(Status::not_found(format!("Group not found: {}", req.id)))
        }
    }

    async fn update_group(
        &self,
        request: Request<UpdateGroupRequest>,
    ) -> Result<Response<GroupResponse>, Status> {
        let req = request.into_inner();
        let db_members = req.members
            .into_iter()
            .map(|m| DbGroupMember {
                value: m.value,
                display: if m.display.is_empty() { None } else { Some(m.display) },
            })
            .collect();

        match self.db.update_group(&req.id, req.display_name, db_members).await {
            Ok(group) => Ok(Response::new(GroupResponse {
                group: Some(db_group_to_proto(group)),
            })),
            Err(e) => Err(map_db_error(e)),
        }
    }

    async fn delete_group(
        &self,
        request: Request<DeleteGroupRequest>,
    ) -> Result<Response<DeleteResponse>, Status> {
        let req = request.into_inner();
        match self.db.delete_group(&req.id).await {
            Ok(_) => Ok(Response::new(DeleteResponse {
                success: true,
                message: "Group deleted successfully".to_string(),
            })),
            Err(e) => Err(map_db_error(e)),
        }
    }

    async fn list_groups(
        &self,
        request: Request<ListGroupsRequest>,
    ) -> Result<Response<ListGroupsResponse>, Status> {
        let req = request.into_inner();
        let filter = if req.filter.is_empty() { None } else { Some(req.filter) };
        let start_index = if req.start_index <= 0 { 1 } else { req.start_index as usize };
        let count = if req.count <= 0 { 20 } else { req.count as usize };

        let (groups, total) = self.db.list_groups(filter, start_index, count).await;
        let proto_groups = groups.into_iter().map(db_group_to_proto).collect();

        Ok(Response::new(ListGroupsResponse {
            groups: proto_groups,
            total_results: total as i32,
            items_per_page: count as i32,
            start_index: start_index as i32,
        }))
    }

    async fn get_stats(
        &self,
        _request: Request<StatsRequest>,
    ) -> Result<Response<StatsResponse>, Status> {
        let (users, groups) = self.db.get_stats().await;
        Ok(Response::new(StatsResponse {
            total_users: users as i32,
            total_groups: groups as i32,
        }))
    }

    async fn clear_all(
        &self,
        _request: Request<ClearAllRequest>,
    ) -> Result<Response<ClearAllResponse>, Status> {
        self.db.clear_all().await;
        Ok(Response::new(ClearAllResponse { success: true }))
    }
}

pub async fn run(db: Database, settings: Settings) {
    let grpc_addr = format!("{}:{}", settings.grpc_host, settings.grpc_port);
    let addr: SocketAddr = grpc_addr.parse().expect("Failed to parse grpc listener address");

    let service = ScimControlService::new(db);

    println!("Starting SCIM gRPC control server on {}", addr);
    Server::builder()
        .add_service(ScimControlServer::new(service))
        .serve(addr)
        .await
        .expect("Failed to serve gRPC server");
}
