use chrono::{Duration, Utc};
use frona::api::db;
use frona::api::repo::generic::SurrealRepo;
use frona::repository::Repository;
use frona::schedule::models::RoutineStatus;
use frona::schedule::service::ScheduleService;
use surrealdb::engine::local::{Db, Mem};
use surrealdb::Surreal;

async fn test_db() -> Surreal<Db> {
    let db = Surreal::new::<Mem>(()).await.unwrap();
    db::setup_schema(&db).await.unwrap();
    db
}

fn make_service(db: Surreal<Db>) -> ScheduleService {
    ScheduleService::new(SurrealRepo::new(db))
}

#[tokio::test]
async fn get_or_create_routine_creates_new() {
    let db = test_db().await;
    let svc = make_service(db);

    let routine = svc.get_or_create_routine("user-1", "agent-1").await.unwrap();
    assert_eq!(routine.user_id, "user-1");
    assert_eq!(routine.agent_id, "agent-1");
    assert!(routine.items.is_empty());
    assert!(routine.interval_mins.is_none());
    assert!(routine.next_run_at.is_none());
    assert_eq!(routine.status, RoutineStatus::Idle);
}

#[tokio::test]
async fn get_or_create_routine_returns_existing() {
    let db = test_db().await;
    let svc = make_service(db);

    let first = svc.get_or_create_routine("user-1", "agent-1").await.unwrap();
    let second = svc.get_or_create_routine("user-1", "agent-1").await.unwrap();
    assert_eq!(first.id, second.id);
}

#[tokio::test]
async fn get_or_create_routine_separate_per_agent() {
    let db = test_db().await;
    let svc = make_service(db);

    let r1 = svc.get_or_create_routine("user-1", "agent-1").await.unwrap();
    let r2 = svc.get_or_create_routine("user-1", "agent-2").await.unwrap();
    assert_ne!(r1.id, r2.id);
    assert_eq!(r1.agent_id, "agent-1");
    assert_eq!(r2.agent_id, "agent-2");
}

#[tokio::test]
async fn update_routine_items_adds_items() {
    let db = test_db().await;
    let svc = make_service(db);

    let routine = svc.get_or_create_routine("user-1", "agent-1").await.unwrap();
    let updated = svc
        .update_routine_items(
            &routine.id,
            vec!["Check emails".into(), "Review calendar".into()],
            vec![],
        )
        .await
        .unwrap();

    assert_eq!(updated.items.len(), 2);
    assert_eq!(updated.items[0].description, "Check emails");
    assert_eq!(updated.items[1].description, "Review calendar");
    assert!(!updated.items[0].id.is_empty());
}

#[tokio::test]
async fn update_routine_items_removes_by_id() {
    let db = test_db().await;
    let svc = make_service(db);

    let routine = svc.get_or_create_routine("user-1", "agent-1").await.unwrap();
    let with_items = svc
        .update_routine_items(
            &routine.id,
            vec!["Item A".into(), "Item B".into(), "Item C".into()],
            vec![],
        )
        .await
        .unwrap();

    assert_eq!(with_items.items.len(), 3);

    let remove_id = with_items.items[1].id.clone();
    let after_remove = svc
        .update_routine_items(&routine.id, vec![], vec![remove_id])
        .await
        .unwrap();

    assert_eq!(after_remove.items.len(), 2);
    assert_eq!(after_remove.items[0].description, "Item A");
    assert_eq!(after_remove.items[1].description, "Item C");
}

#[tokio::test]
async fn update_routine_items_add_and_remove_simultaneously() {
    let db = test_db().await;
    let svc = make_service(db);

    let routine = svc.get_or_create_routine("user-1", "agent-1").await.unwrap();
    let with_items = svc
        .update_routine_items(&routine.id, vec!["Old item".into()], vec![])
        .await
        .unwrap();

    let old_id = with_items.items[0].id.clone();
    let updated = svc
        .update_routine_items(
            &routine.id,
            vec!["New item".into()],
            vec![old_id],
        )
        .await
        .unwrap();

    assert_eq!(updated.items.len(), 1);
    assert_eq!(updated.items[0].description, "New item");
}

#[tokio::test]
async fn set_routine_interval_sets_next_run_at() {
    let db = test_db().await;
    let svc = make_service(db);

    let routine = svc.get_or_create_routine("user-1", "agent-1").await.unwrap();
    assert!(routine.next_run_at.is_none());

    let before = Utc::now();
    let updated = svc.set_routine_interval(&routine.id, Some(30)).await.unwrap();

    assert_eq!(updated.interval_mins, Some(30));
    assert!(updated.next_run_at.is_some());
    let next = updated.next_run_at.unwrap();
    assert!(next >= before + Duration::minutes(29));
    assert!(next <= before + Duration::minutes(31));
}

