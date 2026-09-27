# Auto Publisher

A small Rust Discord bot designed for easy self-hosting. It immediately requests publication of new messages in every announcement channel it can see, across every server it joins. Includes messages from people, bots, and webhooks.

One native executable and a bot token. No database, Docker, commands, or per-server setup.

## Setup

1. Install stable Rust using [rustup](https://rustup.rs/) to build the executable. On Windows, install the Visual Studio C++ build tools if the installer requests them. Rust is only needed on the build machine.
2. Create an application and bot in the [Discord Developer Portal](https://discord.com/developers/applications). Copy the bot token.
3. Copy `.env.example` to `.env` and replace the placeholder:

   ```dotenv
   DISCORD_TOKEN=YOUR_BOT_TOKEN_HERE
   ```

   `.env.example` is included in source control; `.env` is ignored. You can also set `DISCORD_TOKEN` directly in your hosting environment, which takes precedence over the file.
4. Run from this directory:

   ```sh
   cargo build --release --locked
   ```

5. Start the executable:

   Windows (PowerShell):

   ```powershell
   .\target\release\auto-publisher.exe
   ```

   Linux/macOS:

   ```sh
   ./target/release/auto-publisher
   ```

6. Open the invite URL printed at startup and add the bot to your server using an account that can manage that server. The link requests the permissions listed below.

Leave the process running. Stop with Ctrl+C (or SIGTERM on Linux/macOS). For unattended hosting, use your operating system's service manager and set `DISCORD_TOKEN` in its environment, or set its working directory to the folder containing `.env`. Only the current directory's `.env` is read; parent directories are not searched. Restart after changing the token.

Deploy the release executable built for your host's OS and architecture, and configure the token. Cargo, Rust source, and build dependencies are not needed to run it. Windows builds use the usual Windows/MSVC system runtime; Linux builds use the target's system libraries (use a musl target if a static Linux executable is needed).

## Discord permissions and settings

The bot needs these permissions in every announcement channel you want it to publish:

| Permission | Why it is needed |
| --- | --- |
| **View Channel** | Receive new message events in the channel. |
| **Send Messages** | Required by Discord's publish endpoint, even though the bot does not write new messages. |
| **Manage Messages** | Publish messages written by other people, bots, and webhooks. |

Check **Edit Channel > Permissions**, including inherited category permissions, if a channel does not work. The bot's effective channel permissions must include all three; an invite's permission request does not bypass channel overrides.

Administrator, Manage Channels, Manage Roles, Manage Webhooks, and Read Message History are not required. This bot only handles new messages and does not fetch channel history.

In the Developer Portal's **Bot > Privileged Gateway Intents** section, leave **Presence Intent**, **Server Members Intent**, and **Message Content Intent** off. The code requests only the standard Guilds and GuildMessages intents. Publishing uses message IDs and metadata, so it does not need access to message content. See Discord's [gateway intent documentation](https://discord.com/developers/docs/events/gateway#gateway-intents).

For a manual invite, select the **bot** scope in **OAuth2 > URL Generator**, then select the three permissions above. The permission integer is `11264`. No slash-command scope or Interactions Endpoint URL is needed. Leave **Requires OAuth2 Code Grant** disabled for this simple bot invite.

Use a channel whose actual type is **Announcement**; naming a normal text channel `announcements` does not make it one. Announcement channels are a Community server feature.

## Verify it works

1. Start the bot and wait for `Logged in as ...` in the terminal.
2. Send a new message in an announcement channel where it has the required permissions.
3. Confirm Discord marks the message as published and the terminal prints `Published ...`.

Only new messages received while the bot is online are handled. An existing unpublished message is not a valid startup test.

## Troubleshooting

| Symptom | What to check |
| --- | --- |
| Cannot read .env / missing token | Put `.env` in the current working directory, or set `DISCORD_TOKEN` in the environment. Replace the placeholder with the token from the Developer Portal. |
| Discord connection stopped | Use the token from the Developer Portal's Bot page, not the application ID, public key, or client secret. After resetting a token, update the file and restart. Also check gateway intent settings and the sharding limit below. |
| Bot is online but nothing happens | Check the channel's actual type and View Channel permission, then send a new message. Ordinary channels, threads, and received crossposts are skipped. |
| Publish fails with code `50013` | Check Send Messages and Manage Messages permissions, including channel/category overrides. |
| Publish fails with code `50001` | The bot has lost access to the channel or message; check View Channel and server membership. |
| Publishing is delayed | Discord rate limits and network latency still apply. The bot adds no intentional delay. |

## Behavior

- Uses Discord's live message events; no polling, intentional delay, or preliminary API fetch.
- Detects announcement channels automatically, including ones created after startup.
- Skips ordinary channels, threads, system messages, already published messages, and announcements received from followed channels.
- Respects Discord's rate limits via Twilight; publication cannot be guaranteed to be instantaneous. HTTP requests run asynchronously so rate limits do not block gateway heartbeats or channel discovery.
- Reconnects after temporary connection loss. There is no stored backlog or scan for messages posted while offline. Failed publications are logged; permanent failures are not retried by this bot.
- To exclude a channel, deny the bot View Channel there.
- Uses one gateway shard, supporting up to 2,500 servers. Larger deployments need sharding support.
- To bound memory during a sustained backlog, at most 1,024 publications may be pending. Additional eligible messages are skipped with a log entry. Pending requests are canceled at shutdown and are not persisted.

## Resource usage

The bot uses Tokio's single-threaded async runtime and only Twilight's gateway, HTTP, and model libraries. It retains announcement-channel IDs and their guild IDs, with no message, member, role, or full guild cache. Incoming event objects are discarded after processing; pending publications retain only three IDs. Gateway compression and optional HTTP decompression are disabled to reduce dependencies and per-connection state, at the cost of more network traffic.

Release builds use size optimization, link-time optimization, stripped symbols, and abort-on-panic. `Cargo.lock` pins dependencies for repeatable builds. Actual memory use depends on server count, incoming payloads, and pending HTTP work.

Discord documents publishing permissions in its [Crosspost Message endpoint](https://discord.com/developers/docs/resources/message#crosspost-message).

## Development

```sh
cargo test --locked
cargo fmt --check
cargo clippy --locked --all-targets -- -D warnings
```

Tests cover message filtering, channel discovery and updates, configuration validation without token disclosure, and publication requests and failures through a local HTTP test server. They do not connect to Discord. Use the steps under "Verify it works" for a live check with your bot token.
