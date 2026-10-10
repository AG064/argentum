use std::collections::{BTreeMap, VecDeque};

use argentum_domain::goal_supervision::{GoalInvocationState, GoalInvocationStatus};
use argentum_domain::{GoalId, RunId, SessionId, TaskId, TaskLifecycle};
use argentum_runtime::{SubmissionTarget, TaskSubmission, TaskSubmissionResult};

pub const MAX_QUEUE_ENTRIES: usize = 16;
pub const MAX_QUEUE_BYTES: usize = 512 * 1024;

#[derive(Clone)]
struct Entry {
    id: TaskId,
    prompt: String,
    goal_id: Option<argentum_domain::GoalId>,
    skill: Option<argentum_workspaces::SelectedSkill>,
}
fn skill_bytes(skill: Option<&argentum_workspaces::SelectedSkill>) -> usize {
    skill.map_or(0, |s| {
        s.name.len()
            + s.description.len()
            + s.relative_path.len()
            + s.base_path.len()
            + s.digest.len()
            + s.body.len()
    })
}

struct InFlight {
    entry: Entry,
    run_id: RunId,
    admitted: bool,
    released: bool,
    terminal: Option<TaskLifecycle>,
    uncertain: bool,
    cancel_requested: bool,
    cancel_sent: bool,
}

#[derive(Default)]
struct SessionQueue {
    pending: VecDeque<Entry>,
    in_flight: Option<InFlight>,
    paused: Option<String>,
    waiting_goal: Option<GoalId>,
    active_invocation: Option<GoalId>,
    settled_invocations: VecDeque<GoalId>,
}

#[derive(Default)]
pub(super) struct FollowUps {
    sessions: BTreeMap<SubmissionTarget, SessionQueue>,
    stopping: bool,
    uncertain_handoff: bool,
}

impl FollowUps {
    pub fn has_in_flight(&self, target: &SubmissionTarget) -> bool {
        self.sessions
            .get(target)
            .is_some_and(|queue| queue.in_flight.is_some())
    }
    pub fn active_run(&self, target: &SubmissionTarget) -> Option<RunId> {
        self.sessions
            .get(target)?
            .in_flight
            .as_ref()
            .filter(|flight| flight.admitted && flight.terminal.is_none())
            .map(|flight| flight.run_id)
    }
    pub fn pause(&mut self, target: &SubmissionTarget, reason: &str) {
        if let Some(queue) = self.sessions.get_mut(target) {
            queue.paused = Some(reason.into());
        }
    }
    pub fn pause_reason(&self, target: &SubmissionTarget) -> Option<&str> {
        self.sessions.get(target)?.paused.as_deref()
    }
    pub fn totals(&self) -> (usize, usize) {
        self.sessions
            .values()
            .fold((0, 0), |(count, bytes), queue| {
                let entries = queue
                    .pending
                    .iter()
                    .chain(queue.in_flight.iter().map(|flight| &flight.entry));
                entries.fold((count, bytes), |(count, bytes), entry| {
                    (
                        count + 1,
                        bytes + entry.prompt.len() + skill_bytes(entry.skill.as_ref()),
                    )
                })
            })
    }

    pub fn pending_count(&self) -> usize {
        self.sessions
            .values()
            .map(|queue| queue.pending.len())
            .sum()
    }

    pub fn retained_count_for_session(&self, session_id: SessionId) -> usize {
        self.sessions
            .iter()
            .filter(|(target, _)| target.session_id == session_id)
            .map(|(_, queue)| queue.pending.len() + usize::from(queue.in_flight.is_some()))
            .sum()
    }

