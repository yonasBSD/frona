use std::str::FromStr;

use chrono::{DateTime, Duration, Utc};

use crate::api::repo::schedules::SurrealRoutineRepo;
use crate::error::AppError;
use crate::repository::Repository;

use super::models::{Routine, RoutineItem, RoutineStatus};
use super::repository::RoutineRepository;

#[derive(Clone)]
pub struct ScheduleService {
    routine_repo: SurrealRoutineRepo,
}

impl ScheduleService {
    pub fn new(routine_repo: SurrealRoutineRepo) -> Self {
        Self { routine_repo }
    }

    pub fn repo(&self) -> &SurrealRoutineRepo {
        &self.routine_repo
    }

    pub fn parse_cron(expression: &str) -> Result<cron::Schedule, AppError> {
        let seven_field = format!("0 {} *", expression);
        cron::Schedule::from_str(&seven_field)
            .map_err(|e| AppError::Validation(format!("Invalid cron expression '{}': {}", expression, e)))
    }

    pub fn next_cron_occurrence(expression: &str) -> Result<DateTime<Utc>, AppError> {
        let schedule = Self::parse_cron(expression)?;
        schedule
            .upcoming(Utc)
            .next()
            .ok_or_else(|| AppError::Validation("Cron expression has no future occurrences".into()))
    }

    pub async fn get_or_create_routine(
        &self,
        user_id: &str,
        agent_id: &str,
    ) -> Result<Routine, AppError> {
        if let Some(routine) = self.routine_repo.find_by_agent_id(user_id, agent_id).await? {
            return Ok(routine);
        }

        let now = Utc::now();
        let routine = Routine {
            id: uuid::Uuid::new_v4().to_string(),
            user_id: user_id.to_string(),
            agent_id: agent_id.to_string(),
            items: Vec::new(),
            interval_mins: None,
            chat_id: None,
            status: RoutineStatus::Idle,
            next_run_at: None,
            last_run_at: None,
            created_at: now,
            updated_at: now,
        };

        self.routine_repo.create(&routine).await
    }

    pub async fn update_routine_items(
        &self,
        routine_id: &str,
        items_to_add: Vec<String>,
        items_to_remove: Vec<String>,
    ) -> Result<Routine, AppError> {
        let mut routine = self
            .routine_repo
            .find_by_id(routine_id)
            .await?
            .ok_or_else(|| AppError::NotFound("Routine not found".into()))?;

        routine.items.retain(|item| !items_to_remove.contains(&item.id));

        let now = Utc::now();
        for desc in items_to_add {
            routine.items.push(RoutineItem {
                id: uuid::Uuid::new_v4().to_string(),
                description: desc,
                added_at: now,
            });
        }

        routine.updated_at = now;
        self.routine_repo.update(&routine).await
    }

    pub async fn set_routine_interval(
        &self,
        routine_id: &str,
        interval_mins: Option<u64>,
    ) -> Result<Routine, AppError> {
        let mut routine = self
            .routine_repo
            .find_by_id(routine_id)
            .await?
            .ok_or_else(|| AppError::NotFound("Routine not found".into()))?;

        routine.interval_mins = interval_mins;

        match interval_mins {
            Some(mins) => {
                routine.next_run_at = Some(Utc::now() + Duration::minutes(mins as i64));
            }
            None => {
                routine.next_run_at = None;
            }
        }

        routine.updated_at = Utc::now();
        self.routine_repo.update(&routine).await
    }

    pub async fn mark_running(&self, routine_id: &str) -> Result<Routine, AppError> {
        let mut routine = self
            .routine_repo
            .find_by_id(routine_id)
            .await?
            .ok_or_else(|| AppError::NotFound("Routine not found".into()))?;

        routine.status = RoutineStatus::Running;
        routine.updated_at = Utc::now();
        self.routine_repo.update(&routine).await
    }

