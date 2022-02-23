use std::{collections::HashMap, net::SocketAddr, path::PathBuf, str::FromStr};

use async_executor::Executor;
use async_std::sync::{Arc, Mutex};
use async_trait::async_trait;
use clap::{IntoApp, Parser};
use easy_parallel::Parallel;
use log::{debug, info};
use num_bigint::BigUint;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use simplelog::{ColorChoice, TermLogger, TerminalMode};
use url::Url;

use darkfi::{
    blockchain::{rocks::columns, Rocks, RocksColumn},
    cli::{
        cli_config::{log_config, spawn_config},
        Config,
    },
    crypto::{
        address::Address,
        keypair::{Keypair, PublicKey, SecretKey},
        proof::VerifyingKey,
        token_list::{assign_id, DrkTokenList, TokenList},
        types::DrkTokenId,
    },
    node::{
        client::Client,
        state::{ProgramState, State},
        wallet::walletdb::WalletDb,
    },
    rpc::{
        jsonrpc::{
            error as jsonerr, request as jsonreq, response as jsonresp, send_request, ErrorCode::*,
            JsonRequest, JsonResult,
        },
        rpcserver::{listen_and_serve, RequestHandler, RpcServerConfig},
    },
    util::{decode_base10, encode_base10, expand_path, join_config_path, NetworkName},
    zk::circuit::{MintContract, SpendContract},
    Error, Result,
};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CashierC {
    /// Cashier name
    pub name: String,
    /// The RPC endpoint for a selected cashier
    pub rpc_url: String,
    /// The selected cashier public key
    pub public_key: String,
}

/// The configuration for darkfid
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct DarkfidConfig {
    /// The address where darkfid should bind its RPC socket
    pub rpc_listen_address: SocketAddr,
    /// Whether to listen with TLS or plain TCP
    pub serve_tls: bool,
    /// Path to DER-formatted PKCS#12 archive. (Unused if serve_tls=false)
    pub tls_identity_path: String,
    /// Password for the TLS identity. (Unused if serve_tls=false)
    pub tls_identity_password: String,
    /// The endpoint to a gatewayd protocol API
    pub gateway_protocol_url: String,
    /// The endpoint to a gatewayd publisher API
    pub gateway_publisher_url: String,
    /// Path to the client database
    pub database_path: String,
    /// Path to the wallet database
    pub wallet_path: String,
    /// The wallet password
    pub wallet_password: String,
    /// The configured cashiers to use
    pub cashiers: Vec<CashierC>,
}

/// Darkfid cli
#[derive(Parser)]
#[clap(name = "darkfid")]
pub struct CliDarkfid {
    /// Sets a custom config file
    #[clap(short, long)]
    pub config: Option<String>,
    /// Local cashier public key
    #[clap(long)]
    pub cashier: Option<String>,
    /// Increase verbosity
    #[clap(short, parse(from_occurrences))]
    pub verbose: u8,
    /// Refresh the wallet and slabstore
    #[clap(short, long)]
    pub refresh: bool,
}

const CONFIG_FILE_CONTENTS: &[u8] = include_bytes!("../darkfid_config.toml");

pub const ETH_NATIVE_TOKEN_ID: &str = "0x0000000000000000000000000000000000000000";

#[derive(Clone, Debug)]
pub struct Cashier {
    pub name: String,
    pub rpc_url: Url,
    pub public_key: PublicKey,
}

struct Darkfid {
    client: Arc<Mutex<Client>>,
    state: Arc<Mutex<State>>,
    sol_tokenlist: TokenList,
    eth_tokenlist: TokenList,
    btc_tokenlist: TokenList,
    drk_tokenlist: DrkTokenList,
    cashiers: Vec<Cashier>,
}

