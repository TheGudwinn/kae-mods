use postgresql_embedded::{PostgreSQL, Settings};
use sha2::{Digest, Sha384};
use sqlx::postgres::{PgPool, PgPoolOptions};
use std::path::Path;

const DB_NAME: &str = "deadlock_mm";

#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error("embedded postgres error: {0}")]
    Embedded(String),
    #[error("database error: {0}")]
    Sql(#[from] sqlx::Error),
    #[error("migration error: {0}")]
    Migrate(#[from] sqlx::migrate::MigrateError),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

/// True when `applied` is the checksum of `sql` with other line endings.
fn differs_only_in_line_endings(sql: &str, applied: &[u8]) -> bool {
    let lf = sql.replace("\r\n", "\n");
    let crlf = lf.replace('\n', "\r\n");
    [lf, crlf].iter().any(|text| Sha384::digest(text.as_bytes()).as_slice() == applied)
}

/// sqlx checksums the raw bytes of a migration file, so a build made from a CRLF checkout rejects a database
/// created by an LF build (and the other way round). Accept that one difference before running the migrations;
/// any real change to an applied migration still fails.
async fn migrate(pool: &PgPool) -> Result<(), DbError> {
    let migrator = sqlx::migrate!("./migrations");
    let has_table: bool = sqlx::query_scalar("SELECT to_regclass('_sqlx_migrations') IS NOT NULL").fetch_one(pool).await?;
    if has_table {
        let applied: Vec<(i64, Vec<u8>)> = sqlx::query_as("SELECT version, checksum FROM _sqlx_migrations").fetch_all(pool).await?;
        for (version, checksum) in applied {
            let Some(m) = migrator.iter().find(|m| m.version == version) else { continue };
            if m.checksum.as_ref() != checksum.as_slice() && differs_only_in_line_endings(m.sql.as_str(), &checksum) {
                sqlx::query("UPDATE _sqlx_migrations SET checksum = $1 WHERE version = $2")
                    .bind(m.checksum.to_vec())
                    .bind(version)
                    .execute(pool)
                    .await?;
            }
        }
    }
    migrator.run(pool).await?;
    Ok(())
}

/// Local PostgreSQL started by the app itself (no external server, no admin rights).
/// Binaries are bundled at build time, so first start needs no internet.
pub struct Db {
    pub pool: PgPool,
    pg: PostgreSQL,
}

impl Db {
    pub async fn start(app_dir: &Path) -> Result<Self, DbError> {
        let pg_dir = app_dir.join("postgres");
        std::fs::create_dir_all(&pg_dir)?;

        // Stable password across runs, stored next to the data (local-only listener).
        let pw_file = pg_dir.join("password");
        let password = match std::fs::read_to_string(&pw_file) {
            Ok(p) if !p.trim().is_empty() => p.trim().to_string(),
            _ => {
                let p = hex::encode(rand_bytes());
                std::fs::write(&pw_file, &p)?;
                p
            }
        };

        let settings = Settings {
            installation_dir: pg_dir.join("bin"),
            data_dir: pg_dir.join("data"),
            password_file: pg_dir.join("pgpass"),
            host: "127.0.0.1".into(),
            port: 0, // pick a free port on every start
            password,
            temporary: false,
            timeout: Some(std::time::Duration::from_secs(120)), // initdb is slow on Windows with antivirus
            ..Settings::default()
        };

        // A server left behind by a run that was killed (crash, forced quit, update restart) keeps the data
        // folder locked: starting a second one then waits for its full timeout and fails. Use the live one
        // when it answers, else clear it away.
        if let Some((pid, port)) = leftover_server(&pg_dir.join("data")).await {
            let url = format!("postgres://{}:{}@127.0.0.1:{port}/{DB_NAME}", settings.username, settings.password);
            let connect = PgPoolOptions::new().max_connections(5).acquire_timeout(std::time::Duration::from_secs(3)).connect(&url);
            if let Ok(pool) = connect.await {
                migrate(&pool).await?;
                return Ok(Self { pool, pg: PostgreSQL::new(settings) });
            }
            let _ = std::process::Command::new("taskkill").args(["/PID", &pid.to_string(), "/T", "/F"]).creation_flags_hidden().output();
        }

        let mut pg = PostgreSQL::new(settings);
        pg.setup().await.map_err(|e| DbError::Embedded(e.to_string()))?;
        pg.start().await.map_err(|e| DbError::Embedded(e.to_string()))?;
        if !pg.database_exists(DB_NAME).await.map_err(|e| DbError::Embedded(e.to_string()))? {
            pg.create_database(DB_NAME).await.map_err(|e| DbError::Embedded(e.to_string()))?;
        }

        let pool = PgPoolOptions::new().max_connections(5).connect(&pg.settings().url(DB_NAME)).await?;
        migrate(&pool).await?;
        Ok(Self { pool, pg })
    }

    /// Stop the server without consuming the handle (used on app exit).
    pub async fn shutdown(&self) -> Result<(), DbError> {
        self.pool.close().await;
        self.pg.stop().await.map_err(|e| DbError::Embedded(e.to_string()))
    }

    pub async fn stop(self) -> Result<(), DbError> {
        self.pool.close().await;
        self.pg.stop().await.map_err(|e| DbError::Embedded(e.to_string()))
    }
}

/// The port of a server still running on `data`, per its `postmaster.pid`. A pid file whose process is gone
/// is ignored (the server's own start-up clears it); a process that is alive but unusable is stopped.
async fn leftover_server(data: &Path) -> Option<(u32, u16)> {
    let text = std::fs::read_to_string(data.join("postmaster.pid")).ok()?;
    let mut lines = text.lines();
    let pid: u32 = lines.next()?.trim().parse().ok()?;
    let port: u16 = lines.nth(2)?.trim().parse().ok()?;
    let alive = tokio::task::spawn_blocking(move || {
        std::process::Command::new("tasklist").args(["/FI", &format!("PID eq {pid}"), "/NH"]).creation_flags_hidden().output()
            .map(|o| String::from_utf8_lossy(&o.stdout).contains(&pid.to_string()))
            .unwrap_or(false)
    })
    .await
    .unwrap_or(false);
    alive.then_some((pid, port))
}

trait HiddenWindow {
    fn creation_flags_hidden(&mut self) -> &mut Self;
}
impl HiddenWindow for std::process::Command {
    #[cfg(windows)]
    fn creation_flags_hidden(&mut self) -> &mut Self {
        use std::os::windows::process::CommandExt;
        self.creation_flags(0x0800_0000)
    }
    #[cfg(not(windows))]
    fn creation_flags_hidden(&mut self) -> &mut Self {
        self
    }
}

fn rand_bytes() -> [u8; 16] {
    // Not cryptographic secrecy against local users; the server only listens on 127.0.0.1.
    use std::time::{SystemTime, UNIX_EPOCH};
    let n = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let mut out = [0u8; 16];
    out.copy_from_slice(&(n ^ (std::process::id() as u128) << 64).to_le_bytes());
    out
}

#[cfg(test)]
mod line_ending_tests {
    use super::*;

    #[test]
    fn accepts_only_a_line_ending_difference() {
        let lf = "CREATE TABLE a (\n    id INT\n);\n";
        let crlf_sum = Sha384::digest(lf.replace('\n', "\r\n").as_bytes());
        assert!(differs_only_in_line_endings(lf, crlf_sum.as_slice()));
        assert!(!differs_only_in_line_endings("CREATE TABLE b ();\n", crlf_sum.as_slice()));
    }
}