    pub async fn mark_idle_and_advance(&self, routine_id: &str) -> Result<Routine, AppError> {
        let mut routine = self
            .routine_repo
            .find_by_id(routine_id)
            .await?
            .ok_or_else(|| AppError::NotFound("Routine not found".into()))?;

        let now = Utc::now();
        routine.status = RoutineStatus::Idle;
        routine.last_run_at = Some(now);

        if let Some(mins) = routine.interval_mins {
            routine.next_run_at = Some(now + Duration::minutes(mins as i64));
        }

        routine.updated_at = now;
        self.routine_repo.update(&routine).await
    }

    pub async fn find_due_routines(&self) -> Result<Vec<Routine>, AppError> {
        self.routine_repo.find_due_idle(Utc::now()).await
    }

    pub async fn find_by_id(&self, routine_id: &str) -> Result<Option<Routine>, AppError> {
        self.routine_repo.find_by_id(routine_id).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_cron_valid_every_minute() {
        let schedule = ScheduleService::parse_cron("* * * * *").unwrap();
        let next = schedule.upcoming(Utc).next().unwrap();
        assert!(next > Utc::now());
    }

    #[test]
    fn parse_cron_valid_daily_9am() {
        let schedule = ScheduleService::parse_cron("0 9 * * *").unwrap();
        let next = schedule.upcoming(Utc).next().unwrap();
        assert_eq!(next.minute(), 0);
        assert_eq!(next.hour(), 9);
    }

    #[test]
    fn parse_cron_valid_weekdays_at_noon() {
        let schedule = ScheduleService::parse_cron("0 12 * * MON-FRI").unwrap();
        let next = schedule.upcoming(Utc).next().unwrap();
        assert_eq!(next.hour(), 12);
        assert_eq!(next.minute(), 0);
        let weekday = next.weekday().num_days_from_monday();
        assert!(weekday < 5, "Should be a weekday (Mon=0 .. Fri=4), got {weekday}");
    }

    #[test]
    fn parse_cron_valid_every_30_mins() {
        let schedule = ScheduleService::parse_cron("*/30 * * * *").unwrap();
        let occurrences: Vec<_> = schedule.upcoming(Utc).take(4).collect();
        assert_eq!(occurrences.len(), 4);
        for occ in &occurrences {
            assert!(occ.minute() == 0 || occ.minute() == 30);
        }
    }

    #[test]
    fn parse_cron_invalid_expression() {
        let result = ScheduleService::parse_cron("not a cron");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Invalid cron expression"), "Error: {err}");
    }

    #[test]
    fn parse_cron_rejects_empty() {
        assert!(ScheduleService::parse_cron("").is_err());
    }

    #[test]
    fn parse_cron_rejects_6_fields() {
        let result = ScheduleService::parse_cron("0 0 9 * * MON");
        assert!(result.is_err());
    }

    #[test]
    fn parse_cron_rejects_3_fields() {
        let result = ScheduleService::parse_cron("0 9 *");
        assert!(result.is_err());
    }

    #[test]
    fn next_cron_occurrence_returns_future() {
        let next = ScheduleService::next_cron_occurrence("* * * * *").unwrap();
        assert!(next > Utc::now());
    }

    #[test]
    fn next_cron_occurrence_daily_has_correct_time() {
        let next = ScheduleService::next_cron_occurrence("30 14 * * *").unwrap();
        assert_eq!(next.hour(), 14);
        assert_eq!(next.minute(), 30);
    }

    #[test]
    fn next_cron_occurrence_multiple_calls_are_consistent() {
        let a = ScheduleService::next_cron_occurrence("0 0 * * *").unwrap();
        let b = ScheduleService::next_cron_occurrence("0 0 * * *").unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn next_cron_occurrence_invalid_returns_error() {
        assert!(ScheduleService::next_cron_occurrence("invalid").is_err());
    }

    use chrono::{Datelike, Timelike};
}