    #[cfg(test)]
    pub fn enqueue(
        &mut self,
        target: SubmissionTarget,
        prompt: String,
    ) -> Result<TaskId, &'static str> {
        self.enqueue_scoped(target, prompt, None)
    }

    #[cfg(test)]
    pub fn enqueue_scoped(
        &mut self,
        target: SubmissionTarget,
        prompt: String,
        goal_id: Option<argentum_domain::GoalId>,
    ) -> Result<TaskId, &'static str> {
        self.enqueue_with_skill(target, prompt, goal_id, None)
    }
    pub fn enqueue_with_skill(
        &mut self,
        target: SubmissionTarget,
        prompt: String,
        goal_id: Option<GoalId>,
        skill: Option<argentum_workspaces::SelectedSkill>,
    ) -> Result<TaskId, &'static str> {
        if self.stopping {
            return Err("Shutdown has started; draft was not accepted");
        }
        if prompt.trim().is_empty() {
            return Err("Enter a nonempty prompt");
        }
        if prompt.len() > super::MAX_COMPOSER_BYTES {
            return Err("Prompt exceeds the 128 KiB input limit");
        }
        let (count, bytes) = self.totals();
        if count >= MAX_QUEUE_ENTRIES {
            return Err("Queue limit: 16 retained submissions; newest draft was not accepted");
        }
        if bytes + prompt.len() + skill_bytes(skill.as_ref()) > MAX_QUEUE_BYTES {
            return Err("Queue byte limit: 512 KiB retained; newest draft was not accepted");
        }
        let id = TaskId::new_v4();
        self.sessions
            .entry(target)
            .or_default()
            .pending
            .push_back(Entry {
                id,
                prompt,
                goal_id,
                skill,
            });
        Ok(id)
    }

    pub fn goal_for_run(&self, run_id: RunId) -> Option<argentum_domain::GoalId> {
        self.sessions
            .values()
            .filter_map(|queue| queue.in_flight.as_ref())
            .find(|flight| flight.run_id == run_id)
            .and_then(|flight| flight.entry.goal_id)
    }

    pub fn claim(&mut self, target: &SubmissionTarget) -> Option<TaskSubmission> {
        if self.stopping || self.uncertain_handoff {
            return None;
        }
        let queue = self.sessions.get_mut(target)?;
        if queue.paused.is_some() || queue.in_flight.is_some() {
            return None;
        }
        let entry = queue.pending.pop_front()?;
        let run_id = RunId::new_v4();
        let submission = TaskSubmission {
            id: entry.id,
            run_id,
            target: target.clone(),
            prompt: entry.prompt.clone(),
            skill: entry.skill.clone(),
            answer_contract: None,
        };
        queue.in_flight = Some(InFlight {
            entry,
            run_id,
            admitted: false,
            released: false,
            terminal: None,
            uncertain: false,
            cancel_requested: false,
            cancel_sent: false,
        });
        Some(submission)
    }

    pub fn target_for_run(&self, run_id: RunId) -> Option<SubmissionTarget> {
        self.sessions.iter().find_map(|(target, queue)| {
            queue
                .in_flight
                .as_ref()
                .filter(|flight| flight.run_id == run_id)
                .map(|_| target.clone())
        })
    }

    pub fn admitted(&mut self, id: TaskId, session_id: argentum_domain::SessionId) {
        for (target, queue) in &mut self.sessions {
            if target.session_id == session_id {
                if let Some(flight) = queue
                    .in_flight
                    .as_mut()
                    .filter(|flight| flight.entry.id == id)
                {
                    flight.admitted = true;
                }
            }
        }
    }

    pub fn terminal(&mut self, run_id: RunId, lifecycle: TaskLifecycle) {
        if !lifecycle.is_terminal() {
            return;
        }
        let Some(target) = self.target_for_run(run_id) else {
            return;
        };
        let queue = self.sessions.get_mut(&target).expect("located queue");
        let flight = queue.in_flight.as_mut().expect("located flight");
        if flight.terminal.is_some() {
            return;
        }
        flight.terminal = Some(lifecycle);
        if flight.entry.goal_id.is_some() {
            queue.waiting_goal = flight.entry.goal_id;
            queue.paused = Some(
                "Goal attempt settled; use /queue continue for explicit admission of later work"
                    .into(),
            );
        } else if lifecycle != TaskLifecycle::Complete {
            queue.paused = Some(format!(
                "Previous task {}; use /queue continue",
                lifecycle.label().to_lowercase()
            ));
        }
        Self::finish_confirmed(queue);
        self.prune_empty();
    }

    pub fn goal_invocation(&mut self, session_id: SessionId, status: &GoalInvocationStatus) {
        for (target, queue) in &mut self.sessions {
            if target.session_id != session_id || queue.waiting_goal != Some(status.goal_id) {
                continue;
            }
            match status.state {
                GoalInvocationState::Active => {
                    if queue.settled_invocations.contains(&status.id) {
                        continue;
                    }
                    match queue.active_invocation {
                        None => {
                            queue.active_invocation = Some(status.id);
                            if queue.paused.as_deref().is_some_and(is_goal_hold_pause) {
                                queue.paused = Some(
                                    "Goal supervision active; dependent work remains paused".into(),
                                );
                            }
                        }
                        Some(id) if id == status.id => {}
                        Some(_) => {
                            queue.paused = Some(
                                "Conflicting goal invocation evidence; dependent work remains paused"
                                    .into(),
                            );
                        }
                    }
                }
                state if state.is_terminal() => {
                    if queue.settled_invocations.contains(&status.id) {
                        continue;
                    }
                    if queue.active_invocation.is_none() && !queue.settled_invocations.is_empty() {
                        continue;
                    }
                    if queue
                        .active_invocation
                        .is_some_and(|invocation_id| invocation_id != status.id)
                    {
                        continue;
                    }
                    queue.active_invocation = None;
                    queue.settled_invocations.push_back(status.id);
                    if queue.settled_invocations.len() > MAX_QUEUE_ENTRIES {
                        queue.settled_invocations.pop_front();
                    }
                    if state == GoalInvocationState::Verified {
                        queue.waiting_goal = None;
                        if queue.paused.as_deref().is_some_and(is_goal_hold_pause) {
                            queue.paused = None;
                        }
                    } else {
                        queue.paused = Some(format!(
                            "Goal supervision {}; use /queue continue for explicit admission of later work",
                            status
                                .stop_reason
                                .as_deref()
                                .unwrap_or("did_not_verify")
                        ));
                    }
                }
                _ => {}
            }
        }
        self.prune_empty();
    }

    pub fn completed(&mut self, completion: TaskSubmissionResult) -> Option<String> {
        let target = self.target_for_run(completion.run_id)?;
        let queue = self.sessions.get_mut(&target)?;
        if queue.in_flight.as_ref()?.entry.id != completion.id {
            return None;
        }
        let mut flight = queue.in_flight.take()?;
        let notice;
        if !completion.admitted {
            if flight.admitted || flight.terminal.is_some() {
                flight.uncertain = true;
                queue.in_flight = Some(flight);
                queue.paused =
                    Some("Conflicting handoff evidence; submission will not be retried".into());
            } else {
                queue.pending.push_front(flight.entry);
                queue.paused = Some(format!(
                    "Not admitted: {}; item retained. Use /queue continue to retry",
                    completion
                        .result
                        .err()
                        .map(|error| error.to_string())
                        .unwrap_or_else(|| "missing admission".into())
                ));
            }
            notice = queue.paused.clone();
        } else if let Err(error) = completion.result {
            queue.paused = Some(format!(
                "Submitted {} failed: {}; later items paused",
                completion.id, error
            ));
            notice = queue.paused.clone();
        } else {
            flight.admitted = true;
            flight.released = true;
            flight.uncertain = false;
            queue.in_flight = Some(flight);
            Self::finish_confirmed(queue);
            notice = queue.paused.clone();
        }
        self.prune_empty();
        notice
    }

    fn finish_confirmed(queue: &mut SessionQueue) {
        if queue
            .in_flight
            .as_ref()
            .is_some_and(|flight| flight.released && flight.terminal.is_some())
        {
            queue.in_flight = None;
        }
    }

    fn prune_empty(&mut self) {
        // An empty queue has no dependent instructions to resume. A fresh manual
        // submission is a new intent; pauses remain while any retained work exists.
        self.sessions
            .retain(|_, queue| queue.in_flight.is_some() || !queue.pending.is_empty());
    }

    pub fn pause_all(&mut self, reason: &str) {
        for queue in self.sessions.values_mut() {
            queue.paused = Some(reason.into());
        }
    }

    pub fn uncertain(&mut self, reason: &str) {
        self.uncertain_handoff = true;
        for queue in self.sessions.values_mut() {
            queue.paused = Some(reason.into());
            if let Some(flight) = &mut queue.in_flight {
                flight.uncertain = true;
            }
        }
    }

    pub fn continue_selected(&mut self, target: &SubmissionTarget) -> Result<(), &'static str> {
        if self.stopping {
            return Err("Shutdown has started");
        }
        if self.uncertain_handoff {
            return Err("Handoff is uncertain; do not resubmit blindly. Inspect the session before restarting");
        }
        if let Some(queue) = self.sessions.get_mut(target) {
            if queue.active_invocation.is_some() {
                return Err("Goal supervision is active; later work remains paused");
            }
            if queue.in_flight.as_ref().is_some_and(|flight| {
                flight.uncertain || flight.released && flight.terminal.is_none()
            }) {
                return Err(
                    "Submission outcome is unconfirmed; retained instructions will not be retried",
                );
            }
            queue.waiting_goal = None;
            queue.paused = None;
        }
        Ok(())
    }

    pub fn request_cancel(&mut self, target: &SubmissionTarget) {
        if let Some(queue) = self.sessions.get_mut(target) {
            queue.paused =
                Some("Cancellation requested; later items paused. Use /queue continue".into());
            if let Some(flight) = &mut queue.in_flight {
                flight.cancel_requested = true;
            }
        }
    }

    pub fn take_cancel(&mut self, target: &SubmissionTarget) -> Option<RunId> {
        let flight = self.sessions.get_mut(target)?.in_flight.as_mut()?;
        if flight.cancel_requested
            && flight.admitted
            && !flight.cancel_sent
            && flight.terminal.is_none()
        {
            flight.cancel_sent = true;
            Some(flight.run_id)
        } else {
            None
        }
    }

    pub fn discard_pending(&mut self, target: &SubmissionTarget) -> usize {
        let count = self.sessions.get_mut(target).map_or(0, |queue| {
            let count = queue.pending.len();
            queue.pending.clear();
            count
        });
        self.prune_empty();
        count
    }

    pub fn stop(&mut self) -> (usize, usize) {
        self.stopping = true;
        let pending = self.pending_count();
        let in_flight = self
            .sessions
            .values()
            .filter(|queue| queue.in_flight.is_some())
            .count();
        for queue in self.sessions.values_mut() {
            queue.pending.clear();
        }
        (pending, in_flight)
    }

    pub fn describe(&self) -> String {
        let (count, bytes) = self.totals();
        let mut lines = vec![format!("Queue: {} pending, {count}/{MAX_QUEUE_ENTRIES} retained, {bytes}/{MAX_QUEUE_BYTES} UTF-8 bytes (memory only)", self.pending_count())];
        for (target, queue) in &self.sessions {
            lines.push(format!(
                "Session {} at {}",
                target.session_id,
                target.workspace.display()
            ));
            if let Some(reason) = &queue.paused {
                lines.push(format!("Paused: {}", super::bounded_text(reason)));
            }
            if let Some(invocation_id) = queue.active_invocation {
                lines.push(format!("Goal supervision: {invocation_id}"));
            }
            if let Some(flight) = &queue.in_flight {
                lines.push(format!("Current {} run {}: admitted={}, admission_released={}, terminal={:?}, uncertain={}", flight.entry.id, flight.run_id, flight.admitted, flight.released, flight.terminal, flight.uncertain));
            }
            for (position, entry) in queue.pending.iter().enumerate() {
                lines.push(format!(
                    "{}. {} ({} bytes, preview): {}",
                    position + 1,
                    entry.id,
                    entry.prompt.len(),
                    super::bounded_text(&entry.prompt)
                ));
            }
        }
        lines.join("\n")
    }

    #[cfg(test)]
    pub fn pending_prompts(&self, target: &SubmissionTarget) -> Vec<&str> {
        self.sessions
            .get(target)
            .map(|queue| {
                queue
                    .pending
                    .iter()
                    .map(|entry| entry.prompt.as_str())
                    .collect()
            })
            .unwrap_or_default()
    }
    #[cfg(test)]
    pub fn pending_ids(&self, target: &SubmissionTarget) -> Vec<TaskId> {
        self.sessions
            .get(target)
            .map(|queue| queue.pending.iter().map(|entry| entry.id).collect())
            .unwrap_or_default()
    }
    #[cfg(test)]
    pub fn has_active_invocation(&self, target: &SubmissionTarget) -> bool {
        self.sessions
            .get(target)
            .is_some_and(|queue| queue.active_invocation.is_some())
    }
}

