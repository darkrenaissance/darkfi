/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2023 Dyne.org foundation
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as
 * published by the Free Software Foundation, either version 3 of the
 * License, or (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU Affero General Public License for more details.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

use std::process::exit;

use clap::{IntoApp, Parser, Subcommand};
use prettytable::{format, row, Table};
use url::Url;

use darkfi::{rpc::client::RpcClient, Result};

mod rpc;

#[derive(Subcommand)]
pub enum CliDaoSubCommands {
    /// Create DAO
    Create {
        /// Minium number of governance tokens a user must have to propose a vote.
        dao_proposer_limit: u64,

        /// Minimum number of governance tokens staked on a proposal for it to pass.
        dao_quorum: u64,

        /// Quotient value of minimum vote ratio of yes:no votes required for a proposal to pass.
        dao_approval_ratio_quot: u64,

        /// Base value of minimum vote ratio of yes:no votes required for a proposal to pass.
        dao_approval_ratio_base: u64,
    },
    /// Get DAO public address.
    Addr {},
    /// Get votes on current proposal in the form of [[true/false, user's GOV_tokens],[...]].
    GetVotes {},
    /// Get proposals in the form of [[destination, amount, token_id], [...]].
    GetProposals {},
    /// Mint tokens.
    Mint {
        /// Number of treasury tokens to mint.
        token_supply: u64,

        /// Public key of the DAO treasury.
        dao_addr: String,
    },
    /// Get user balance.
    UserBalance {
        /// User public address.
        addr: String,
    },
    /// Get DAO treasury balance.
    DaoBalance {},
    /// Get DAO bulla.
    DaoBulla {},
    /// Generate a new PublicKey.
    Keygen {},
    /// Airdrop tokens given recipient address and value.
    Airdrop {
        /// Airdrop recipient address.
        addr: String,
        /// Value to be airdropped.
        value: u64,
    },
    /// Create a Proposal.
    Propose {
        /// Sender PublicKey.
        sender: String,
        /// Recipient PublicKey.
        recipient: String,
        /// Amount of tokens to be sent.
        amount: u64,
    },
    /// Vote
    Vote {
        /// Voter's public address.
        addr: String,
        /// Vote value [yes/no].
        vote: String,
    },
    /// Execute proposal bulla.
    Exec {
        /// Bulla.
        bulla: String,
    },
}

/// DAO cli
#[derive(Parser)]
#[clap(name = "dao")]
#[clap(arg_required_else_help(true))]
pub struct CliDao {
    /// Increase verbosity
    #[clap(short, parse(from_occurrences))]
    pub verbose: u8,
    #[clap(subcommand)]
    pub command: Option<CliDaoSubCommands>,
}

pub struct Rpc {
    client: RpcClient,
}

async fn start(options: CliDao) -> Result<()> {
    let rpc_addr = "tcp://127.0.0.1:7777";
    let client = Rpc { client: RpcClient::new(Url::parse(rpc_addr)?).await? };
    match options.command {
        Some(CliDaoSubCommands::Create {
            dao_proposer_limit,
            dao_quorum,
            dao_approval_ratio_base,
            dao_approval_ratio_quot,
        }) => {
            let reply = client
                .create(
                    dao_proposer_limit,
                    dao_quorum,
                    dao_approval_ratio_quot,
                    dao_approval_ratio_base,
                )
                .await?;
            println!("Created DAO bulla: {}", &reply.to_string());
            return Ok(())
        }
        Some(CliDaoSubCommands::Addr {}) => {
            let reply = client.addr().await?;
            println!("DAO public address: {}", &reply.to_string());
            return Ok(())
        }
        Some(CliDaoSubCommands::GetVotes {}) => {
            let reply = client.get_votes().await?;
            println!("Votes on current proposals: {}", &reply.to_string());
            return Ok(())
        }
        Some(CliDaoSubCommands::GetProposals {}) => {
            let reply = client.get_proposals().await?;
            println!("Current proposals: {}", &reply.to_string());
            return Ok(())
        }
        Some(CliDaoSubCommands::Mint { token_supply, dao_addr }) => {
            let reply = client.mint(token_supply, dao_addr).await?;
            println!("{}", &reply.as_str().unwrap().to_string());
            return Ok(())
        }
        Some(CliDaoSubCommands::Keygen {}) => {
            let reply = client.keygen().await?;
            println!("User public key: {}", &reply.to_string());
            return Ok(())
        }
        Some(CliDaoSubCommands::Airdrop { addr, value }) => {
            println!("Requesting airdrop of {} GOV", value);

            let reply = client.airdrop(addr, value).await?;
            println!("{}", &reply.as_str().unwrap().to_string());

            return Ok(())
        }
        Some(CliDaoSubCommands::DaoBalance {}) => {
            let rep = client.dao_balance().await?;

            if !rep.is_object() {
                eprintln!("Invalid balance data received from darkfid RPC endpoint.");
                exit(1);
            }

            let mut table = Table::new();
            table.set_format(*format::consts::FORMAT_NO_BORDER_LINE_SEPARATOR);
            table.set_titles(row!["Token", "Balance"]);

            for i in rep.as_object().unwrap().keys() {
                if let Some(balance) = rep[i].as_u64() {
                    table.add_row(row![i, balance]);
                    continue
                }

                eprintln!("Found invalid balance data for key \"{}\"", i);
            }

            if table.is_empty() {
                println!("No balances.");
            } else {
                println!("{}", table);
            }
            // println!("DAO balance: {}", &reply.to_string());
            return Ok(())
        }
        Some(CliDaoSubCommands::DaoBulla {}) => {
            let reply = client.dao_bulla().await?;
            println!("DAO bulla: {}", &reply.to_string());
            return Ok(())
        }
        Some(CliDaoSubCommands::UserBalance { addr }) => {
            let rep = client.user_balance(addr).await?;

            if !rep.is_object() {
                eprintln!("Invalid balance data received from darkfid RPC endpoint.");
                exit(1);
            }

            let mut table = Table::new();
            table.set_format(*format::consts::FORMAT_NO_BORDER_LINE_SEPARATOR);
            table.set_titles(row!["Token", "Balance"]);

            for i in rep.as_object().unwrap().keys() {
                if let Some(balance) = rep[i].as_u64() {
                    table.add_row(row![i, balance]);
                    continue
                }

                eprintln!("Found invalid balance data for key \"{}\"", i);
            }

            if table.is_empty() {
                println!("No balances.");
            } else {
                println!("{}", table);
            }
            // println!("User balance: {}", &reply.to_string());
            return Ok(())
        }
        Some(CliDaoSubCommands::Propose { sender, recipient, amount }) => {
            let reply = client.propose(sender.clone(), recipient.clone(), amount).await?;
            println!(
                "Proposal bulla: {}\nSender: {}\nRecipient: {}\nAmount: {} DRK",
                &reply.to_string(),
                sender,
                recipient,
                amount
            );
            return Ok(())
        }
        Some(CliDaoSubCommands::Vote { addr, vote }) => {
            let reply = client.vote(addr, vote).await?;
            println!("{}", &reply.to_string());
            return Ok(())
        }
        Some(CliDaoSubCommands::Exec { bulla }) => {
            let reply = client.exec(bulla).await?;
            println!("{}", &reply.to_string());
            return Ok(())
        }
        None => {}
    }

    Ok(())
}

#[async_std::main]
async fn main() -> Result<()> {
    let args = CliDao::parse();
    let _matches = CliDao::command().get_matches();

    //let config_path = if args.config.is_some() {
    //    expand_path(&args.config.clone().unwrap())?
    //} else {
    //    join_config_path(&PathBuf::from("drk.toml"))?
    //};

    // Spawn config file if it's not in place already.
    //spawn_config(&config_path, CONFIG_FILE_CONTENTS)?;

    //let (lvl, conf) = log_config(matches)?;
    //TermLogger::init(lvl, conf, TerminalMode::Mixed, ColorChoice::Auto)?;

    //let config = Config::<DrkConfig>::load(config_path)?;

    start(args).await
}
