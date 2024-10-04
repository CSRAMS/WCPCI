use sqlx::prelude::FromRow;

use crate::{db::DbPoolConnection, error::prelude::*};

#[derive(Serialize, Clone, FromRow)]
pub struct Team {
    pub id: i64,
    pub name: String,
    pub contest_id: i64,
    pub place: Option<i64>,
}

#[derive(Serialize, Clone)]
pub struct TeamMember {
    pub id: i64,
    pub team_id: i64,
    pub user_id: i64,
    pub is_leader: bool,
}

impl Team {
    pub fn temp(name: String, contest_id: i64) -> Team {
        Team {
            id: 0,
            name,
            contest_id,
            place: None,
        }
    }

    pub async fn by_id(db: &mut DbPoolConnection, id: i64) -> Result<Option<Team>> {
        sqlx::query_as!(
            Team,
            "
            SELECT *
            FROM team
            WHERE id = ?
            ",
            id
        )
        .fetch_optional(&mut **db)
        .await
        .context("Failed to fetch team by id")
    }

    pub async fn create(
        name: String,
        contest_id: i64,
        creating_user_id: i64,
        db: &mut DbPoolConnection,
    ) -> Result<Team> {
        let team = Team::temp(name, contest_id);
        let team = team.save(db).await?;
        team.add_member(db, creating_user_id, true).await?;
        Ok(team)
    }

    pub async fn save(self, db: &mut DbPoolConnection) -> Result<Team> {
        sqlx::query_as!(
            Team,
            "
            INSERT INTO team (name, contest_id, place)
            VALUES (?, ?, ?)
            RETURNING id, name, contest_id, place
            ",
            self.name,
            self.contest_id,
            self.place,
        )
        .fetch_one(&mut **db)
        .await
        .context("Failed to save team")
    }

    pub async fn update(&mut self, db: &mut DbPoolConnection) -> Result {
        sqlx::query!(
            "
            UPDATE team
            SET name = ?
            WHERE id = ?
            ",
            self.name,
            self.id
        )
        .fetch_one(&mut **db)
        .await
        .context("Failed to update team")
        .map(|_| ())
    }

    pub async fn delete(&mut self, db: &mut DbPoolConnection) -> Result {
        sqlx::query!(
            "
            DELETE FROM team
            WHERE id = ?
            ",
            self.id
        )
        .fetch_one(&mut **db)
        .await
        .context("Failed to delete team")
        .map(|_| ())
    }

    pub async fn member(
        &self,
        db: &mut DbPoolConnection,
        user_id: i64,
    ) -> Result<Option<TeamMember>> {
        sqlx::query_as!(
            TeamMember,
            "
            SELECT id, team_id, user_id, is_leader
            FROM team_member
            WHERE team_id = ? AND user_id = ?
            ",
            self.id,
            user_id
        )
        .fetch_optional(&mut **db)
        .await
        .context("Failed to fetch team members")
    }

    pub async fn members(&self, db: &mut DbPoolConnection) -> Result<Vec<TeamMember>> {
        sqlx::query_as!(
            TeamMember,
            "
            SELECT id, team_id, user_id, is_leader
            FROM team_member
            WHERE team_id = ?
            ",
            self.id
        )
        .fetch_all(&mut **db)
        .await
        .context("Failed to fetch team members")
    }

    pub async fn add_member(
        &self,
        db: &mut DbPoolConnection,
        user_id: i64,
        is_leader: bool,
    ) -> Result<TeamMember> {
        let member = TeamMember::temp(self.id, user_id, is_leader);
        member.save(db).await
    }

    async fn delegate_new_leader(
        &self,
        db: &mut DbPoolConnection,
        exclude_user_id: i64,
    ) -> Result<bool> {
        let new_leader = sqlx::query!(
            "
            SELECT user_id
            FROM team_member
            WHERE team_id = ? AND user_id != ?
            LIMIT 1
            ",
            self.id,
            exclude_user_id
        )
        .fetch_optional(&mut **db)
        .await
        .context("Failed to fetch new leader")?;

        if let Some(new_leader) = new_leader {
            sqlx::query!(
                "
                UPDATE team_member
                SET is_leader = 1
                WHERE team_id = ? AND user_id = ?
                ",
                self.id,
                new_leader.user_id
            )
            .fetch_one(&mut **db)
            .await
            .context("Failed to update new leader")?;
            Ok(false)
        } else {
            // No other member in the team, we'll give up as the team will be deleted
            Ok(true)
        }
    }

    pub async fn remove_member(&mut self, db: &mut DbPoolConnection, user_id: i64) -> Result {
        let member = self.member(db, user_id).await?;
        if let Some(member) = member {
            let is_leader = member.is_leader;
            member.delete(db).await?;
            if is_leader {
                let no_more_members = self.delegate_new_leader(db, user_id).await?;
                if no_more_members {
                    self.delete(db).await?;
                }
            }
        }
        Ok(())
    }

    pub async fn list(db: &mut DbPoolConnection, contest_id: i64) -> Result<Vec<Team>> {
        sqlx::query_as!(Team, "SELECT * FROM team WHERE contest_id = ?", contest_id)
            .fetch_all(&mut **db)
            .await
            .context("Failed to fetch all teams for contest")
    }

    pub async fn from_user_and_contest(
        db: &mut DbPoolConnection,
        user_id: i64,
        contest_id: i64,
    ) -> Result<Option<Team>> {
        sqlx::query_as!(
            Team,
            "
            SELECT team.*
            FROM team
            JOIN team_member ON team.id = team_member.team_id
            WHERE team.contest_id = ? AND team_member.user_id = ?
            ",
            contest_id,
            user_id
        )
        .fetch_optional(&mut **db)
        .await
        .context("Failed to fetch team by user and contest")
    }
}

impl TeamMember {
    pub fn temp(team_id: i64, user_id: i64, is_leader: bool) -> TeamMember {
        TeamMember {
            id: 0,
            team_id,
            user_id,
            is_leader,
        }
    }

    pub async fn save(self, db: &mut DbPoolConnection) -> Result<TeamMember> {
        sqlx::query_as!(
            TeamMember,
            "
            INSERT INTO team_member (team_id, user_id, is_leader)
            VALUES (?, ?, ?)
            RETURNING id, team_id, user_id, is_leader
            ",
            self.team_id,
            self.user_id,
            self.is_leader
        )
        .fetch_one(&mut **db)
        .await
        .context("Failed to save team member")
    }

    pub async fn update(&mut self, db: &mut DbPoolConnection) -> Result {
        sqlx::query!(
            "
            UPDATE team_member
            SET is_leader = ?
            WHERE id = ?
            ",
            self.is_leader,
            self.id
        )
        .fetch_one(&mut **db)
        .await
        .context("Failed to update team member")
        .map(|_| ())
    }

    pub async fn delete(self, db: &mut DbPoolConnection) -> Result {
        sqlx::query!(
            "
            DELETE FROM team_member
            WHERE id = ?
            ",
            self.id
        )
        .fetch_one(&mut **db)
        .await
        .context("Failed to delete team member")
        .map(|_| ())
    }
}
