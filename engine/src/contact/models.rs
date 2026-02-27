use chrono::{DateTime, Utc};
use crate::Entity;
use serde::{Deserialize, Serialize};
use surrealdb::types::SurrealValue;

#[derive(Debug, Clone, Serialize, Deserialize, SurrealValue, Entity)]
#[surreal(crate = "surrealdb::types")]
#[entity(table = "contact")]
pub struct Contact {
    pub id: String,
    pub user_id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phone: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub company: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub job_title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub avatar: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct CreateContactRequest {
    pub name: String,
    pub phone: Option<String>,
    pub email: Option<String>,
    pub company: Option<String>,
    pub job_title: Option<String>,
    pub notes: Option<String>,
    pub avatar: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateContactRequest {
    pub name: Option<String>,
    pub phone: Option<String>,
    pub email: Option<String>,
    pub company: Option<String>,
    pub job_title: Option<String>,
    pub notes: Option<String>,
    pub avatar: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ContactResponse {
    pub id: String,
    pub user_id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phone: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub company: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub job_title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl From<Contact> for ContactResponse {
    fn from(c: Contact) -> Self {
        Self {
            id: c.id,
            user_id: c.user_id,
            name: c.name,
            phone: c.phone,
            email: c.email,
            company: c.company,
            job_title: c.job_title,
            notes: c.notes,
            avatar: c.avatar,
            created_at: c.created_at,
            updated_at: c.updated_at,
        }
    }
}
