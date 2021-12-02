use std::{collections::HashMap, path::PathBuf, str::FromStr};

use async_executor::Executor;
use async_std::sync::{Arc, Mutex};
use async_trait::async_trait;
use clap::clap_app;
use easy_parallel::Parallel;
use incrementalmerkletree::bridgetree::BridgeTree;
use log::{debug, info};
use num_bigint::BigUint;
use pasta_curves::pallas;
use serde_json::{json, Value};
use url::Url;

use drk::{
    blockchain::{rocks::columns, Rocks, RocksColumn},
    circuit::{MintContract, SpendContract},
    cli::{Config, DarkfidConfig},
    client::Client,
    crypto::{keypair::PublicKey, merkle_node::MerkleNode, proof::VerifyingKey},
    rpc::{
        jsonrpc::{
            error as jsonerr, request as jsonreq, response as jsonresp, send_raw_request,
            ErrorCode::*, JsonRequest, JsonResult,
        },
        rpcserver::{listen_and_serve, RequestHandler, RpcServerConfig},
    },
    serial::{deserialize, serialize},
    state::{ProgramState, State},
    util::{
        assign_id, decode_base10, encode_base10, expand_path, join_config_path, DrkTokenList,
        NetworkName, TokenList,
    },
    wallet::walletdb::WalletDb,
    Error, Result,
};