#[async_trait]
impl RequestHandler for Darkfid {
    async fn handle_request(&self, req: JsonRequest, _executor: Arc<Executor<'_>>) -> JsonResult {
        if req.params.as_array().is_none() {
            return JsonResult::Err(jsonerr(InvalidParams, None, req.id))
        }

        debug!(target: "RPC", "--> {}", serde_json::to_string(&req).unwrap());

        if self.update_balances().await.is_err() {
            return JsonResult::Err(jsonerr(
                InternalError,
                Some("Unable to update balances".into()),
                req.id,
            ))
        }

        match req.method.as_str() {
            Some("say_hello") => return self.say_hello(req.id, req.params).await,
            Some("create_wallet") => return self.create_wallet(req.id, req.params).await,
            Some("key_gen") => return self.key_gen(req.id, req.params).await,
            Some("get_key") => return self.get_key(req.id, req.params).await,
            Some("get_keys") => return self.get_keys(req.id, req.params).await,
            Some("export_keypair") => return self.export_keypair(req.id, req.params).await,
            Some("import_keypair") => return self.import_keypair(req.id, req.params).await,
            Some("set_default_address") => {
                return self.set_default_address(req.id, req.params).await
            }
            Some("get_balances") => return self.get_balances(req.id, req.params).await,
            Some("get_token_id") => return self.get_token_id(req.id, req.params).await,
            Some("features") => return self.features(req.id, req.params).await,
            Some("deposit") => return self.deposit(req.id, req.params).await,
            Some("withdraw") => return self.withdraw(req.id, req.params).await,
            Some("transfer") => return self.transfer(req.id, req.params).await,
            Some(_) | None => return JsonResult::Err(jsonerr(MethodNotFound, None, req.id)),
        };
    }
}

impl Darkfid {
    async fn new(
        client: Arc<Mutex<Client>>,
        state: Arc<Mutex<State>>,
        cashiers: Vec<Cashier>,
    ) -> Result<Self> {
        let sol_tokenlist =
            TokenList::new(include_bytes!("../../../contrib/token/solana_token_list.json"))?;
        let eth_tokenlist =
            TokenList::new(include_bytes!("../../../contrib/token/erc20_token_list.json"))?;
        let btc_tokenlist =
            TokenList::new(include_bytes!("../../../contrib/token/bitcoin_token_list.json"))?;
        let drk_tokenlist = DrkTokenList::new(&sol_tokenlist, &eth_tokenlist, &btc_tokenlist)?;

        Ok(Self {
            client,
            state,
            sol_tokenlist,
            eth_tokenlist,
            btc_tokenlist,
            drk_tokenlist,
            cashiers,
        })
    }

    async fn start(&mut self, executor: Arc<Executor<'_>>) -> Result<()> {
        self.client.lock().await.start().await?;
        self.client.lock().await.connect_to_subscriber(self.state.clone(), executor).await?;

        Ok(())
    }

    async fn update_balances(&self) -> Result<()> {
        let own_coins = self.client.lock().await.get_own_coins().await?;

        for own_coin in own_coins.iter() {
            let nullifier_exists = self.state.lock().await.nullifier_exists(&own_coin.nullifier);

            if nullifier_exists {
                self.client.lock().await.confirm_spend_coin(&own_coin.coin).await?;
            }
        }

        Ok(())
    }

    // RPCAPI:
    // Returns a `helloworld` string.
    // --> {"jsonrpc": "2.0", "method": "say_hello", "params": [], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": "hello world", "id": 1}
    async fn say_hello(&self, id: Value, _params: Value) -> JsonResult {
        JsonResult::Resp(jsonresp(json!("hello world"), id))
    }

    // RPCAPI:
    // Attempts to initialize a wallet, and returns `true` upon success.
    // --> {"jsonrpc": "2.0", "method": "create_wallet", "params": [], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": true, "id": 1}
    async fn create_wallet(&self, id: Value, _params: Value) -> JsonResult {
        match self.client.lock().await.init_db().await {
            Ok(()) => JsonResult::Resp(jsonresp(json!(true), id)),
            Err(e) => JsonResult::Err(jsonerr(ServerError(-32001), Some(e.to_string()), id)),
        }
    }

    // RPCAPI:
    // Attempts to generate a new keypair and returns `true` upon success.
    // --> {"jsonrpc": "2.0", "method": "key_gen", "params": [], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": true, "id": 1}
    async fn key_gen(&self, id: Value, _params: Value) -> JsonResult {
        let client = self.client.lock().await;
        match client.key_gen().await {
            Ok(()) => JsonResult::Resp(jsonresp(json!(true), id)),
            Err(e) => JsonResult::Err(jsonerr(ServerError(-32002), Some(e.to_string()), id)),
        }
    }

