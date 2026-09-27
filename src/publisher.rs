use std::{collections::HashMap, fmt};
use twilight_gateway::Event;
use twilight_http::{Client, error::ErrorType};
use twilight_model::{
    channel::{
        Channel, ChannelType, Message,
        message::{MessageFlags, MessageType},
    },
    gateway::payload::incoming::GuildCreate,
    id::{
        Id,
        marker::{ChannelMarker, GuildMarker, MessageMarker},
    },
};

#[derive(Default)]
pub struct AnnouncementChannels(HashMap<Id<ChannelMarker>, Id<GuildMarker>>);

impl AnnouncementChannels {
    pub fn update(&mut self, event: &Event) {
        match event {
            // A fresh session replaces the guild list; a resumed session preserves it.
            Event::Ready(_) => self.0.clear(),
            Event::GuildCreate(guild) => match guild.as_ref() {
                GuildCreate::Available(guild) => {
                    self.remove_guild(guild.id);
                    for channel in &guild.channels {
                        self.set(channel, Some(guild.id));
                    }
                }
                GuildCreate::Unavailable(guild) => self.remove_guild(guild.id),
            },
            Event::GuildDelete(guild) => self.remove_guild(guild.id),
            Event::ChannelCreate(channel) => self.set(channel, channel.guild_id),
            Event::ChannelUpdate(channel) => self.set(channel, channel.guild_id),
            Event::ChannelDelete(channel) => {
                self.0.remove(&channel.id);
            }
            _ => {}
        }
    }

    fn set(&mut self, channel: &Channel, guild_id: Option<Id<GuildMarker>>) {
        self.0.remove(&channel.id);
        if channel.kind == ChannelType::GuildAnnouncement
            && let Some(guild_id) = guild_id
        {
            self.0.insert(channel.id, guild_id);
        }
    }

    fn remove_guild(&mut self, guild_id: Id<GuildMarker>) {
        self.0.retain(|_, guild| *guild != guild_id);
    }

