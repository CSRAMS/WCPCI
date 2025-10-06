use std::collections::HashMap;

use chrono::NaiveDateTime;

use crate::{
    contests::{Contest, Participant},
    db::DbPoolConnection,
    error::prelude::*,
    problems::ProblemCompletion,
};

#[derive(Serialize, Clone, Copy, Debug)]
pub struct ScoreEntry {
    pub id: i64,         // Problem ID
    pub score: i64,      // In Seconds
    pub time_taken: i64, // In Minutes
    pub secs_taken: i64,
    pub num_wrong: i64,
}

impl ScoreEntry {
    pub fn from_completion(
        completion: &ProblemCompletion,
        contest_start: NaiveDateTime,
        contest_penalty_minutes: i64,
    ) -> Self {
        let delta = completion.completed_at.unwrap() - contest_start;
        Self {
            id: completion.problem_id,
            score: delta.num_seconds() + (completion.number_wrong * contest_penalty_minutes * 60),
            time_taken: delta.num_minutes(),
            secs_taken: delta.num_seconds(),
            num_wrong: completion.number_wrong,
        }
    }
}

#[derive(Serialize, Clone, Debug)]
pub struct ParticipantScores {
    contest_start: NaiveDateTime,
    contest_penalty_minutes: i64,
    contest_end: NaiveDateTime,
    contest_freeze: i64,
    pub participant_id: i64,
    pub user_id: i64,
    pub scores: HashMap<i64, ScoreEntry>,
}

impl ParticipantScores {
    async fn get_scores(
        db: &mut DbPoolConnection,
        id: i64,
        contest_start: NaiveDateTime,
        contest_penalty_minutes: i64,
        contest_end: NaiveDateTime,
        contest_freeze: i64,
    ) -> Result<HashMap<i64, ScoreEntry>> {
        let completions = ProblemCompletion::get_for_participant(db, id)
            .await
            .with_context(|| format!("Couldn't score for participant {id}"))?;
        let now = chrono::Utc::now().naive_utc();
        let c = completions
            .into_iter()
            .filter_map(|c| {
                c.completed_at
                    .filter(|c| {
                        c >= &contest_start
                            && c <= &contest_end
                            && (contest_freeze == 0
                                || now >= contest_end
                                || (contest_end - *c).num_minutes() > contest_freeze)
                    })
                    .map(|_| {
                        (
                            c.problem_id,
                            ScoreEntry::from_completion(&c, contest_start, contest_penalty_minutes),
                        )
                    })
            })
            .collect::<HashMap<_, _>>();
        Ok(c)
    }

    pub async fn new(
        db: &mut DbPoolConnection,
        participant: &Participant,
        contest: &Contest,
    ) -> Result<Self> {
        Ok(Self {
            contest_start: contest.start_time,
            contest_penalty_minutes: contest.penalty,
            contest_end: contest.end_time,
            contest_freeze: contest.freeze_time,
            participant_id: participant.p_id,
            user_id: participant.user_id,
            scores: Self::get_scores(
                db,
                participant.p_id,
                contest.start_time,
                contest.penalty,
                contest.end_time,
                contest.freeze_time,
            )
            .await?,
        })
    }

    pub fn process_completion(&mut self, completion: &ProblemCompletion) {
        if completion.participant_id == self.participant_id {
            if let Some(entry) = self.scores.get_mut(&completion.problem_id) {
                if completion.completed_at.is_some() {
                    *entry = ScoreEntry::from_completion(
                        completion,
                        self.contest_start,
                        self.contest_penalty_minutes,
                    );
                } else {
                    self.scores.remove(&completion.problem_id);
                }
            } else if completion.completed_at.is_some() {
                self.scores.insert(
                    completion.problem_id,
                    ScoreEntry::from_completion(
                        completion,
                        self.contest_start,
                        self.contest_penalty_minutes,
                    ),
                );
            }
        }
    }
}

impl Eq for ParticipantScores {}

impl PartialEq for ParticipantScores {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == std::cmp::Ordering::Equal
    }
}

impl PartialOrd for ParticipantScores {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for ParticipantScores {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.scores.len().cmp(&other.scores.len()).reverse().then(
            self.scores
                .values()
                .map(|s| s.score)
                .sum::<i64>()
                .cmp(&other.scores.values().map(|s| s.score).sum()),
        )
    }
}