    // RPCAPI:
    // Fetches the main keypair from the wallet and returns it
    // in an encoded format.
    // --> {"jsonrpc": "2.0", "method": "get_key", "params": [], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": "vdNS7oBj7KvsMWWmo9r96SV4SqATLrGsH2a3PGpCfJC", "id": 1}
    async fn get_key(&self, id: Value, _params: Value) -> JsonResult {
        let pk = self.client.lock().await.main_keypair.public;
        let addr = Address::from(pk).to_string();
        JsonResult::Resp(jsonresp(json!(addr), id))
    }

    // RPCAPI:
    // Fetches all keypairs from the wallet and returns a list of them
    // in an encoded format.
    // The first one in the list is the default selected keypair.
    // --> {"jsonrpc": "2.0", "method": "get_keys", "params": [], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": ["vdNS7oBj7KvsMWWmo9r96SV4SqATLrGsH2a3PGpCfJC", "..."], "id": 1}
    async fn get_keys(&self, id: Value, _params: Value) -> JsonResult {
        let result: Result<Vec<String>> = async {
            let keypairs = self.client.lock().await.get_keypairs().await?;
            let default_keypair = self.client.lock().await.main_keypair;

            let mut addresses: Vec<String> = keypairs
                .iter()
                .filter_map(|k| {
                    if *k == default_keypair {
                        return None
                    }
                    Some(Address::from(k.public).to_string())
                })
                .collect();

            addresses.insert(0, Address::from(default_keypair.public).to_string());

            Ok(addresses)
        }
        .await;

        match result {
            Ok(addresses) => JsonResult::Resp(jsonresp(json!(addresses), id)),
            Err(err) => JsonResult::Err(jsonerr(ServerError(-32003), Some(err.to_string()), id)),
        }
    }

    // RPCAPI:
    // Imports a keypair into the wallet with a given path on the filesystem.
    // Returns `true` upon success.
    // --> {"jsonrpc": "2.0", "method": "import_keypair", "params": ["/path"], "id": 1}
    // <-- {"jsonrpc:" "2.0", "result": true, "id": 1}
    async fn import_keypair(&self, id: Value, params: Value) -> JsonResult {
        let args = params.as_array();

        if args.is_none() {
            return JsonResult::Err(jsonerr(InvalidParams, None, id))
        }

        let arg = args.unwrap()[0].clone();

        if arg.as_str().is_none() &&
            expand_path(arg.as_str().unwrap()).is_ok() &&
            expand_path(arg.as_str().unwrap()).unwrap().to_str().is_some()
        {
            return JsonResult::Err(jsonerr(InvalidParams, Some("invalid path".into()), id))
        }

        let path = expand_path(arg.as_str().unwrap()).unwrap();
        let path = path.to_str().unwrap();

        let result: Result<()> = async {
            let keypair_str: String = std::fs::read_to_string(path)?;

            let mut bytes = [0u8; 32];
            let bytes_vec: Vec<u8> = serde_json::from_str(&keypair_str)?;
            bytes.copy_from_slice(bytes_vec.as_slice());

            let secret: SecretKey = SecretKey::from_bytes(bytes)?;
            let public: PublicKey = PublicKey::from_secret(secret);

            self.client.lock().await.put_keypair(&Keypair { secret, public }).await?;
            Ok(())
        }
        .await;

        match result {
            Ok(_) => JsonResult::Resp(jsonresp(json!(true), id)),
            Err(err) => JsonResult::Err(jsonerr(ServerError(-32004), Some(err.to_string()), id)),
        }
    }

    // RPCAPI:
    // Exports the default selected keypair to a given path on the filesystem.
    // Returns `true` upon success.
    // --> {"jsonrpc": "2.0", "method": "export_keypair", "params": ["/path"], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": true, "id": 1}
    async fn export_keypair(&self, id: Value, params: Value) -> JsonResult {
        let args = params.as_array();

        if args.is_none() {
            return JsonResult::Err(jsonerr(InvalidParams, None, id))
        }

        let arg = args.unwrap()[0].clone();

        if arg.as_str().is_none() &&
            expand_path(arg.as_str().unwrap()).is_ok() &&
            expand_path(arg.as_str().unwrap()).unwrap().to_str().is_some()
        {
            return JsonResult::Err(jsonerr(InvalidParams, Some("invalid path".into()), id))
        }

        let path = expand_path(arg.as_str().unwrap()).unwrap();
        let path = path.to_str().unwrap();

        let result: Result<()> = async {
            let keypair: String =
                serde_json::to_string(&self.client.lock().await.main_keypair.secret.to_bytes())?;
            std::fs::write(path, &keypair)?;
            Ok(())
        }
        .await;

        match result {
            Ok(_) => JsonResult::Resp(jsonresp(json!(true), id)),
            Err(err) => JsonResult::Err(jsonerr(ServerError(-32004), Some(err.to_string()), id)),
        }
    }