    pub fn publication(&self, message: &Message) -> Option<Publication> {
        let guild_id = message.guild_id?;
        if self.0.get(&message.channel_id) != Some(&guild_id)
            || !matches!(message.kind, MessageType::Regular | MessageType::Reply)
            || message
                .flags
                .unwrap_or_else(MessageFlags::empty)
                .intersects(MessageFlags::CROSSPOSTED | MessageFlags::IS_CROSSPOST)
        {
            return None;
        }
        Some(Publication {
            guild_id,
            channel_id: message.channel_id,
            message_id: message.id,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Publication {
    guild_id: Id<GuildMarker>,
    channel_id: Id<ChannelMarker>,
    message_id: Id<MessageMarker>,
}

impl fmt::Display for Publication {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}/{}/{}",
            self.guild_id, self.channel_id, self.message_id
        )
    }
}

pub async fn publish(http: &Client, post: Publication) {
    match http
        .crosspost_message(post.channel_id, post.message_id)
        .await
    {
        Ok(_) => println!("Published {post}"),
        Err(error) => {
            let code = match error.kind() {
                ErrorType::Response {
                    error: twilight_http::api_error::ApiError::General(error),
                    ..
                } => Some(error.code),
                _ => None,
            };
            if code == Some(40033) {
                return; // Another publisher already handled it.
            }
            // Neither response bodies nor error sources are safe to print.
            eprintln!(
                "Could not publish {post} (code: {}). Check channel permissions and connectivity.",
                code.map_or_else(|| "unknown".into(), |code| code.to_string())
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    fn channel(id: u64, kind: u8) -> Channel {
        serde_json::from_value(json!({"id": id.to_string(), "guild_id": "1", "type": kind}))
            .unwrap()
    }

    fn message() -> Message {
        serde_json::from_value(json!({
            "id": "3", "channel_id": "2", "guild_id": "1", "type": 0,
            "author": {"id": "4", "username": "person", "discriminator": "0", "avatar": null},
            "content": "", "attachments": [], "embeds": [], "mentions": [], "mention_roles": [],
            "mention_everyone": false, "pinned": false, "tts": false,
            "timestamp": "2026-01-01T00:00:00.000000+00:00"
        }))
        .unwrap()
    }

    fn guild(channels: Value) -> Event {
        let guild = serde_json::from_value(json!({
            "id": "1", "name": "Test", "owner_id": "4", "afk_timeout": 300,
            "default_message_notifications": 0, "emojis": [], "explicit_content_filter": 0,
            "features": [], "large": false, "mfa_level": 0, "nsfw_level": 0,
            "preferred_locale": "en-US", "premium_progress_bar_enabled": false,
            "roles": [], "system_channel_flags": 0, "verification_level": 0, "channels": channels
        }))
        .unwrap();
        Event::GuildCreate(Box::new(GuildCreate::Available(guild)))
    }

    #[test]
    fn accepts_people_bots_webhooks_and_replies_without_content() {
        let mut channels = AnnouncementChannels::default();
        channels.set(&channel(2, 5), Some(Id::new(1)));
        let mut post = message();
        assert!(channels.publication(&post).is_some());
        post.author.bot = true;
        assert!(channels.publication(&post).is_some());
        post.webhook_id = Some(Id::new(5));
        assert!(channels.publication(&post).is_some());
        post.kind = MessageType::Reply;
        assert!(channels.publication(&post).is_some());
    }

    #[test]
    fn skips_other_channels_system_messages_and_crossposts() {
        let mut channels = AnnouncementChannels::default();
        for kind in [0, 1, 10, 11, 12] {
            channels.set(&channel(2, kind), Some(Id::new(1)));
            assert!(channels.publication(&message()).is_none());
        }
        channels.set(&channel(2, 5), Some(Id::new(1)));
        for flags in [
            MessageFlags::CROSSPOSTED,
            MessageFlags::IS_CROSSPOST,
            MessageFlags::CROSSPOSTED | MessageFlags::IS_CROSSPOST,
        ] {
            let mut post = message();
            post.flags = Some(flags);
            assert!(channels.publication(&post).is_none());
        }
        let mut post = message();
        post.kind = MessageType::ChannelMessagePinned;
        assert!(channels.publication(&post).is_none());
        post = message();
        post.guild_id = None;
        assert!(channels.publication(&post).is_none());
    }

    #[test]
    fn tracks_initial_channels_and_replaces_state_on_guild_recovery() {
        let mut channels = AnnouncementChannels::default();
        // Guild payload channels can omit guild_id; the surrounding guild supplies it.
        channels.update(&guild(
            json!([{"id": "2", "type": 5}, {"id": "6", "type": 0}]),
        ));
        assert!(channels.publication(&message()).is_some());
        assert_eq!(channels.0.len(), 1);
        channels.update(&guild(json!([{"id": "7", "type": 5}])));
        assert!(channels.publication(&message()).is_none());
        assert_eq!(channels.0.len(), 1);
        channels.update(&Event::GuildDelete(
            serde_json::from_value(json!({"id": "1", "unavailable": true})).unwrap(),
        ));
        assert!(channels.0.is_empty());
    }

    #[test]
    fn tracks_channel_creation_conversion_and_deletion() {
        use twilight_model::gateway::payload::incoming::{
            ChannelCreate, ChannelDelete, ChannelUpdate,
        };
        let mut channels = AnnouncementChannels::default();
        channels.update(&Event::ChannelCreate(Box::new(ChannelCreate(channel(
            2, 5,
        )))));
        assert!(channels.publication(&message()).is_some());
        channels.update(&Event::ChannelUpdate(Box::new(ChannelUpdate(channel(
            2, 0,
        )))));
        assert!(channels.publication(&message()).is_none());
        channels.update(&Event::ChannelUpdate(Box::new(ChannelUpdate(channel(
            2, 5,
        )))));
        assert!(channels.publication(&message()).is_some());
        channels.update(&Event::ChannelDelete(Box::new(ChannelDelete(channel(
            2, 5,
        )))));
        assert!(channels.0.is_empty());
    }

    #[tokio::test]
    async fn publish_uses_correct_endpoint_and_continues_after_failures() {
        use std::time::Duration;
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let _ = rustls::crypto::ring::default_provider().install_default();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            for (status, body) in [
                (
                    "403 Forbidden",
                    r#"{"code":50013,"message":"Missing Permissions"}"#,
                ),
                (
                    "400 Bad Request",
                    r#"{"code":40033,"message":"Already crossposted"}"#,
                ),
                ("200 OK", "{}"),
            ] {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut request = Vec::new();
                loop {
                    let mut buffer = [0; 1024];
                    let count = stream.read(&mut buffer).await.unwrap();
                    assert!(count > 0);
                    request.extend_from_slice(&buffer[..count]);
                    if request.windows(4).any(|chunk| chunk == b"\r\n\r\n") {
                        break;
                    }
                }
                let request = String::from_utf8(request).unwrap();
                assert!(
                    request.starts_with("POST /api/v10/channels/2/messages/3/crosspost HTTP/1.1"),
                    "{request}"
                );
                let response = format!(
                    "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
                stream.write_all(response.as_bytes()).await.unwrap();
            }
        });
        let http = Client::builder()
            .proxy(address.to_string(), true)
            .ratelimiter(None)
            .timeout(Duration::from_secs(2))
            .build();
        let post = Publication {
            guild_id: Id::new(1),
            channel_id: Id::new(2),
            message_id: Id::new(3),
        };
        for _ in 0..3 {
            publish(&http, post).await;
        }
        tokio::time::timeout(Duration::from_secs(5), server)
            .await
            .unwrap()
            .unwrap();
    }
}