#[tokio::test]
async fn set_routine_interval_recalculates_next_run_at() {
    let db = test_db().await;
    let svc = make_service(db);

    let routine = svc.get_or_create_routine("user-1", "agent-1").await.unwrap();
    let first = svc.set_routine_interval(&routine.id, Some(60)).await.unwrap();
    let first_next = first.next_run_at.unwrap();

    let before = Utc::now();
    let second = svc.set_routine_interval(&routine.id, Some(120)).await.unwrap();
    assert_eq!(second.interval_mins, Some(120));
    let second_next = second.next_run_at.unwrap();
    assert_ne!(second_next, first_next, "next_run_at should be recalculated");
    assert!(second_next >= before + Duration::minutes(119));
    assert!(second_next <= before + Duration::minutes(121));
}

#[tokio::test]
async fn set_routine_interval_recalculates_when_shortened() {
    let db = test_db().await;
    let svc = make_service(db);

    let routine = svc.get_or_create_routine("user-1", "agent-1").await.unwrap();
    svc.set_routine_interval(&routine.id, Some(60)).await.unwrap();

    let before = Utc::now();
    let updated = svc.set_routine_interval(&routine.id, Some(15)).await.unwrap();
    let next = updated.next_run_at.unwrap();
    assert!(next >= before + Duration::minutes(14));
    assert!(next <= before + Duration::minutes(16));
}

#[tokio::test]
async fn set_routine_interval_recalculates_when_lengthened() {
    let db = test_db().await;
    let svc = make_service(db);

    let routine = svc.get_or_create_routine("user-1", "agent-1").await.unwrap();
    svc.set_routine_interval(&routine.id, Some(15)).await.unwrap();

    let before = Utc::now();
    let updated = svc.set_routine_interval(&routine.id, Some(120)).await.unwrap();
    let next = updated.next_run_at.unwrap();
    assert!(next >= before + Duration::minutes(119));
    assert!(next <= before + Duration::minutes(121));
}

#[tokio::test]
async fn set_routine_interval_same_value_still_resets() {
    let db = test_db().await;
    let svc = make_service(db);

    let routine = svc.get_or_create_routine("user-1", "agent-1").await.unwrap();
    let first = svc.set_routine_interval(&routine.id, Some(30)).await.unwrap();
    let first_next = first.next_run_at.unwrap();

    let before = Utc::now();
    let second = svc.set_routine_interval(&routine.id, Some(30)).await.unwrap();
    let second_next = second.next_run_at.unwrap();
    assert!(second_next >= before + Duration::minutes(29));
    assert!(second_next <= before + Duration::minutes(31));
    assert!(second_next >= first_next, "next_run_at should be recalculated from now");
}

#[tokio::test]
async fn set_routine_interval_none_clears_schedule() {
    let db = test_db().await;
    let svc = make_service(db);

    let routine = svc.get_or_create_routine("user-1", "agent-1").await.unwrap();
    svc.set_routine_interval(&routine.id, Some(30)).await.unwrap();

    let cleared = svc.set_routine_interval(&routine.id, None).await.unwrap();
    assert!(cleared.interval_mins.is_none());
    assert!(cleared.next_run_at.is_none());
}

#[tokio::test]
async fn mark_running_transitions_status() {
    let db = test_db().await;
    let svc = make_service(db);

    let routine = svc.get_or_create_routine("user-1", "agent-1").await.unwrap();
    assert_eq!(routine.status, RoutineStatus::Idle);

    let running = svc.mark_running(&routine.id).await.unwrap();
    assert_eq!(running.status, RoutineStatus::Running);
}

#[tokio::test]
async fn mark_idle_and_advance_sets_next_run_from_completion() {
    let db = test_db().await;
    let svc = make_service(db);

    let routine = svc.get_or_create_routine("user-1", "agent-1").await.unwrap();
    svc.set_routine_interval(&routine.id, Some(45)).await.unwrap();
    svc.mark_running(&routine.id).await.unwrap();

    let before = Utc::now();
    let idle = svc.mark_idle_and_advance(&routine.id).await.unwrap();

    assert_eq!(idle.status, RoutineStatus::Idle);
    assert!(idle.last_run_at.is_some());

    let next = idle.next_run_at.unwrap();
    assert!(next >= before + Duration::minutes(44));
    assert!(next <= before + Duration::minutes(46));
}