    // RPCAPI:
    // Sets the default wallet address to the given parameter.
    // Returns true upon success.
    // --> {"jsonrpc": "2.0", "method": "set_default_address", "params": ["vdNS7oBj7KvsMWWmo9r96SV4SqATLrGsH2a3PGpCfJC"], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": true, "id": 1}
    async fn set_default_address(&self, id: Value, params: Value) -> JsonResult {
        let args = params.as_array();

        if args.is_none() && args.unwrap()[0].as_str().is_none() {
            return JsonResult::Err(jsonerr(InvalidParams, None, id))
        }

        let addr_str = args.unwrap()[0].as_str().unwrap();

        let result: Result<()> = async {
            let public = PublicKey::try_from(Address::from_str(addr_str)?)?;
            self.client.lock().await.set_default_keypair(&public).await?;
            Ok(())
        }
        .await;

        match result {
            Ok(_) => JsonResult::Resp(jsonresp(json!(true), id)),
            Err(err) => JsonResult::Err(jsonerr(ServerError(-32005), Some(err.to_string()), id)),
        }
    }

    // RPCAPI:
    // Fetches the known balances from the wallet.
    // Returns a map of balances, indexed by `network`, and token ID.
    // --> {"jsonrpc": "2.0", "method": "get_balances", "params": [], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": [{"btc": [100, "Bitcoin"]}, {...}], "id": 1}
    async fn get_balances(&self, id: Value, _params: Value) -> JsonResult {
        let result: Result<HashMap<String, (String, String)>> = async {
            let balances = self.client.lock().await.get_balances().await?;
            let mut symbols: HashMap<String, (String, String)> = HashMap::new();

            for b in balances.list.iter() {
                let network: String;
                let symbol: String;

                let mut amount = BigUint::from(b.value);

                if let Some((net, sym)) = self.drk_tokenlist.symbol_from_id(&b.token_id)? {
                    network = net.to_string();
                    symbol = sym;
                } else {
                    // TODO: SQL needs to have the mint address for show, not the internal hash.
                    // TODO: SQL needs to have the nework name
                    network = String::from("UNKNOWN");
                    symbol = format!("{:?}", b.token_id);
                }

                if let Some(prev) = symbols.get(&symbol) {
                    let prev_amnt = decode_base10(&prev.0, 8, true)?;
                    amount += prev_amnt;
                }

                let amount = encode_base10(amount, 8);
                symbols.insert(symbol, (amount, network));
            }

            Ok(symbols)
        }
        .await;

        match result {
            Ok(res) => JsonResult::Resp(jsonresp(json!(res), id)),
            Err(err) => JsonResult::Err(jsonerr(InternalError, Some(err.to_string()), json!(id))),
        }
    }

