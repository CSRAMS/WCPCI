use std::{collections::HashMap, sync::Arc};

use chrono::NaiveDateTime;
use log::error;
use sqlx::{FromRow, Row};
use tokio::sync::Mutex;

use crate::{
    auth::users::User,
    contests::{Contest, Participant},
    db::DbPoolConnection,
    error::prelude::*,
    problems::ProblemCompletion,
};

use super::scoring::{ParticipantScores, ScoreEntry};

pub struct Leaderboard {
    pub contest: Contest,
    pub scores: Vec<ParticipantScores>,
    pub first_map: HashMap<i64, Option<i64>>,
    last_update: Option<NaiveDateTime>,
    tx: LeaderboardUpdateSender,
}

#[derive(Serialize)]
pub struct LeaderboardEntry {
    pub user: User,
    pub p_id: i64,
    pub scores: HashMap<String, ScoreEntry>,
}

impl Leaderboard {
    pub async fn new(
        db: &mut DbPoolConnection,
        contest: Contest,
    ) -> Result<(Self, LeaderboardUpdateReceiver)> {
        let scores = Self::get_scores(db, &contest).await?;
        let first_map = Self::get_first(db, &scores, &contest).await?;
        let (tx, rx) = tokio::sync::broadcast::channel(16);
        Ok((
            Self {
                contest,
                scores,
                first_map,
                last_update: None,
                tx,
            },
            rx,
        ))
    }

    pub fn is_frozen(&self) -> bool {
        self.contest.is_frozen()
    }

    fn get_first_person_for_problem(scores: &[ParticipantScores], problem_id: i64) -> Option<i64> {
        scores
            .iter()
            .filter_map(|s| Some((s.participant_id, s.scores.get(&problem_id)?)))
            .min_by_key(|(_, s)| s.secs_taken)
            .map(|(i, _)| i)
    }

    async fn get_first(
        db: &mut DbPoolConnection,
        scores: &[ParticipantScores],
        contest: &Contest,
    ) -> Result<HashMap<i64, Option<i64>>> {
        let problems = sqlx::query!("SELECT id FROM problem WHERE contest_id = ?", contest.id)
            .fetch_all(&mut **db)
            .await?;
        Ok(problems
            .into_iter()
            .map(|p| (p.id, Self::get_first_person_for_problem(scores, p.id)))
            .collect())
    }

    async fn get_scores(
        db: &mut DbPoolConnection,
        contest: &Contest,
    ) -> Result<Vec<ParticipantScores>> {
        let participants = Participant::list_not_judge(db, contest.id)
            .await
            .context("Failed to get participants for leaderboard")?;
        let mut scores = Vec::new();
        for p in participants {
            scores.push(ParticipantScores::new(db, &p, contest).await?);
        }
        scores.sort();
        Ok(scores)
    }

    fn send_msg(&self, msg: LeaderboardUpdateMessage) {
        if self.is_frozen() {
            return;
        }
        if let Err(why) = self.tx.send(msg) {
            error!("Failed to send leaderboard update: {:?}", why);
        }
    }

    pub async fn full(&mut self, db: &mut DbPoolConnection) -> Result<Vec<LeaderboardEntry>> {
        let now = chrono::Utc::now().naive_utc();
        if self
            .last_update
            .map(|lu| now > self.contest.end_time && lu < self.contest.end_time)
            .unwrap_or(true)
        {
            self.full_refresh(db, None).await?;
        } else {
            self.last_update = Some(now);
        }
        let cases = self
            .scores
            .iter()
            .enumerate()
            .map(|(i, s)| format!("WHEN {} THEN {}", s.participant_id, i))
            .collect::<Vec<_>>()
            .join(" ");
        let scores = self
            .scores
            .iter()
            .map(|s| (s.user_id, s.scores.clone()))
            .collect::<HashMap<_, _>>();
        let query = format!(
            "
            SELECT user.*, participant.p_id FROM participant 
            JOIN user ON participant.user_id = user.id 
            WHERE contest_id = ? AND is_judge = false
            ORDER BY CASE participant.p_id {} ELSE 0 END;
        ",
            if cases.is_empty() {
                "WHEN 0 THEN 0"
            } else {
                &cases
            }
        );
        let res = sqlx::query(query.trim())
            .bind(self.contest.id)
            .fetch_all(&mut **db)
            .await
            .unwrap();
        let res = res
            .into_iter()
            .map(|row| {
                let p_id = row.try_get::<i64, _>("p_id").unwrap();
                let user = User::from_row(&row).unwrap();
                let scores = scores.get(&user.id);
                LeaderboardEntry {
                    user,
                    p_id,
                    scores: scores.map_or(HashMap::new(), |s| {
                        s.clone()
                            .into_iter()
                            .map(|(k, v)| (k.to_string(), v))
                            .collect::<HashMap<_, _>>()
                    }),
                }
            })
            .collect::<Vec<_>>();
        Ok(res)
    }

