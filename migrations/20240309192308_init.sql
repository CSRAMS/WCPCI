-- NOTE: This file **WILL** change during dev, meaning you will have to run `cargo sqlx database reset -y` sometimes when pulling
-- Migrations will only be used properly later in development, for now, we will just use them to create the initial schema

CREATE TABLE IF NOT EXISTS user (
    id INTEGER PRIMARY KEY NOT NULL,
    sso_id TEXT NOT NULL UNIQUE,
    email TEXT NOT NULL UNIQUE,
    profile_picture_source TEXT NOT NULL DEFAULT 'gravatar',
    bio TEXT NOT NULL DEFAULT '',
    default_display_name TEXT NOT NULL,
    display_name VARCHAR(32),
    default_language TEXT NOT NULL,
    color_scheme TEXT NOT NULL,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP NOT NULL,
    github_id INTEGER UNIQUE,
    google_id TEXT UNIQUE
);

CREATE TABLE IF NOT EXISTS session (
    id INTEGER PRIMARY KEY NOT NULL,
    user_id INTEGER NOT NULL,
    token TEXT NOT NULL UNIQUE,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP NOT NULL,
    expires_at TIMESTAMP NOT NULL,
    FOREIGN KEY (user_id) REFERENCES user(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS contest (
    id INTEGER PRIMARY KEY NOT NULL,
    name VARCHAR(100) NOT NULL,
    description TEXT,
    start_time TIMESTAMP NOT NULL,
    end_time TIMESTAMP NOT NULL,
    freeze_time INTEGER NOT NULL,
    registration_deadline TIMESTAMP NOT NULL,
    penalty INTEGER NOT NULL,
    max_participants INTEGER,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS judge (
    id INTEGER PRIMARY KEY NOT NULL,
    contest_id INTEGER NOT NULL,
    user_id INTEGER NOT NULL,
    FOREIGN KEY (contest_id) REFERENCES contest(id) ON DELETE CASCADE,
    FOREIGN KEY (user_id) REFERENCES user(id) ON DELETE CASCADE
    UNIQUE (contest_id, user_id)
);

CREATE TABLE IF NOT EXISTS team (
    id INTEGER PRIMARY KEY NOT NULL,
    name VARCHAR(100) NOT NULL,
    contest_id INTEGER NOT NULL,
    FOREIGN KEY (contest_id) REFERENCES contest(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS team_member (
    id INTEGER PRIMARY KEY NOT NULL,
    team_id INTEGER NOT NULL,
    user_id INTEGER NOT NULL,
    is_leader BOOLEAN NOT NULL,
    FOREIGN KEY (team_id) REFERENCES team(id) ON DELETE CASCADE,
    FOREIGN KEY (user_id) REFERENCES user(id) ON DELETE CASCADE
    UNIQUE (team_id, user_id)
);

CREATE TABLE IF NOT EXISTS problem (
    id INTEGER PRIMARY KEY NOT NULL,
    contest_id INTEGER NOT NULL,
    name VARCHAR(100) NOT NULL,
    slug VARCHAR(100) NOT NULL,
    description TEXT NOT NULL,
    cpu_time INTEGER NOT NULL CHECK (cpu_time >= 0),
    memory_limit INTEGER NOT NULL CHECK (memory_limit >= 0),
    FOREIGN KEY (contest_id) REFERENCES contest(id) ON DELETE CASCADE
    UNIQUE (contest_id, slug)
);

CREATE TABLE IF NOT EXISTS test_case (
    id INTEGER PRIMARY KEY NOT NULL,
    problem_id INTEGER NOT NULL,
    ord INTEGER NOT NULL,
    stdin TEXT NOT NULL,
    expected_pattern TEXT NOT NULL,
    use_regex BOOLEAN NOT NULL,
    case_insensitive BOOLEAN NOT NULL,
    FOREIGN KEY (problem_id) REFERENCES problem(id) ON DELETE CASCADE
    UNIQUE (problem_id, ord)
);

CREATE TABLE IF NOT EXISTS judge_run (
    id INTEGER PRIMARY KEY NOT NULL,
    problem_id INTEGER NOT NULL,
    user_id INTEGER NOT NULL,
    amount_run INTEGER NOT NULL,
    total_cases INTEGER NOT NULL,
    error TEXT,
    program VARCHAR(100000) NOT NULL,
    language TEXT NOT NULL,
    ran_at TIMESTAMP NOT NULL,
    FOREIGN KEY (problem_id) REFERENCES problem(id) ON DELETE CASCADE,
    FOREIGN KEY (user_id) REFERENCES user(id) ON DELETE CASCADE
    UNIQUE (problem_id, user_id, ran_at)
);

CREATE TABLE IF NOT EXISTS problem_completion (
    problem_id INTEGER NOT NULL,
    team_id INTEGER NOT NULL,
    completed_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    number_wrong INTEGER NOT NULL,
    FOREIGN KEY (problem_id) REFERENCES problem(id) ON DELETE CASCADE,
    FOREIGN KEY (team_id) REFERENCES team(id) ON DELETE CASCADE,
    UNIQUE (problem_id, team_id)
);