    // RPCAPI:
    // Generates the internal token ID for a given `network` and token ticker or address.
    // Returns the internal representation of the token ID.
    // --> {"jsonrpc": "2.0", "method": "get_token_id", "params": ["network", "token"], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": "Ht5G1RhkcKnpLVLMhqJc5aqZ4wYUEbxbtZwGCVbgU7DL", "id": 1}
    async fn get_token_id(&self, id: Value, params: Value) -> JsonResult {
        let args = params.as_array();

        if args.is_none() {
            return JsonResult::Err(jsonerr(InvalidParams, None, id))
        }

        let args = args.unwrap();

        if args.len() != 2 {
            return JsonResult::Err(jsonerr(InvalidParams, None, id))
        }

        let network: &str;
        let symbol: &str;

        match (args[0].as_str(), args[1].as_str()) {
            (Some(net), Some(sym)) => {
                network = net;
                symbol = sym;
            }
            (None, _) => return JsonResult::Err(jsonerr(InvalidNetworkParam, None, id)),
            (_, None) => return JsonResult::Err(jsonerr(InvalidSymbolParam, None, id)),
        }

        let result: Result<Value> = async {
            let network = NetworkName::from_str(network)?;
            match network {
                NetworkName::Solana => {
                    if let Some(tkn) = self.sol_tokenlist.search_id(symbol)? {
                        Ok(json!(tkn))
                    } else {
                        Err(Error::NotSupportedToken)
                    }
                }
                NetworkName::Bitcoin => {
                    if let Some(tkn) = self.btc_tokenlist.search_id(symbol)? {
                        Ok(json!(tkn))
                    } else {
                        Err(Error::NotSupportedToken)
                    }
                }
                NetworkName::Ethereum => {
                    if symbol.to_lowercase() == "eth" {
                        Ok(json!(ETH_NATIVE_TOKEN_ID.to_string()))
                    } else if let Some(tkn) = self.eth_tokenlist.search_id(symbol)? {
                        Ok(json!(tkn))
                    } else {
                        Err(Error::NotSupportedToken)
                    }
                }
                _ => Err(Error::NotSupportedNetwork),
            }
        }
        .await;

        match result {
            Ok(res) => JsonResult::Resp(jsonresp(json!(res), id)),
            Err(err) => JsonResult::Err(jsonerr(InternalError, Some(err.to_string()), json!(id))),
        }
    }

    // RPCAPI:
    // Asks the configured cashier for their supported features.
    // Returns a map of features received from the requested cashier.
    // --> {"jsonrpc": "2.0", "method": "features", "params": [], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": {"network": ["btc", "sol"]}, "id": 1}
    async fn features(&self, id: Value, _params: Value) -> JsonResult {
        let req = jsonreq(json!("features"), json!([]));
        let rep: JsonResult =
            // NOTE: this just selects the first cashier in the list
            match send_request(&self.cashiers[0].rpc_url, json!(req)).await {
                Ok(v) => v,
                Err(e) => return JsonResult::Err(jsonerr(ServerError(-32004), Some(e.to_string()), id)),
            };

        match rep {
            JsonResult::Resp(r) => JsonResult::Resp(r),
            JsonResult::Err(e) => JsonResult::Err(e),
            JsonResult::Notif(_) => JsonResult::Err(jsonerr(InternalError, None, id)),
        }
    }

    // RPCAPI:
    // Initializes a DarkFi deposit request for a given `network`, `token`,
    // and `publickey`.
    // The public key send here is used so the cashier can know where to send
    // the newly minted tokens once the deposit is received.
    // Returns an address to which the caller is supposed to deposit funds.
    // --> {"jsonrpc": "2.0", "method": "deposit", "params": ["network", "token", "publickey"], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": "Ht5G1RhkcKnpLVLMhqJc5aqZ4wYUEbxbtZwGCVbgU7DL", "id": 1}
    async fn deposit(&self, id: Value, params: Value) -> JsonResult {
        let args = params.as_array();

        if args.is_none() {
            return JsonResult::Err(jsonerr(InvalidParams, None, id))
        }

        let args = args.unwrap();
        if args.len() != 2 {
            return JsonResult::Err(jsonerr(InvalidParams, None, id))
        }

        let network: NetworkName;
        let token: &str;

        match (args[0].as_str(), args[1].as_str()) {
            (Some(net), Some(tkn)) => {
                if NetworkName::from_str(net).is_err() {
                    return JsonResult::Err(jsonerr(InvalidNetworkParam, None, id))
                }
                network = NetworkName::from_str(net).unwrap();
                token = tkn;
            }
            (None, _) => return JsonResult::Err(jsonerr(InvalidNetworkParam, None, id)),
            (_, None) => return JsonResult::Err(jsonerr(InvalidTokenIdParam, None, id)),
        }

        let token_id = match assign_id(
            &network,
            token,
            &self.sol_tokenlist,
            &self.eth_tokenlist,
            &self.btc_tokenlist,
        ) {
            Ok(t) => t,
            Err(e) => return JsonResult::Err(jsonerr(InternalError, Some(e.to_string()), id)),
        };

        let pk = self.client.lock().await.main_keypair.public;
        let pubkey = Address::from(pk).to_string();

        // Send request to cashier. If the cashier supports the requested network
        // (and token), it shall return a valid address where tokens can be deposited.
        // If not, an error is returned, and forwarded to the method caller.
        let req = jsonreq(json!("deposit"), json!([network, token_id, pubkey]));
        let rep: JsonResult = match send_request(&self.cashiers[0].rpc_url, json!(req)).await {
            Ok(v) => v,
            Err(e) => {
                debug!(target: "DARKFID", "REQUEST IS ERR");
                return JsonResult::Err(jsonerr(ServerError(-32004), Some(e.to_string()), id))
            }
        };

        match rep {
            JsonResult::Resp(r) => JsonResult::Resp(r),
            JsonResult::Err(e) => JsonResult::Err(e),
            JsonResult::Notif(_n) => JsonResult::Err(jsonerr(InternalError, None, id)),
        }
    }

