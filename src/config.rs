use anyhow::*;
use serde::Deserialize;
use std::{fs::File, io::BufReader, path::Path};

use structopt::StructOpt;

#[derive(StructOpt, Debug, Clone)]
#[structopt(name = "lunner")]
struct CliOpts {
    #[structopt(short, long, env = "LUNNER_CONF", default_value = "./lunner.yml")]
    config_path: String,
}

#[derive(Debug, Deserialize)]
pub struct Config {
    pub id: String,
    pub leader_timeout_seconds: u64,
    pub postgres: PostgresConf,
    pub hooks: Hooks,
}

impl Config {
    pub fn load() -> Result<Self> {
        let opts = CliOpts::from_args();
        Self::from_file(opts.config_path)
    }

    fn from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let file = File::open(path).context("Couldn't open config file")?;
        let reader = BufReader::new(file);

        let config: Self = serde_yaml::from_reader(reader)?;

        Ok(config)
    }
}

#[derive(Debug, Deserialize)]
pub struct Hooks {
    pub become_leader: Hook,
    pub become_standby: Hook,
}

#[derive(Debug, Deserialize)]
pub struct Hook {
    pub cmd: String,
    pub args: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct PostgresConf {
    pub connection: String,
}
