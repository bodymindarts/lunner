mod config;

use config::*;
use std::{
    sync::Arc,
    time::{Duration, SystemTime},
};
use tokio::{process::Command, sync::RwLock};
use tokio_postgres::{Client, IsolationLevel, NoTls, Statement};

const CREATE_TABLE: &'static str =
    "CREATE TABLE IF NOT EXISTS lunner ( leader VARCHAR, since TIMESTAMP );";
const SELECT_LEADER: &'static str = "SELECT leader, since, NOW() FROM lunner LIMIT 1;";
const DELETE_LEADER: &'static str = "DELETE FROM lunner;";
const UPDATE_LEADER: &'static str =
    "UPDATE lunner SET leader = $1, since = NOW() WHERE leader = $1;";
const INSERT_LEADER: &'static str = "INSERT INTO lunner (leader, since) VALUES ($1, NOW() );";

const INNER_LOOP_SECONDS: u64 = 10;

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let conf = Config::load()?;
    let state = State::new();
    println!("[lunner] Executing become-standby hook");
    let mut child = Command::new(&conf.hooks.become_standby.cmd)
        .args(&conf.hooks.become_standby.args)
        .spawn()
        .expect("[lunner] Couldn't execute hook");
    tokio::spawn(async move {
        let _ = child.wait().await;
    });
    run_pg_loop(state.clone()).await?;
    let mut inner_loop = tokio::time::interval(Duration::new(INNER_LOOP_SECONDS / 2, 0));
    let mut currently_leader = false;
    loop {
        inner_loop.tick().await;
        let is_leader = state.is_leader().await;
        println!("[lunner] Testing if state has changed");
        if is_leader != currently_leader {
            if is_leader {
                println!("[lunner] Executing become-leader hook");
                let mut child = Command::new(&conf.hooks.become_leader.cmd)
                    .args(&conf.hooks.become_leader.args)
                    .spawn()
                    .expect("[lunner] Couldn't execute hook");
                tokio::spawn(async move {
                    let _ = child.wait().await;
                });
            } else {
                println!("[lunner] Executing become-standby hook");
                let mut child = Command::new(&conf.hooks.become_standby.cmd)
                    .args(&conf.hooks.become_standby.args)
                    .spawn()
                    .expect("[lunner] Couldn't execute hook");
                tokio::spawn(async move {
                    let _ = child.wait().await;
                });
            }
        }
        currently_leader = is_leader;
    }
}

async fn run_pg_loop(state: State) -> Result<(), anyhow::Error> {
    let conf = Config::load()?;
    let leader_timeout = Duration::new(conf.leader_timeout_seconds, 0);

    let (mut client, connection) =
        tokio_postgres::connect(&conf.postgres.connection, NoTls).await?;

    tokio::spawn(async move {
        if let Err(e) = connection.await {
            eprintln!("[lunner] connection error: {}", e);
        }
    });

    client.execute(CREATE_TABLE, &[]).await?;
    let (select, update, delete, insert) = tokio::try_join!(
        client.prepare(SELECT_LEADER),
        client.prepare(UPDATE_LEADER),
        client.prepare(DELETE_LEADER),
        client.prepare(INSERT_LEADER)
    )?;
    tokio::spawn(async move {
        let mut pg_loop = tokio::time::interval(Duration::new(INNER_LOOP_SECONDS, 0));
        loop {
            pg_loop.tick().await;
            if let Err(e) = pg_inner_loop(
                &mut client,
                &conf.id,
                leader_timeout,
                &select,
                &update,
                &delete,
                &insert,
            )
            .await
            {
                eprintln!("[lunner] PG inner loop error {}", e);
            }
            match client.query_one(&select, &[]).await {
                Ok(row) => {
                    let leader: &str = row.get(0);
                    state.set_leader(leader == &conf.id).await;
                }
                Err(e) => {
                    eprintln!("[lunner] Selecting leader failed {}", e);
                    state.set_leader(false).await;
                }
            }
        }
    });
    Ok(())
}
async fn pg_inner_loop(
    client: &mut Client,
    id: &String,
    leader_timeout: Duration,
    select: &Statement,
    update: &Statement,
    delete: &Statement,
    insert: &Statement,
) -> Result<(), anyhow::Error> {
    let tx = client
        .build_transaction()
        .isolation_level(IsolationLevel::Serializable)
        .start()
        .await?;
    println!("[lunner] Checking leader");
    match tx.query_opt(select, &[]).await? {
        None => {
            println!(
                "[lunner] No leader found - inserting self ('{}') as leader",
                id
            );
            let _ = tx.execute(insert, &[id]).await?;
        }
        Some(row) => {
            let leader: &str = row.get(0);
            println!("[lunner] Leader is '{}'", leader);
            if leader == id {
                println!("[lunner] We are leader - updating table");
                tx.execute(update, &[id]).await?;
            } else {
                let since: SystemTime = row.get(1);
                let now: SystemTime = row.get(2);
                match now.duration_since(since) {
                    Ok(n) if n > leader_timeout => {
                        println!("[lunner] Leader has timeoud out - inserting self as leader");
                        tx.execute(delete, &[]).await?;
                        tx.execute(insert, &[id]).await?;
                    }
                    _ => (),
                }
            }
        }
    }
    tx.commit().await?;
    Ok(())
}

#[derive(Clone)]
struct State {
    leader: Arc<RwLock<bool>>,
}
impl State {
    fn new() -> Self {
        Self {
            leader: Arc::new(RwLock::new(false)),
        }
    }
    async fn is_leader(&self) -> bool {
        *self.leader.read().await
    }

    async fn set_leader(&self, leader: bool) -> bool {
        let mut val = self.leader.write().await;
        let changed = leader != *val;
        *val = leader;
        changed
    }
}