    // RPCAPI:
    // Initializes a withdraw request for a given `network`, `token`, `publickey`,
    // and `amount`.
    // The publickey send here is the address where the caller wants to receive
    // the tokens they plan to withdraw.
    // On request, sends a request to a cashier to get a deposit address, and
    // then transfers wrapped DarkFitokens to the cashier's wallet. Following that,
    // the cashier should return a transaction ID of them sending the funds that
    // are requested for withdrawal.
    // --> {"jsonrpc": "2.0", "method": "withdraw", "params": ["network", "token", "publickey", "amount"], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": "txID", "id": 1}
    async fn withdraw(&self, id: Value, params: Value) -> JsonResult {
        let args = params.as_array();

        if args.is_none() {
            return JsonResult::Err(jsonerr(InvalidParams, None, id))
        }

        let args = args.unwrap();

        if args.len() != 4 {
            return JsonResult::Err(jsonerr(InvalidParams, None, id))
        }

        let network: NetworkName;
        let token: &str;
        let address: &str;
        let amount: &str;

        match (args[0].as_str(), args[1].as_str(), args[2].as_str(), args[3].as_str()) {
            (Some(net), Some(tkn), Some(addr), Some(val)) => {
                if NetworkName::from_str(net).is_err() {
                    return JsonResult::Err(jsonerr(InvalidNetworkParam, None, id))
                }
                network = NetworkName::from_str(net).unwrap();
                token = tkn;
                address = addr;
                amount = val;
            }
            (None, _, _, _) => return JsonResult::Err(jsonerr(InvalidNetworkParam, None, id)),
            (_, None, _, _) => return JsonResult::Err(jsonerr(InvalidTokenIdParam, None, id)),
            (_, _, None, _) => return JsonResult::Err(jsonerr(InvalidAddressParam, None, id)),
            (_, _, _, None) => return JsonResult::Err(jsonerr(InvalidAmountParam, None, id)),
        }

        let amount_in_apo = match decode_base10(amount, 8, true) {
            Ok(a) => a,
            Err(e) => return JsonResult::Err(jsonerr(InvalidAmountParam, Some(e.to_string()), id)),
        };

        let token_id = match assign_id(
            &network,
            token,
            &self.sol_tokenlist,
            &self.eth_tokenlist,
            &self.btc_tokenlist,
        ) {
            Ok(t) => t,
            Err(e) => return JsonResult::Err(jsonerr(InternalError, Some(e.to_string()), id)),
        };

        let req = jsonreq(json!("withdraw"), json!([network, token_id, address, amount_in_apo]));
        let mut rep: JsonResult = match send_request(&self.cashiers[0].rpc_url, json!(req)).await {
            Ok(v) => v,
            Err(e) => return JsonResult::Err(jsonerr(ServerError(-32004), Some(e.to_string()), id)),
        };

        let token_id: &DrkTokenId;

        if let Some(tk_id) = self.drk_tokenlist.tokens[&network].get(&token.to_uppercase()) {
            token_id = tk_id;
        } else {
            return JsonResult::Err(jsonerr(InvalidTokenIdParam, None, id))
        }

        // send drk to cashier_public
        if let JsonResult::Resp(cashier_public) = &rep {
            let result: Result<()> = async {
                let cashier_public = cashier_public.result.as_str().unwrap();

                let cashier_public: PublicKey =
                    PublicKey::try_from(Address::from_str(cashier_public)?)?;

                self.client
                    .lock()
                    .await
                    .transfer(
                        *token_id,
                        cashier_public,
                        amount_in_apo.try_into()?,
                        self.state.clone(),
                    )
                    .await?;

                Ok(())
            }
            .await;

            match result {
                Err(e) => {
                    rep = JsonResult::Err(jsonerr(InternalError, Some(e.to_string()), id.clone()))
                }
                Ok(_) => {
                    rep = JsonResult::Resp(jsonresp(
                        json!(format!(
                            "Sent request to withdraw {} amount of {:?}",
                            amount, token_id
                        )),
                        id.clone(),
                    ))
                }
            }
        };

        match rep {
            JsonResult::Resp(r) => JsonResult::Resp(r),
            JsonResult::Err(e) => JsonResult::Err(e),
            JsonResult::Notif(_n) => JsonResult::Err(jsonerr(InternalError, None, id)),
        }
    }

