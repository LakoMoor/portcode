//! `team`: run several role agents at once (project manager, tech lead, researchers, developers, testers, reviewers).
use crate::agents::TEAM_ROLES;
use crate::tool::*;
use async_trait::async_trait;
use futures_util::future::join_all;
use serde_json::{json, Value};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use tokio::sync::Semaphore;

const MAX_TASKS: usize = 8;

pub struct Team;
#[async_trait]
impl Tool for Team {
    fn name(&self) -> String {
        "team".into()
    }
    fn description(&self) -> String {
        format!(
            "Run team members at the same time and get every answer back. Roles: {}. Each task must be self-contained (goal, files, what to reply with). Developers on one call must own different files.",
            TEAM_ROLES.join(", ")
        )
    }
    fn example(&self) -> String {
        r#"{"tasks": [{"role": "researcher", "task": "Where is the config file parsed? Answer with file:line."}, {"role": "pm", "task": "Split 'add a --json flag' into steps with done-criteria."}]}"#.into()
    }
    fn schema(&self) -> Value {
        obj(
            json!({"tasks": {"type":"array","description":"1-8 jobs that run together","items":{"type":"object","properties":{"role":{"type":"string","enum":TEAM_ROLES},"task":{"type":"string"},"name":{"type":"string","description":"short label"}},"required":["role","task"]}}}),
            &["tasks"],
        )
    }
    async fn run(&self, ctx: &Ctx, args: Value) -> ToolResult {
        let limit = ctx.team_parallel.load(Ordering::Relaxed);
        if limit == 0 {
            return err("team mode is off. Ask the user to turn it on (config \"team\": {\"enabled\": true} or /team on).");
        }
        let tasks = args
            .get("tasks")
            .and_then(|t| t.as_array())
            .filter(|t| !t.is_empty())
            .ok_or_else(|| {
                ToolError(format!(
                    "tasks must be a non-empty list. Example: {}",
                    self.example()
                ))
            })?;
        if tasks.len() > MAX_TASKS {
            return err(format!(
                "at most {MAX_TASKS} tasks per call; split the rest into a second call."
            ));
        }
        let runner = lock(&ctx.sub)
            .clone()
            .ok_or_else(|| ToolError("sub-agents are not available in this session".into()))?;
        let mut jobs = Vec::new();
        for (i, t) in tasks.iter().enumerate() {
            let role = t.get("role").and_then(|v| v.as_str()).unwrap_or("");
            let task = t.get("task").and_then(|v| v.as_str()).unwrap_or("").trim();
            if !TEAM_ROLES.contains(&role) {
                return err(format!(
                    "task {}: role '{role}' is not one of {}",
                    i + 1,
                    TEAM_ROLES.join(", ")
                ));
            }
            if task.is_empty() {
                return err(format!(
                    "task {}: task is empty. Describe the job completely.",
                    i + 1
                ));
            }
            let label = t
                .get("name")
                .and_then(|v| v.as_str())
                .map(|n| format!("{role}: {n}"))
                .unwrap_or_else(|| format!("{role} #{}", i + 1));
            jobs.push((label, role.to_string(), task.to_string()));
        }
        let sem = Arc::new(Semaphore::new(limit));
        let total = jobs.len();
        ctx.say(format!("team: {total} task(s), up to {limit} at once"));
        let futs = jobs.into_iter().map(|(label, role, task)| {
            let (sem, runner) = (sem.clone(), runner.clone());
            async move {
                let _permit = sem.acquire().await.ok();
                ctx.say(format!("team ▸ {label} started"));
                let r = runner.run(&role, &task).await;
                ctx.say(format!(
                    "team ■ {label} {}",
                    if r.is_ok() { "done" } else { "failed" }
                ));
                (label, r)
            }
        });
        let results = join_all(futs).await;
        let per = (ctx.output_limit / total).max(1500);
        let mut text = String::new();
        for (label, r) in results {
            let body = match r {
                Ok(t) => clip(t.trim(), per),
                Err(e) => format!("FAILED: {e}"),
            };
            text += &format!("## {label}\n{body}\n\n");
        }
        Ok(Out::new(format!("team ({total})"), text.trim_end()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::permission::{AutoGate, Rules};
    use std::sync::atomic::AtomicUsize;

    struct Fake(AtomicUsize, AtomicUsize);
    #[async_trait]
    impl SubRunner for Fake {
        async fn run(&self, agent: &str, prompt: &str) -> Result<String, String> {
            let now = self.0.fetch_add(1, Ordering::SeqCst) + 1;
            self.1.fetch_max(now, Ordering::SeqCst);
            tokio::time::sleep(std::time::Duration::from_millis(40)).await;
            self.0.fetch_sub(1, Ordering::SeqCst);
            if prompt.contains("boom") {
                Err("exploded".into())
            } else {
                Ok(format!("{agent} did: {prompt}"))
            }
        }
    }

    fn ctx(parallel: usize, fake: Arc<Fake>) -> Ctx {
        let c = Ctx::new(
            std::env::temp_dir(),
            Arc::new(AutoGate(crate::permission::Reply::Once)),
            Rules::default(),
        );
        c.team_parallel.store(parallel, Ordering::Relaxed);
        *lock(&c.sub) = Some(fake);
        c
    }

    #[tokio::test]
    async fn runs_tasks_together_up_to_the_limit_and_reports_each() {
        let fake = Arc::new(Fake(AtomicUsize::new(0), AtomicUsize::new(0)));
        let c = ctx(2, fake.clone());
        let lines = Arc::new(std::sync::Mutex::new(Vec::new()));
        let l2 = lines.clone();
        *lock(&c.notify) = Some(Arc::new(move |s| l2.lock().unwrap().push(s)));
        let args = json!({"tasks":[
            {"role":"researcher","task":"a"},{"role":"developer","task":"b","name":"api"},
            {"role":"tester","task":"boom"},{"role":"pm","task":"d"}]});
        let out = Team.run(&c, args).await.unwrap();
        assert_eq!(
            fake.1.load(Ordering::SeqCst),
            2,
            "never more than parallel at once"
        );
        assert!(out.text.contains("## developer: api\ndeveloper did: b"));
        assert!(out.text.contains("FAILED: exploded"));
        assert!(lines.lock().unwrap().iter().any(|l| l.contains("started")));
    }

    #[tokio::test]
    async fn refuses_when_off_and_on_bad_input() {
        let fake = Arc::new(Fake(AtomicUsize::new(0), AtomicUsize::new(0)));
        let off = ctx(0, fake.clone());
        let e = Team
            .run(&off, json!({"tasks":[{"role":"pm","task":"x"}]}))
            .await
            .unwrap_err();
        assert!(e.0.contains("team mode is off"));
        let on = ctx(2, fake);
        assert!(Team
            .run(&on, json!({"tasks":[{"role":"ceo","task":"x"}]}))
            .await
            .is_err());
        assert!(Team.run(&on, json!({"tasks":[]})).await.is_err());
    }
}
