use std::env;
use std::str::FromStr;
use std::sync::Arc;

use etherscan::{Etherscan, Network, Sort, Tag};
use serenity::async_trait;
use serenity::client::{Client, Context, EventHandler};
use serenity::framework::standard::{Args, CommandResult, macros::{
    command,
    group,
}, StandardFramework};
use serenity::futures::{StreamExt, TryFutureExt};
use serenity::model::channel::Message;
use serenity::prelude::*;
use std::cmp::max;

// A container type is created for inserting into the Client's `data`, which
// allows for data to be accessible across all events and framework commands, or
// anywhere else that has a copy of the `data` Arc.
struct Fetcher;

impl TypeMapKey for Fetcher {
    type Value = Etherscan;
}

#[group]
#[commands(balance, erc20, clean_channel)]
struct General;

struct Handler;

#[async_trait]
impl EventHandler for Handler {}

#[tokio::main]
async fn main() {
    let framework = StandardFramework::new()
        .configure(|c|
            c.with_whitespace(true)
             .delimiters(vec![" "])
             .prefix("~")
        ) // set the bot's prefix to "~"
        .group(&GENERAL_GROUP);

    // Login with a bot token from the environment
    let token = dotenv::var("DISCORD_TOKEN").expect("token");
    let mut client = Client::builder(token)
        .event_handler(Handler)
        .framework(framework)
        .await
        .expect("Error creating client");

    {
        let api_key = dotenv::var("API_KEY").expect("api_key");
        let network = Network::MainNet;

        let etherscan = Etherscan::new(api_key, network);

        let mut data = client.data.write().await;
        data.insert::<Fetcher>(etherscan);
    }

    // start listening for events by starting a single shard
    if let Err(why) = client.start().await {
        println!("An error occurred while running the client: {:?}", why);
    }
}

#[command]
async fn clean_channel(ctx: &Context, msg: &Message) -> CommandResult {
    let mut messages = msg.channel_id.messages_iter(&ctx).boxed();
    while let Some(message_result) = messages.next().await {
        match message_result {
            Ok(m) => {
                if m.is_own(ctx).await {
                    m.delete(ctx).await;
                } else if m.content.starts_with("~") {
                    msg.channel_id.delete_message(ctx, m.id).await;
                }
            }
            Err(_) => continue,
        }
    }

    Ok(())
}

#[command]
async fn balance(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
    let data = ctx.data.read().await;
    let etherscan = data.get::<Fetcher>().expect("Expected Etherscan in TypeMap.");

    let channel = msg.channel_id;

    match args.len() {
        0 => { channel.say(ctx, "Please specify an address: `~balance <address>`").await?; },
        1 => {
            let address = args.single::<String>().unwrap();
            let balance = etherscan.balance(address, Tag::Latest).await;
            channel.say(ctx, format!("Balance: {} Wei", balance.result)).await?;
        }
        _ => {
            let address_list = args.iter::<String>().collect::<Result<Vec<_>, _>>().unwrap();
            let balances = etherscan.balances(address_list, Tag::Latest).await;

            channel.say(
                ctx, format!(
                    "```json\n{}\n```",
                    serde_json::to_string_pretty(&balances.result).unwrap()
                ),
            ).await?;
        }
    }

    Ok(())
}

#[command]
async fn erc20(ctx: &Context, msg: &Message, mut args: Args) -> CommandResult {
    let data = ctx.data.read().await;
    let etherscan = data.get::<Fetcher>().expect("Expected Etherscan in TypeMap.");

    let channel = msg.channel_id;

    if args.len() != 3 {
        channel.say(ctx, "Incorrect command: `~erc20 <contract_address> <address> <max_results>`").await?;
        return Ok(());
    }

    let contract_address = args.single::<String>()?;
    let address = args.single::<String>()?;
    let max_result = args.single::<u32>().unwrap_or(999);

    if max_result > 10 || max_result < 1 {
        channel.say(ctx, "Max results should be a number from 1 to 10").await?;
        return Ok(());
    }

    let tte = etherscan.erc20_tte_by_address(
        contract_address,
        address,
        1,
        max_result,
        Sort::DESC,
    ).await;


    for r in tte.result {
        channel.say(ctx, format!("```json\n{}\n```", serde_json::to_string_pretty(&r).unwrap())).await?;
    }

    Ok(())
}