    // RPCAPI:
    // Transfer a given wrapped DarkFi token amount to the given address.
    // Returns the transaction ID of the transfer.
    // --> {"jsonrpc": "2.0", "method": "transfer", "params": ["network", "dToken", "address", "amount"], "id": 1}
    // <-- {"jsonrpc": "2.0", "result": "txID", "id": 1}
    async fn transfer(&self, id: Value, params: Value) -> JsonResult {
        let args = params.as_array();
        if args.is_none() {
            return JsonResult::Err(jsonerr(InvalidParams, None, id))
        }
        let args = args.unwrap();
        if args.len() != 4 {
            return JsonResult::Err(jsonerr(InvalidParams, None, id))
        }

        let network: NetworkName;
        let token: &str;
        let address: &str;
        let amount: &str;

        match (args[0].as_str(), args[1].as_str(), args[2].as_str(), args[3].as_str()) {
            (Some(net), Some(tkn), Some(addr), Some(val)) => {
                if NetworkName::from_str(net).is_err() {
                    return JsonResult::Err(jsonerr(InvalidNetworkParam, None, id))
                }
                network = NetworkName::from_str(net).unwrap();
                token = tkn;
                address = addr;
                amount = val;
            }
            (None, _, _, _) => return JsonResult::Err(jsonerr(InvalidNetworkParam, None, id)),
            (_, None, _, _) => return JsonResult::Err(jsonerr(InvalidTokenIdParam, None, id)),
            (_, _, None, _) => return JsonResult::Err(jsonerr(InvalidAddressParam, None, id)),
            (_, _, _, None) => return JsonResult::Err(jsonerr(InvalidAmountParam, None, id)),
        }

        let token_id: &DrkTokenId;

        // get the id for the token
        if let Some(tk_id) = self.drk_tokenlist.tokens[&network].get(&token.to_uppercase()) {
            token_id = tk_id;
        } else {
            return JsonResult::Err(jsonerr(InvalidTokenIdParam, None, id))
        }

        let result: Result<()> = async {
            let drk_address: PublicKey = PublicKey::try_from(Address::from_str(address)?)?;

            let decimals: usize = 8;
            let amount = decode_base10(amount, decimals, true)?;

            self.client
                .lock()
                .await
                .transfer(*token_id, drk_address, amount.try_into()?, self.state.clone())
                .await?;

            Ok(())
        }
        .await;

        match result {
            Ok(_) => JsonResult::Resp(jsonresp(json!("Success"), id)),
            Err(err) => JsonResult::Err(jsonerr(InternalError, Some(err.to_string()), json!(id))),
        }
    }
}