#[derive(Clone, Debug)]
pub struct Cashier {
    pub name: String,
    pub rpc_url: String,
    pub public_key: PublicKey,
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

struct Darkfid {
    client: Arc<Mutex<Client>>,
    state: Arc<Mutex<State>>,
    sol_tokenlist: TokenList,
    eth_tokenlist: TokenList,
    btc_tokenlist: TokenList,
    drk_tokenlist: DrkTokenList,
    cashiers: Vec<Cashier>,
}

impl Darkfid {
    async fn new(
        client: Arc<Mutex<Client>>,
        state: Arc<Mutex<State>>,
        cashiers: Vec<Cashier>,
    ) -> Result<Self> {
        let sol_tokenlist = TokenList::new(include_bytes!("../../token/solana_token_list.json"))?;
        let eth_tokenlist = TokenList::new(include_bytes!("../../token/erc20_token_list.json"))?;
        let btc_tokenlist = TokenList::new(include_bytes!("../../token/bitcoin_token_list.json"))?;
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

    // --> {"method": "say_hello", "params": []}
    // <-- {"result": "hello world"}
    async fn say_hello(&self, id: Value, _params: Value) -> JsonResult {
        JsonResult::Resp(jsonresp(json!("hello world"), id))
    }

    // --> {"method": "create_wallet", "params": []}
    // <-- {"result": true}
    async fn create_wallet(&self, id: Value, _params: Value) -> JsonResult {
        match self.client.lock().await.init_db().await {
            Ok(()) => JsonResult::Resp(jsonresp(json!(true), id)),
            Err(e) => JsonResult::Err(jsonerr(ServerError(-32001), Some(e.to_string()), id)),
        }
    }

    // --> {"method": "key_gen", "params": []}
    // <-- {"result": true}
    async fn key_gen(&self, id: Value, _params: Value) -> JsonResult {
        let client = self.client.lock().await;
        match client.key_gen().await {
            Ok(()) => JsonResult::Resp(jsonresp(json!(true), id)),
            Err(e) => JsonResult::Err(jsonerr(ServerError(-32002), Some(e.to_string()), id)),
        }
    }

    // --> {"method": "get_key", "params": []}
    // <-- {"result": "vdNS7oBj7KvsMWWmo9r96SV4SqATLrGsH2a3PGpCfJC"}
    async fn get_key(&self, id: Value, _params: Value) -> JsonResult {
        let pk = self.client.lock().await.main_keypair.public;
        let b58 = bs58::encode(serialize(&pk)).into_string();
        JsonResult::Resp(jsonresp(json!(b58), id))
    }

    // --> {"method": "get_balances", "params": []}
    // <-- {"result": "get_balances": "[ {"btc": (value, network)}, .. ]"}
    async fn get_balances(&self, id: Value, _params: Value) -> JsonResult {
        let result: Result<HashMap<String, (String, String)>> = async {
            let balances = self.client.lock().await.get_balances().await?;
            let mut symbols: HashMap<String, (String, String)> = HashMap::new();

            for balance in balances.list.iter() {
                let amount = encode_base10(BigUint::from(balance.value), 8);
                if let Some((network, symbol)) =
                    self.drk_tokenlist.symbol_from_id(&balance.token_id)?
                {
                    symbols.insert(symbol, (amount, network.to_string()));
                } else {
                    // TODO: SQL needs to have the mint address for show, not the internal hash.
                    // TODO: SQL needs to have the network name
                    //symbols.insert(balance.token_id.to_string(), (amount,
                    // String::from("UNKNOWN")));
                    symbols.insert(
                        format!("{:?}", balance.token_id),
                        (amount, String::from("UNKNONW")),
                    );
                }
            }
            Ok(symbols)
        }
        .await;
        match result {
            Ok(res) => JsonResult::Resp(jsonresp(json!(res), id)),
            Err(err) => JsonResult::Err(jsonerr(InternalError, Some(err.to_string()), json!(id))),
        }
    }

    // --> {"method": "get_token_id", "params": [network, token]}
    // <-- {"result": "Ht5G1RhkcKnpLVLMhqJc5aqZ4wYUEbxbtZwGCVbgU7DL"}
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
                #[cfg(feature = "sol")]
                NetworkName::Solana => {
                    if let Some(tkn) = self.sol_tokenlist.search_id(symbol)? {
                        Ok(json!(tkn))
                    } else {
                        Err(Error::NotSupportedToken)
                    }
                }
                #[cfg(feature = "btc")]
                NetworkName::Bitcoin => {
                    if let Some(tkn) = self.btc_tokenlist.search_id(symbol)? {
                        Ok(json!(tkn))
                    } else {
                        Err(Error::NotSupportedToken)
                    }
                }
                #[cfg(feature = "eth")]
                NetworkName::Ethereum => {
                    if symbol.to_lowercase() == "eth" {
                        use drk::service::eth::ETH_NATIVE_TOKEN_ID;
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

    // --> {""method": "features", "params": []}
    // <-- {"result": { "network": ["btc", "sol"] } }
    async fn features(&self, id: Value, _params: Value) -> JsonResult {
        let req = jsonreq(json!("features"), json!([]));
        let rep: JsonResult;
        // NOTE: this just selects the first cashier in the list
        match send_raw_request(&self.cashiers[0].rpc_url, json!(req)).await {
            Ok(v) => rep = v,
            Err(e) => return JsonResult::Err(jsonerr(ServerError(-32004), Some(e.to_string()), id)),
        }

        match rep {
            JsonResult::Resp(r) => JsonResult::Resp(r),
            JsonResult::Err(e) => JsonResult::Err(e),
            JsonResult::Notif(_) => JsonResult::Err(jsonerr(InternalError, None, id)),
        }
    }

    // --> {"method": "deposit", "params": [network, token, publickey]}
    // The publickey sent here is used so the cashier can know where to send
    // tokens once the deposit is received.
    // <-- {"result": "Ht5G1RhkcKnpLVLMhqJc5aqZ4wYUEbxbtZwGCVbgU7DL"}
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

        // TODO: Optional sanity checking here, but cashier *must* do so too.

        let pk = self.client.lock().await.main_keypair.public;
        let pubkey = bs58::encode(serialize(&pk)).into_string();

        // Send request to cashier. If the cashier supports the requested network
        // (and token), it shall return a valid address where tokens can be deposited.
        // If not, an error is returned, and forwarded to the method caller.
        let req = jsonreq(json!("deposit"), json!([network, token_id, pubkey]));
        let rep: JsonResult;
        match send_raw_request(&self.cashiers[0].rpc_url, json!(req)).await {
            Ok(v) => rep = v,
            Err(e) => {
                debug!(target: "DARKFID", "REQUEST IS ERR");
                return JsonResult::Err(jsonerr(ServerError(-32004), Some(e.to_string()), id))
            }
        }

        match rep {
            JsonResult::Resp(r) => JsonResult::Resp(r),
            JsonResult::Err(e) => JsonResult::Err(e),
            JsonResult::Notif(_n) => JsonResult::Err(jsonerr(InternalError, None, id)),
        }
    }

    // --> {"method": "withdraw", "params": [network, token, publickey, amount]}
    // The publickey sent here is the address where the caller wants to receive
    // the tokens they plan to withdraw.
    // On request, send request to cashier to get deposit address, and then transfer
    // dark tokens to the cashier's wallet. Following that, the cashier should return
    // a transaction ID of them sending the funds that are requested for withdrawal.
    // <-- {"result": "txID"}
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
        let mut rep: JsonResult;
        match send_raw_request(&self.cashiers[0].rpc_url, json!(req)).await {
            Ok(v) => rep = v,
            Err(e) => return JsonResult::Err(jsonerr(ServerError(-32004), Some(e.to_string()), id)),
        }

        let token_id: &pallas::Base;

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
                    deserialize(&bs58::decode(cashier_public).into_vec()?)?;

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

    // --> {"method": "transfer", [network, dToken, address, amount]}
    // <-- {"result": "txID"}
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

        let token_id: &pallas::Base;

        // get the id for the token
        if let Some(tk_id) = self.drk_tokenlist.tokens[&network].get(&token.to_uppercase()) {
            token_id = tk_id;
        } else {
            return JsonResult::Err(jsonerr(InvalidTokenIdParam, None, id))
        }

        let result: Result<()> = async {
            let drk_address = bs58::decode(&address).into_vec()?;
            let drk_address: PublicKey = deserialize(&drk_address)?;

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
    local_cashier: Option<&str>,
    config: &DarkfidConfig,
) -> Result<()> {
    let wallet =
        WalletDb::new(expand_path(&config.wallet_path)?.as_path(), config.wallet_password.clone())
            .await?;

    let rocks = Rocks::new(expand_path(&config.database_path.clone())?.as_path())?;

    let mut cashiers = Vec::new();
    let mut cashier_keys = Vec::new();

    if let Some(cpub) = local_cashier {
        let cashier_public: PublicKey = deserialize(&bs58::decode(cpub).into_vec()?)?;

        cashiers.push(Cashier {
            name: "localCashier".into(),
            rpc_url: "tcp://127.0.0.1:9000".into(),
            public_key: cashier_public,
        });

        cashier_keys.push(cashier_public);
    } else {
        for cashier in config.clone().cashiers {
            if cashier.public_key.is_empty() {
                return Err(Error::CashierKeysNotFound)
            }

            let cashier_public: PublicKey =
                deserialize(&bs58::decode(cashier.public_key).into_vec()?)?;

            cashiers.push(Cashier {
                name: cashier.name,
                rpc_url: cashier.rpc_url,
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

    let merkle_roots = RocksColumn::<columns::MerkleRoots>::new(rocks.clone());
    let nullifiers = RocksColumn::<columns::Nullifiers>::new(rocks);

    info!("Building verifying key for the mint contract...");
    let mint_vk = VerifyingKey::build(11, MintContract::default());
    info!("Building verifying key for the spend contract...");
    let spend_vk = VerifyingKey::build(11, SpendContract::default());

    let state = Arc::new(Mutex::new(State {
        tree: BridgeTree::<MerkleNode, 32>::new(100),
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
    let args = clap_app!(darkfid =>
        (@arg CONFIG: -c --config +takes_value "Sets a custom config file")
        (@arg verbose: -v --verbose "Increase verbosity")
        (@arg refresh: -r --refresh "Refresh the wallet and slabstore")
        (@arg cashier: --cashier +takes_value "Local cashier public key")
    )
    .get_matches();

    let config_path = if args.is_present("CONFIG") {
        expand_path(args.value_of("CONFIG").unwrap())?
    } else {
        join_config_path(&PathBuf::from("darkfid.toml"))?
    };

    let loglevel = if args.is_present("verbose") { log::Level::Debug } else { log::Level::Info };

    simple_logger::init_with_level(loglevel)?;

    let config: DarkfidConfig = Config::<DarkfidConfig>::load(config_path)?;

    if args.is_present("refresh") {
        debug!(target: "DARKFI DAEMON", "Refresh the wallet and the database");

        let wallet = WalletDb::new(
            expand_path(&config.wallet_path)?.as_path(),
            config.wallet_password.clone(),
        )
        .await?;

        wallet.remove_own_coins().await?;

        if let Some(path) = expand_path(&config.database_path)?.to_str() {
            debug!(target: "DARKFI DAEMON", "Remove database: {}", path);
            std::fs::remove_dir_all(path)?;
        }

        println!("Wallet got updated successfully.");

        return Ok(())
    }

    let mut local_cashier: Option<&str> = None;

    if args.is_present("cashier") {
        local_cashier = Some(args.value_of("cashier").unwrap())
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
                start(ex2, local_cashier, &config).await?;
                drop(signal);
                Ok::<(), drk::Error>(())
            })
        });

    result
}