fn is_goal_hold_pause(reason: &str) -> bool {
    reason.starts_with("Goal attempt settled;") || reason.starts_with("Goal supervision ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use argentum_domain::goal_supervision::GoalInvocationState;
    use argentum_domain::{now, ProjectId};

    fn invocation_status(
        project_id: ProjectId,
        session_id: SessionId,
        goal_id: GoalId,
        invocation_id: GoalId,
        state: GoalInvocationState,
    ) -> GoalInvocationStatus {
        let started_at = now();
        GoalInvocationStatus {
            version: 1,
            id: invocation_id,
            goal_id,
            project_id,
            session_id,
            workspace_key: "workspace".into(),
            owner_generation: None,
            owner_claim_retired: false,
            definition_revision: GoalId::new_v4(),
            criteria_version: 1,
            accounting_revision: GoalId::new_v4(),
            iteration_limit: 2,
            elapsed_seconds: 60,
            started_at,
            deadline_at: started_at + std::time::Duration::from_secs(60),
            state,
            attempts: Vec::new(),
            evaluated_criteria: Vec::new(),
            progress: Vec::new(),
            delivery: None,
            stop_reason: (state != GoalInvocationState::Active).then(|| "stopped".into()),
            finished_at: (state != GoalInvocationState::Active).then_some(started_at),
        }
    }

    #[test]
    fn late_duplicate_invocation_events_cannot_reopen_or_release_a_goal_hold() {
        let project_id = ProjectId::new_v4();
        let session_id = SessionId::new_v4();
        let goal_id = GoalId::new_v4();
        let invocation_id = GoalId::new_v4();
        let target = SubmissionTarget {
            project_id,
            session_id,
            workspace: std::path::PathBuf::from("workspace"),
        };
        let mut followups = FollowUps::default();
        followups.sessions.insert(
            target.clone(),
            SessionQueue {
                pending: VecDeque::from([Entry {
                    id: TaskId::new_v4(),
                    prompt: "dependent work".into(),
                    goal_id: Some(goal_id),
                    skill: None,
                }]),
                paused: Some("Goal supervision active; dependent work remains paused".into()),
                waiting_goal: Some(goal_id),
                active_invocation: Some(invocation_id),
                ..SessionQueue::default()
            },
        );

        let incomplete = invocation_status(
            project_id,
            session_id,
            goal_id,
            invocation_id,
            GoalInvocationState::Incomplete,
        );
        followups.goal_invocation(session_id, &incomplete);
        assert!(!followups.has_active_invocation(&target));
        assert!(followups.pause_reason(&target).is_some());

        let late_active = invocation_status(
            project_id,
            session_id,
            goal_id,
            invocation_id,
            GoalInvocationState::Active,
        );
        followups.goal_invocation(session_id, &late_active);
        followups.goal_invocation(session_id, &incomplete);
        assert!(!followups.has_active_invocation(&target));
        assert!(followups.pause_reason(&target).is_some());

        let unrelated_verified = invocation_status(
            project_id,
            session_id,
            goal_id,
            GoalId::new_v4(),
            GoalInvocationState::Verified,
        );
        followups.goal_invocation(session_id, &unrelated_verified);
        assert!(followups.pause_reason(&target).is_some());
        assert_eq!(followups.pending_prompts(&target), ["dependent work"]);
    }
}
