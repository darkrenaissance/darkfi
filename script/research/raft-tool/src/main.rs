use std::{fs::File, io::Write};

use darkfi::{
    raft::DataStore,
    util::{
        expand_path,
        serial::{deserialize, SerialDecodable, SerialEncodable},
    },
    Result,
};

#[derive(Debug)]
pub struct Info<T> {
    pub logs: Vec<Log<T>>,
    pub commits: Vec<T>,
    pub voted_for: Vec<Option<NodeId>>,
    pub terms: Vec<u64>,
}

#[derive(Debug)]
pub struct Log<T> {
    pub term: u64,
    pub msg: T,
}

#[derive(Debug)]
pub struct NodeId(Vec<u8>);

#[derive(Debug, SerialEncodable, SerialDecodable)]
struct EncryptedTask {
    nonce: Vec<u8>,
    payload: Vec<u8>,
}

type PrivmsgId = u32;

#[derive(Debug, SerialEncodable, SerialDecodable)]
struct Privmsg {
    id: PrivmsgId,
    nickname: String,
    channel: String,
    message: String,
}

fn extract_taud() -> Result<String> {
    let db_path = expand_path(&"~/.config/darkfi/tau/tau.db").unwrap();
    let datastore = DataStore::<EncryptedTask>::new(&db_path.to_str().unwrap())?;

    println!("Extracting db from: {:?}", db_path);

    // Retrieve all data trees
    let sled_logs = datastore.logs.get_all()?;
    let sled_commits = datastore.commits.get_all()?;
    let sled_voted_for = datastore.voted_for.get_all()?;
    let sled_terms = datastore.current_term.get_all()?;

    // Parse retrieved data trees
    println!("Data extracted, parsing to viewable form...");

    // Logs
    let mut logs = vec![];
    for log in sled_logs {
        logs.push(Log { term: log.term, msg: deserialize(&log.msg)? });
    }

    // Commits
    let mut commits = vec![];
    for commit in &sled_commits {
        commits
            .push(EncryptedTask { nonce: commit.nonce.clone(), payload: commit.payload.clone() });
    }

    // Voted for
    let mut voted_for = vec![];
    for vote in sled_voted_for {
        match vote {
            Some(v) => voted_for.push(Some(NodeId(v.0.clone()))),
            None => voted_for.push(None),
        }
    }

    // Terms
    let mut terms = vec![];
    for term in sled_terms {
        terms.push(term);
    }

    let info = Info::<EncryptedTask> { logs, commits, voted_for, terms };
    let info_string = format!("{:#?}", info);
    Ok(info_string)
}

fn extract_ircd() -> Result<String> {
    let db_path = expand_path(&"~/.config/darkfi/ircd/ircd.db").unwrap();
    let datastore = DataStore::<Privmsg>::new(&db_path.to_str().unwrap())?;
    println!("Extracting db from: {:?}", db_path);

    // Retrieve all data trees
    let sled_logs = datastore.logs.get_all()?;
    let sled_commits = datastore.commits.get_all()?;
    let sled_voted_for = datastore.voted_for.get_all()?;
    let sled_terms = datastore.current_term.get_all()?;

    // Parse retrieved data trees
    println!("Data extracted, parsing to viewable form...");

    // Logs
    let mut logs = vec![];
    for log in sled_logs {
        logs.push(Log { term: log.term, msg: deserialize(&log.msg)? });
    }

    // Commits
    let mut commits = vec![];
    for commit in &sled_commits {
        commits.push(Privmsg {
            id: commit.id,
            nickname: commit.nickname.clone(),
            channel: commit.channel.clone(),
            message: commit.message.clone(),
        });
    }

    // Voted for
    let mut voted_for = vec![];
    for vote in sled_voted_for {
        match vote {
            Some(v) => voted_for.push(Some(NodeId(v.0.clone()))),
            None => voted_for.push(None),
        }
    }

    // Terms
    let mut terms = vec![];
    for term in sled_terms {
        terms.push(term);
    }

    let info = Info::<Privmsg> { logs, commits, voted_for, terms };
    let info_string = format!("{:#?}", info);

    Ok(info_string)
}

#[async_std::main]
async fn main() -> Result<()> {
    let info_string = extract_taud()?;

    // Generating file
    let file_path = "raft_db";
    println!("Data parsed, writing to file {:?}", file_path);
    let mut file = File::create(file_path)?;
    file.write(info_string.as_bytes())?;
    println!("File created!");

    Ok(())
}
