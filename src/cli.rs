use std::str::FromStr;

use solana_sdk::hash::Hash;
use solana_sdk::{pubkey::Pubkey, signature::Keypair};
use structopt::StructOpt;

use crate::error::Error;

#[derive(Debug, StructOpt)]
#[structopt(name = "solana-tss", about = "A PoC for managing a Solana TSS wallet.")]
pub enum Options {
    /// Generate a pair of keys.
    Generate,
    /// Check the balance of an address.
    Balance {
        /// The address to check the balance of
        address: Pubkey,
        /// Choose the desired netwrok: Mainnet/Testnet/Devnet
        #[structopt(default_value = "testnet")]
        net: Network,
    },
    /// Request an airdrop from a faucet.
    Airdrop {
        /// Address of the recipient
        to: Pubkey,
        /// The amount of SOL you want to send.
        amount: f64,
        /// Choose the desired netwrok: Mainnet/Testnet/Devnet
        #[structopt(default_value = "testnet")]
        net: Network,
    },
    /// Send a transaction using a single private key.
    SendSingle {
        /// A Base58 secret key
        #[structopt(parse(try_from_str = parse_keypair_bs58))]
        keypair: Keypair,
        /// The amount of SOL you want to send.
        amount: f64,
        /// Address of the recipient
        to: Pubkey,
        /// Choose the desired netwrok: Mainnet/Testnet/Devnet
        #[structopt(default_value = "testnet")]
        net: Network,
        /// Add a memo to the transaction
        memo: Option<String>,
    },
    /// Aggregate a list of addresses into a single address that they can all sign on together
    AggregateKeys {
        /// List of addresses
        keys: Vec<Pubkey>,
    },
    /// Start aggregate signing
    AggSendStepOne {
        /// A Base58 secret key of the party signing
        #[structopt(parse(try_from_str = parse_keypair_bs58))]
        keypair: Keypair,
    },
    /// Step 2 of aggregate signing, you should pass in the secret data from step 1.
    AggSendStepTwo {
        /// A Base58 secret key of the party signing
        #[structopt(parse(try_from_str = parse_keypair_bs58))]
        keypair: Keypair,
        /// A list of all the first messages received in step 1
        first_messages: Vec<String>,
        /// The secret state received in step 1.
        secret_state: String,
    },
    /// Step 3 of aggregate signing, you should pass in the secret data from step 2.
    /// It's important that all parties pass in exactly the same transaction details (amount,to,net,memo,recent_block_hash)
    AggSendStepThree {
        /// A Base58 secret key of the party signing
        #[structopt(parse(try_from_str = parse_keypair_bs58))]
        keypair: Keypair,
        /// The amount of SOL you want to send.
        amount: f64,
        /// Address of the recipient
        to: Pubkey,
        /// Add a memo to the transaction
        memo: Option<String>,
        /// A hash of a recent block, can be obtained by calling `recent-block-hash`, all parties *must* pass in the same hash.
        recent_block_hash: Hash,
        /// List of addresses that are part of this
        keys: Vec<Pubkey>,
        /// A list of all the first messages received in step 2
        second_messages: Vec<String>,
        /// The secret state received in step 2.
        secret_state: String,
    },
}

#[derive(Debug)]
pub enum Network {
    Mainnet,
    Testnet,
    Devnet,
}

impl Network {
    pub fn get_cluster_url(&self) -> &'static str {
        match self {
            Self::Mainnet => "https://api.mainnet-beta.solana.com",
            Self::Testnet => "https://api.testnet.solana.com",
            Self::Devnet => "https://api.devnet.solana.com",
        }
    }
}

impl FromStr for Network {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "mainnet" | "Mainnet" => Ok(Self::Mainnet),
            "testnet" | "Testnet" => Ok(Self::Testnet),
            "devnet" | "Devnet" => Ok(Self::Devnet),
            _ => Err(Error::WrongNetwork(s.to_string())),
        }
    }
}

fn parse_keypair_bs58(s: &str) -> Result<Keypair, Error> {
    let decoded = bs58::decode(s).into_vec()?;
    Ok(Keypair::from_bytes(&decoded)?)
}