#[tokio::test]
async fn find_due_routines_returns_due_idle_only() {
    use frona::schedule::models::Routine;

    let db = test_db().await;
    let repo: SurrealRepo<Routine> = SurrealRepo::new(db.clone());
    let svc = make_service(db);

    // r1: idle, interval set, next_run_at in the past → should be due
    let r1 = svc.get_or_create_routine("user-1", "agent-1").await.unwrap();
    svc.set_routine_interval(&r1.id, Some(1)).await.unwrap();
    svc.mark_running(&r1.id).await.unwrap();
    svc.mark_idle_and_advance(&r1.id).await.unwrap();
    // Force next_run_at to the past via repo.update()
    let mut r1_updated = repo.find_by_id(&r1.id).await.unwrap().unwrap();
    r1_updated.next_run_at = Some(Utc::now() - Duration::minutes(5));
    repo.update(&r1_updated).await.unwrap();

    // r2: running, interval set → should NOT be due (running)
    let r2 = svc.get_or_create_routine("user-1", "agent-2").await.unwrap();
    svc.set_routine_interval(&r2.id, Some(1)).await.unwrap();
    svc.mark_running(&r2.id).await.unwrap();

    // r3: idle, interval set, next_run_at far in the future → should NOT be due
    let r3 = svc.get_or_create_routine("user-1", "agent-3").await.unwrap();
    svc.set_routine_interval(&r3.id, Some(99999)).await.unwrap();

    // r4: idle, no interval → should NOT be due
    let _r4 = svc.get_or_create_routine("user-1", "agent-4").await.unwrap();

    let due = svc.find_due_routines().await.unwrap();
    assert_eq!(due.len(), 1, "Only r1 should be due (idle + past next_run_at)");
    assert_eq!(due[0].id, r1.id);
}

#[tokio::test]
async fn running_routine_not_picked_up_after_restart() {
    use frona::schedule::models::Routine;

    let db = test_db().await;
    let repo: SurrealRepo<Routine> = SurrealRepo::new(db.clone());
    let svc = make_service(db);

    // Simulate: routine was running when server crashed
    let r = svc.get_or_create_routine("user-1", "agent-1").await.unwrap();
    svc.set_routine_interval(&r.id, Some(30)).await.unwrap();

    // Force next_run_at to the past so it would normally be due
    let mut updated = repo.find_by_id(&r.id).await.unwrap().unwrap();
    updated.next_run_at = Some(Utc::now() - Duration::minutes(10));
    repo.update(&updated).await.unwrap();

    // Mark running (simulates mid-execution when server crashed)
    svc.mark_running(&r.id).await.unwrap();

    // After restart, find_due_routines should NOT return Running routines
    let due = svc.find_due_routines().await.unwrap();
    assert!(due.is_empty(), "Running routine should not be re-scheduled on restart");

    // Recovery: mark idle and advance to make it eligible again
    svc.mark_idle_and_advance(&r.id).await.unwrap();
    let routine = svc.find_by_id(&r.id).await.unwrap().unwrap();
    assert_eq!(routine.status, RoutineStatus::Idle);
    assert!(routine.next_run_at.unwrap() > Utc::now());
}

#[tokio::test]
async fn multiple_due_routines_all_returned() {
    use frona::schedule::models::Routine;

    let db = test_db().await;
    let repo: SurrealRepo<Routine> = SurrealRepo::new(db.clone());
    let svc = make_service(db);

    let mut expected_ids = Vec::new();
    for i in 1..=3 {
        let r = svc
            .get_or_create_routine("user-1", &format!("agent-{}", i))
            .await
            .unwrap();
        svc.set_routine_interval(&r.id, Some(1)).await.unwrap();
        // Force next_run_at to the past
        let mut updated = repo.find_by_id(&r.id).await.unwrap().unwrap();
        updated.next_run_at = Some(Utc::now() - Duration::minutes(5));
        repo.update(&updated).await.unwrap();
        expected_ids.push(r.id);
    }

    let due = svc.find_due_routines().await.unwrap();
    assert_eq!(due.len(), 3, "All 3 routines should be due");
    let due_ids: Vec<&str> = due.iter().map(|r| r.id.as_str()).collect();
    for id in &expected_ids {
        assert!(due_ids.contains(&id.as_str()));
    }
}
