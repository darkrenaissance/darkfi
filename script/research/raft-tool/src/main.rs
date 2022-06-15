use std::{fs::File, io::Write};

use darkfi::{
    raft::DataStore,
    util::{
        expand_path,
        serial::{SerialDecodable, SerialEncodable},
    },
    Result,
};

#[derive(Debug)]
struct Log {
    _term: u64,
    _msg: Vec<u8>,
}

#[derive(Debug)]
struct NodeId(Vec<u8>);

#[derive(Debug)]
struct Info<T> {
    _logs: Vec<Log>,
    _commits: Vec<T>,
    _voted_for: Vec<Option<NodeId>>,
    _terms: Vec<u64>,
}

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

#[async_std::main]
async fn main() -> Result<()> {
    // Load datastore, uncomment desired bin structures
    // Taud
    //let db_path = expand_path(&"~/.config/darkfi/tau/tau.db").unwrap();
    //let datastore = DataStore::<EncryptedTask>::new(&db_path.to_str().unwrap())?;
    // Ircd
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
        logs.push(Log { _term: log.term, _msg: log.msg });
    }

    // Commits
    let mut commits = vec![];
    for commit in &sled_commits {
        /*
        commits
            .push(EncryptedTask { nonce: commit.nonce.clone(), payload: commit.payload.clone() });
        */
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

    /*
    let info = Info::<EncryptedTask> {
        _logs: logs,
        _commits: commits,
        _voted_for: voted_for,
        _terms: terms,
    };
    */
    let info =
        Info::<Privmsg> { _logs: logs, _commits: commits, _voted_for: voted_for, _terms: terms };
    let info_string = format!("{:#?}", info);

    // Generating file
    let file_path = "raft_db";
    println!("Data parsed, writing to file {:?}", file_path);
    let mut file = File::create(file_path)?;
    file.write(info_string.as_bytes())?;
    println!("File created!");

    Ok(())
}