    pub fn process_completion(&mut self, completion: &ProblemCompletion) {
        if self.is_frozen() {
            return;
        }

        let original_order = self
            .scores
            .iter()
            .enumerate()
            .map(|(i, s)| (s.participant_id, i))
            .collect::<HashMap<_, _>>();

        if let Some(participant) = self
            .scores
            .iter_mut()
            .find(|s| s.participant_id == completion.participant_id)
        {
            participant.process_completion(completion);
            self.scores.sort();
            if completion.completed_at.is_some() {
                self.send_msg(LeaderboardUpdateMessage::Completion {
                    participant_id: completion.participant_id,
                    score: ScoreEntry::from_completion(
                        completion,
                        self.contest.start_time,
                        self.contest.penalty,
                    ),
                });
            } else {
                self.send_msg(LeaderboardUpdateMessage::UnComplete {
                    participant_id: completion.participant_id,
                    problem_id: completion.problem_id,
                });
            }
        }

        let current_first = self
            .first_map
            .get(&completion.problem_id)
            .copied()
            .flatten();
        let new_first = Self::get_first_person_for_problem(&self.scores, completion.problem_id);
        self.first_map.insert(completion.problem_id, new_first);
        if new_first != current_first {
            if let Some(new_first) = new_first {
                self.send_msg(LeaderboardUpdateMessage::CompletedFirst {
                    participant_id: new_first,
                    problem_id: completion.problem_id,
                    is_first: true,
                });
            }
            if let Some(current_first) = current_first {
                self.send_msg(LeaderboardUpdateMessage::CompletedFirst {
                    participant_id: current_first,
                    problem_id: completion.problem_id,
                    is_first: false,
                });
            }
        }

        let new_order = self
            .scores
            .iter()
            .enumerate()
            .map(|(i, s)| (s.participant_id, i))
            .collect::<HashMap<_, _>>();

        let participant_map = original_order
            .iter()
            .map(|(k, v)| (*k, (*v, new_order[k])))
            .collect::<HashMap<_, _>>();

        self.send_msg(LeaderboardUpdateMessage::ReOrder { participant_map });
    }

    pub fn remove_user(&mut self, user_id: i64) {
        self.scores.retain(|s| s.user_id != user_id);
        self.send_msg(LeaderboardUpdateMessage::FullRefresh);
    }

    pub fn remove_participant(&mut self, participant_id: i64) {
        self.scores.retain(|s| s.participant_id != participant_id);
        self.send_msg(LeaderboardUpdateMessage::FullRefresh);
    }

    pub fn stats_of(&self, user_id: i64) -> Option<(usize, usize)> {
        self.scores.iter().enumerate().find_map(|(i, s)| {
            if s.user_id == user_id {
                Some((s.scores.len(), i + 1))
            } else {
                None
            }
        })
    }

    pub async fn full_refresh(
        &mut self,
        db: &mut DbPoolConnection,
        contest: Option<&Contest>,
    ) -> Result {
        if let Some(c) = contest {
            self.contest = c.clone();
        }
        self.scores = Self::get_scores(db, &self.contest).await?;
        self.first_map = Self::get_first(db, &self.scores, &self.contest).await?;
        self.tx.send(LeaderboardUpdateMessage::FullRefresh)?;
        Ok(())
    }
}

