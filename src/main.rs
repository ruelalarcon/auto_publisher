mod config;
mod publisher;

use futures_util::{StreamExt as _, stream::FuturesUnordered};
use std::{process::ExitCode, time::Duration};
use twilight_gateway::{
    CloseFrame, Event, EventTypeFlags, Intents, Shard, ShardId, StreamExt as _,
};

const MAX_PENDING: usize = 1024;

#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> Result<(), &'static str> {
    if std::env::args_os().nth(1).is_some() {
        return Err("Usage: auto-publisher (configure DISCORD_TOKEN in .env or the environment).");
    }
    let token = config::token()?;
    rustls::crypto::ring::default_provider()
        .install_default()
        .map_err(|_| "Cannot initialize TLS.")?;
    let http = twilight_http::Client::new(token.clone());
    let mut shard = Shard::new(
        ShardId::ONE,
        token,
        Intents::GUILDS | Intents::GUILD_MESSAGES,
    );
    let wanted = EventTypeFlags::READY
        | EventTypeFlags::GUILD_CREATE
        | EventTypeFlags::GUILD_DELETE
        | EventTypeFlags::CHANNEL_CREATE
        | EventTypeFlags::CHANNEL_UPDATE
        | EventTypeFlags::CHANNEL_DELETE
        | EventTypeFlags::MESSAGE_CREATE;
    let mut channels = publisher::AnnouncementChannels::default();
    let mut pending = FuturesUnordered::new();
    let shutdown = shutdown_signal();
    tokio::pin!(shutdown);
    println!("Connecting to Discord...");

    loop {
        tokio::select! {
            signal = &mut shutdown => {
                signal?;
                println!("Stopping. Pending publications will be canceled.");
                pending.clear();
                shard.close(CloseFrame::NORMAL);
                // Flush the close frame so Discord can mark the bot offline.
                let _ = tokio::time::timeout(Duration::from_secs(2), async {
                    while let Some(event) = shard.next_event(EventTypeFlags::empty()).await {
                        if matches!(event, Ok(Event::GatewayClose(_))) {
                            break;
                        }
                    }
                }).await;
                return Ok(());
            }
            _ = pending.next(), if !pending.is_empty() => {}
            incoming = shard.next_event(wanted) => {
                let Some(incoming) = incoming else {
                    return Err("Discord connection stopped. Check the bot token, gateway intents, and whether sharding is required.");
                };
                let event = match incoming {
                    Ok(event) => event,
                    Err(_) => {
                        // Gateway errors may contain entire payloads. Never log their contents.
                        eprintln!("Discord connection or event error. The client will attempt to recover.");
                        continue;
                    }
                };
                channels.update(&event);
                match event {
                    Event::Ready(ready) => {
                        println!("Logged in as {}. Watching announcement channels in {} server(s).", ready.user.name, ready.guilds.len());
                        println!("Invite: https://discord.com/oauth2/authorize?client_id={}&scope=bot&permissions=11264", ready.user.id);
                    }
                    Event::MessageCreate(message) => {
                        if let Some(post) = channels.publication(&message) {
                            // Never retain full messages while Discord rate-limits a request.
                            // Keep polling the gateway while publication waits for HTTP.
                            if pending.len() >= MAX_PENDING {
                                eprintln!("Skipped {post}: {MAX_PENDING} publications already pending.");
                            } else {
                                pending.push(publisher::publish(&http, post));
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}

async fn shutdown_signal() -> Result<(), &'static str> {
    #[cfg(unix)]
    {
        let mut terminate =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                .map_err(|_| "Cannot listen for SIGTERM.")?;
        tokio::select! {
            result = tokio::signal::ctrl_c() => result.map_err(|_| "Cannot listen for Ctrl+C."),
            _ = terminate.recv() => Ok(()),
        }
    }
    #[cfg(not(unix))]
    tokio::signal::ctrl_c()
        .await
        .map_err(|_| "Cannot listen for Ctrl+C.")
}
