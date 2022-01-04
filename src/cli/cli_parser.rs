use clap::{AppSettings, Parser, Subcommand};

#[derive(Subcommand)]
pub enum CliDrkSubCommands {
    /// Say hello to the RPC
    Hello {},
    /// Show what features the cashier supports
    Features {},
    /// Wallet operations
    Wallet {
        /// Initialize a new wallet
        #[clap(long)]
        create: bool,
        /// Generate wallet keypair
        #[clap(long)]
        keygen: bool,
        /// Get default wallet address
        #[clap(long)]
        address: bool,
        /// Get wallet addresses
        #[clap(long)]
        addresses: bool,
        /// Set default address
        #[clap(long, value_name = "ADDRESS")]
        set_default_address: Option<String>,
        /// Export default address
        #[clap(long, value_name = "PATH")]
        export_keypair: Option<String>,
        /// Import address
        #[clap(long, value_name = "PATH")]
        import_keypair: Option<String>,
        /// Get wallet balances
        #[clap(long)]
        balances: bool,
    },
    /// Get hexidecimal ID for token symbol
    Id {
        /// Which network to use (bitcoin/solana/...)
        #[clap(long)]
        network: String,
        /// Which token to query (btc/sol/usdc/...)
        #[clap(parse(try_from_str))]
        token: String,
    },
    /// Withdraw Dark tokens for clear tokens
    Withdraw {
        /// Which network to use (bitcoin/solana/...)
        #[clap(long)]
        network: String,
        /// Which token to receive (btc/sol/usdc/...)
        #[clap(parse(try_from_str))]
        token_sym: String,
        /// Recipient address
        #[clap(parse(try_from_str))]
        address: String,
        /// Amount to withdraw
        #[clap(parse(try_from_str))]
        amount: u64,
    },
    /// Transfer Dark tokens to address
    Transfer {
        /// Which network to use (bitcoin/solana/...)
        #[clap(long)]
        network: String,
        /// Which token to transfer (btc/sol/usdc/...)
        #[clap(parse(try_from_str))]
        token_sym: String,
        /// Recipient address
        #[clap(parse(try_from_str))]
        address: String,
        /// Amount to transfer
        #[clap(parse(try_from_str))]
        amount: u64,
    },
    /// Deposit clear tokens for Dark tokens
    Deposit {
        /// Which network to use (bitcoin/solana/...)
        #[clap(long)]
        network: String,
        /// Which token to deposit (btc/sol/usdc/...)
        #[clap(parse(try_from_str))]
        token_sym: String,
    },
}

/// Drk cli
#[derive(Parser)]
#[clap(name = "drk")]
#[clap(author, version, about)]
#[clap(global_setting(AppSettings::PropagateVersion))]
#[clap(global_setting(AppSettings::UseLongFormatForHelpSubcommand))]
#[clap(setting(AppSettings::SubcommandRequiredElseHelp))]
pub struct CliDrk {
    /// Sets a custom config file
    #[clap(short, long)]
    pub config: Option<String>,
    /// Increase verbosity
    #[clap(short, parse(from_occurrences))]
    pub verbose: u8,
    #[clap(subcommand)]
    pub command: Option<CliDrkSubCommands>,
}

/// Gatewayd cli
#[derive(Parser)]
#[clap(name = "gatewayd")]
pub struct CliGatewayd {
    /// Sets a custom config file
    #[clap(short, long)]
    pub config: Option<String>,
    /// Increase verbosity
    #[clap(short, parse(from_occurrences))]
    pub verbose: u8,
    /// Show event trace
    #[clap(short, long)]
    pub trace: bool,
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

/// Cashierd cli
#[derive(Parser)]
#[clap(name = "cashierd")]
pub struct CliCashierd {
    /// Sets a custom config file
    #[clap(short, long)]
    pub config: Option<String>,
    /// Get Cashier Public key
    #[clap(short, long)]
    pub address: bool,
    /// Increase verbosity
    #[clap(short, parse(from_occurrences))]
    pub verbose: u8,
    /// Refresh the wallet and slabstore
    #[clap(short, long)]
    pub refresh: bool,
}