#[derive(Serialize, Clone, Debug)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum LeaderboardUpdateMessage {
    FullRefresh,
    #[serde(rename_all = "camelCase")]
    UnComplete {
        participant_id: i64,
        problem_id: i64,
    },
    #[serde(rename_all = "camelCase")]
    Completion {
        participant_id: i64,
        score: ScoreEntry,
    },
    #[serde(rename_all = "camelCase")]
    CompletedFirst {
        participant_id: i64,
        problem_id: i64,
        is_first: bool,
    },
    #[serde(rename_all = "camelCase")]
    ReOrder {
        participant_map: HashMap<i64, (usize, usize)>,
    },
}

pub type LeaderboardUpdateSender = tokio::sync::broadcast::Sender<LeaderboardUpdateMessage>;
pub type LeaderboardUpdateReceiver = tokio::sync::broadcast::Receiver<LeaderboardUpdateMessage>;

pub struct LeaderboardManager {
    leaderboards: HashMap<i64, (Arc<Mutex<Leaderboard>>, LeaderboardUpdateReceiver)>,
    shutdown_rx: ShutdownReceiver,
}

pub type ShutdownReceiver = tokio::sync::watch::Receiver<bool>;

impl LeaderboardManager {
    pub async fn new(shutdown_rx: ShutdownReceiver) -> Self {
        Self {
            leaderboards: HashMap::new(),
            shutdown_rx,
        }
    }

    pub async fn get_leaderboard(
        &mut self,
        db: &mut DbPoolConnection,
        contest: &Contest,
    ) -> Result<Arc<Mutex<Leaderboard>>> {
        if let Some((leaderboard, _)) = self.leaderboards.get(&contest.id) {
            Ok(leaderboard.clone())
        } else {
            let (leaderboard, rx) = Leaderboard::new(db, contest.clone())
                .await
                .context("Can't create new leaderboard")?;
            let leaderboard = Arc::new(Mutex::new(leaderboard));
            self.leaderboards
                .insert(contest.id, (leaderboard.clone(), rx));
            Ok(leaderboard)
        }
    }

    pub fn subscribe_shutdown(&self) -> ShutdownReceiver {
        self.shutdown_rx.clone()
    }

    pub async fn subscribe_leaderboard(
        &mut self,
        db: &mut DbPoolConnection,
        contest: &Contest,
    ) -> Result<LeaderboardUpdateReceiver> {
        let leaderboard = self.get_leaderboard(db, contest).await?;
        let leaderboard = leaderboard.lock().await;
        Ok(leaderboard.tx.subscribe())
    }

    pub async fn delete_user(&mut self, user_id: i64) {
        for (leaderboard, _) in self.leaderboards.values() {
            let mut leaderboard = leaderboard.lock().await;
            leaderboard.remove_user(user_id);
        }
    }

    pub async fn delete_participant_for_contest(&mut self, participant_id: i64, contest_id: i64) {
        if let Some((leaderboard, _)) = self.leaderboards.get_mut(&contest_id) {
            let mut leaderboard = leaderboard.lock().await;
            leaderboard.remove_participant(participant_id);
        }
    }

    pub async fn refresh_leaderboard(
        &mut self,
        db: &mut DbPoolConnection,
        contest: &Contest,
    ) -> Result {
        if let Some((leaderboard, _)) = self.leaderboards.get_mut(&contest.id) {
            let mut leaderboard = leaderboard.lock().await;
            leaderboard.full_refresh(db, Some(contest)).await?;
        }
        Ok(())
    }

    pub async fn process_completion(&mut self, completion: &ProblemCompletion, contest: &Contest) {
        if let Some((leaderboard, _)) = self.leaderboards.get_mut(&contest.id) {
            let mut leaderboard = leaderboard.lock().await;
            leaderboard.process_completion(completion);
        }
    }
}

pub type LeaderboardManagerHandle = Arc<Mutex<LeaderboardManager>>;