async fn start(
    executor: Arc<Executor<'_>>,
    local_cashier: Option<String>,
    config: &DarkfidConfig,
) -> Result<()> {
    let wallet_path = format!("sqlite://{}", expand_path(&config.wallet_path)?.to_str().unwrap());
    let wallet = WalletDb::new(&wallet_path, &config.wallet_password).await?;

    let rocks = Rocks::new(expand_path(&config.database_path.clone())?.as_path())?;

    let mut cashiers = Vec::new();
    let mut cashier_keys = Vec::new();

    if let Some(cpub) = local_cashier {
        let cashier_public: PublicKey = PublicKey::try_from(Address::from_str(&cpub)?)?;

        cashiers.push(Cashier {
            name: "localCashier".into(),
            rpc_url: Url::parse("tcp://127.0.0.1:9000")?,
            public_key: cashier_public,
        });

        cashier_keys.push(cashier_public);
    } else {
        for cashier in config.clone().cashiers {
            if cashier.public_key.is_empty() {
                return Err(Error::CashierKeysNotFound)
            }

            let cashier_public: PublicKey =
                PublicKey::try_from(Address::from_str(&cashier.public_key)?)?;

            cashiers.push(Cashier {
                name: cashier.name,
                rpc_url: Url::parse(&cashier.rpc_url)?,
                public_key: cashier_public,
            });

            cashier_keys.push(cashier_public);
        }
    }

    let client = Client::new(
        rocks.clone(),
        (Url::parse(&config.gateway_protocol_url)?, Url::parse(&config.gateway_publisher_url)?),
        wallet.clone(),
    )
    .await?;

    let client = Arc::new(Mutex::new(client));

    let tree = client.lock().await.get_tree().await?;
    let merkle_roots = RocksColumn::<columns::MerkleRoots>::new(rocks.clone());
    let nullifiers = RocksColumn::<columns::Nullifiers>::new(rocks);

    info!("Building verifying key for the mint contract...");
    let mint_vk = VerifyingKey::build(11, &MintContract::default());
    info!("Building verifying key for the spend contract...");
    let spend_vk = VerifyingKey::build(11, &SpendContract::default());

    let state = Arc::new(Mutex::new(State {
        tree,
        merkle_roots,
        nullifiers,
        mint_vk,
        spend_vk,
        public_keys: cashier_keys,
    }));

    let mut darkfid = Darkfid::new(client, state, cashiers).await?;

    let server_config = RpcServerConfig {
        socket_addr: config.rpc_listen_address,
        use_tls: config.serve_tls,
        identity_path: expand_path(&config.tls_identity_path.clone())?,
        identity_pass: config.tls_identity_password.clone(),
    };

    darkfid.start(executor.clone()).await?;
    listen_and_serve(server_config, Arc::new(darkfid), executor).await
}

#[async_std::main]
async fn main() -> Result<()> {
    let args = CliDarkfid::parse();
    let matches = CliDarkfid::into_app().get_matches();

    let config_path = if args.config.is_some() {
        expand_path(&args.config.unwrap())?
    } else {
        join_config_path(&PathBuf::from("darkfid.toml"))?
    };

    // Spawn config file if it's not in place already.
    spawn_config(&config_path, CONFIG_FILE_CONTENTS)?;

    let (lvl, conf) = log_config(matches)?;
    TermLogger::init(lvl, conf, TerminalMode::Mixed, ColorChoice::Auto)?;

    let config: DarkfidConfig = Config::<DarkfidConfig>::load(config_path)?;

    if args.refresh {
        info!(target: "DARKFI DAEMON", "Refresh the wallet and the database");
        let wallet_path =
            format!("sqlite://{}", expand_path(&config.wallet_path)?.to_str().unwrap());
        let wallet = WalletDb::new(&wallet_path, &config.wallet_password).await?;

        wallet.remove_own_coins().await?;

        if let Some(path) = expand_path(&config.database_path)?.to_str() {
            info!(target: "DARKFI DAEMON", "Remove database: {}", path);
            std::fs::remove_dir_all(path)?;
        }

        info!("Wallet updated successfully.");

        return Ok(())
    }

    let ex = Arc::new(Executor::new());
    let (signal, shutdown) = async_channel::unbounded::<()>();

    let ex2 = ex.clone();

    let nthreads = num_cpus::get();
    debug!(target: "DARKFI DAEMON", "Run {} executor threads", nthreads);

    let (_, result) = Parallel::new()
        .each(0..nthreads, |_| smol::future::block_on(ex.run(shutdown.recv())))
        // Run the main future on the current thread.
        .finish(|| {
            smol::future::block_on(async move {
                start(ex2, args.cashier, &config).await?;
                drop(signal);
                Ok::<(), darkfi::Error>(())
            })
        });

    result
}
