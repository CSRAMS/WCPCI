use crate::{auth::users::User, db::DbPoolConnection, error::prelude::*};

#[derive(Serialize, Clone)]
pub struct Judge {
    pub id: i64,
    pub contest_id: i64,
    pub user_id: i64,
}

impl Judge {
    pub fn temp(contest_id: i64, user_id: i64) -> Judge {
        Judge {
            id: 0,
            contest_id,
            user_id,
        }
    }

    pub async fn create(contest_id: i64, user_id: i64, db: &mut DbPoolConnection) -> Result<Judge> {
        let judge = Judge::temp(contest_id, user_id);
        judge.save(db).await
    }

    pub async fn all_for_contest(contest_id: i64, db: &mut DbPoolConnection) -> Result<Vec<User>> {
        sqlx::query_as!(
            User,
            "
            SELECT user.* FROM user JOIN judge ON user.id = judge.user_id WHERE judge.contest_id = ?
            ",
            contest_id,
        )
        .fetch_all(&mut **db)
        .await
        .context("Failed to get judge users for contest")
    }

    pub async fn is_judge(
        user_id: i64,
        contest_id: i64,
        db: &mut DbPoolConnection,
    ) -> Result<bool> {
        sqlx::query_as!(
            Judge,
            "
            SELECT * FROM judge WHERE user_id = ? AND contest_id = ?
            ",
            user_id,
            contest_id
        )
        .fetch_optional(&mut **db)
        .await
        .context("Couldn't find judge status of user")
        .map(|o| o.is_some())
    }

    pub async fn for_contest(
        contest_id: i64,
        user_id: i64,
        db: &mut DbPoolConnection,
    ) -> Result<Option<User>> {
        sqlx::query_as!(
            User,
            "
            SELECT user.* FROM user JOIN judge ON user.id = judge.user_id WHERE judge.contest_id = ? AND judge.user_id = ?
            ",
            contest_id,
            user_id
        )
        .fetch_optional(&mut **db)
        .await.context("Failed to get judge user for contest")
    }

    pub async fn save(self, db: &mut DbPoolConnection) -> Result<Judge> {
        sqlx::query_as!(
            Judge,
            "
            INSERT INTO judge (contest_id, user_id)
            VALUES (?, ?)
            RETURNING id, contest_id, user_id
            ",
            self.contest_id,
            self.user_id
        )
        .fetch_one(&mut **db)
        .await
        .context("Failed to save judge")
    }

    pub async fn delete_for_user_and_contest(
        user_id: i64,
        contest_id: i64,
        db: &mut DbPoolConnection,
    ) -> Result {
        sqlx::query!(
            "DELETE FROM judge WHERE user_id = ? AND contest_id = ?",
            user_id,
            contest_id
        )
        .fetch_one(&mut **db)
        .await
        .context("Couldn't delete judge").map(|_| ())
    }

    pub async fn delete(&mut self, db: &mut DbPoolConnection) -> Result {
        sqlx::query!(
            "
            DELETE FROM judge
            WHERE id = ?
            ",
            self.id
        )
        .fetch_one(&mut **db)
        .await
        .context("Failed to delete judge")
        .map(|_| ())
    }
}
